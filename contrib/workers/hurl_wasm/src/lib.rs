//! Hurl runner for WebAssembly hosts like Cloudflare Workers.
//!
//! The module exports `alloc`, `dealloc`, `hurl_parse`, `hurl_fmt` and `hurl_run`, and imports the
//! `hurl_host` functions below. `hurl_run` is synchronous for Rust: the host makes the async
//! imports (`http_send`, `sleep_ms`, `fs_read`, `fs_write`) suspend the WebAssembly stack with JSPI
//! (`WebAssembly.Suspending` and `WebAssembly.promising`).
//!
//! Strings go in and out as UTF-8 JSON. A returned `u64` packs a pointer (high 32 bits) and a
//! length (low 32 bits); the host frees it with `dealloc`.
use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, Utc};
use hurl::host::{Host, HostRequest, HostResponse};
use hurl::http::{CredentialForwarding, FollowLocation, Header, HeaderVec};
use hurl::runner::{self, Input, Number, RunnerOptionsBuilder, Value, VariableSet};
use hurl::util::logger::LoggerOptionsBuilder;
use hurl::util::path::ContextDir;
use hurl::util::redacted::Redact;
use hurl_core::error::{DisplaySourceError, OutputFormat};
use hurl_core::types::Count;
use serde::{Deserialize, Serialize};
use serde_json::json;

mod unsupported;

#[link(wasm_import_module = "hurl_host")]
unsafe extern "C" {
    /// Sends the JSON request at `ptr` and returns the length of the JSON response (async).
    fn http_send(ptr: *const u8, len: usize) -> usize;
    /// Reads the file of the JSON request at `ptr` and returns the length of the JSON response
    /// (async).
    fn fs_read(ptr: *const u8, len: usize) -> usize;
    /// Writes the file of the JSON request at `ptr` and returns the length of the JSON response
    /// (async).
    fn fs_write(ptr: *const u8, len: usize) -> usize;
    /// Copies the last JSON response (of `http_send`, `fs_read` or `fs_write`) to `ptr`.
    fn take_response(ptr: *mut u8);
    /// Returns the current time in milliseconds since the Unix epoch.
    fn now_ms() -> f64;
    /// Waits `ms` milliseconds (async).
    fn sleep_ms(ms: f64);
    /// Fills `len` bytes at `ptr` with random values.
    fn random_fill(ptr: *mut u8, len: usize);
}

#[derive(Serialize)]
struct JsRequest<'a> {
    method: &'a str,
    url: &'a str,
    headers: &'a [(String, String)],
    body: String,
    timeout_ms: u128,
}

#[derive(Deserialize)]
struct JsResponse {
    status: Option<u32>,
    #[serde(default)]
    headers: Vec<(String, String)>,
    #[serde(default)]
    body: String,
    error: Option<String>,
}

#[derive(Serialize)]
struct JsFileRequest {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    append: Option<bool>,
}

#[derive(Deserialize)]
struct JsFileResponse {
    #[serde(default)]
    body: String,
    error: Option<String>,
    #[serde(default)]
    not_found: bool,
}

/// Calls an import that takes a JSON request and returns the length of a JSON response.
fn call_json<T: Serialize, R: for<'de> Deserialize<'de>>(
    import: unsafe extern "C" fn(*const u8, usize) -> usize,
    request: &T,
) -> Result<R, String> {
    let data = serde_json::to_vec(request).map_err(|e| e.to_string())?;
    let buf = unsafe {
        let len = import(data.as_ptr(), data.len());
        let mut buf = vec![0; len];
        take_response(buf.as_mut_ptr());
        buf
    };
    serde_json::from_slice(&buf).map_err(|e| e.to_string())
}

fn file_call(
    import: unsafe extern "C" fn(*const u8, usize) -> usize,
    request: &JsFileRequest,
) -> io::Result<Vec<u8>> {
    let response: JsFileResponse = call_json(import, request).map_err(io::Error::other)?;
    if response.not_found {
        return Err(io::Error::from(io::ErrorKind::NotFound));
    }
    if let Some(error) = response.error {
        return Err(io::Error::other(error));
    }
    STANDARD.decode(response.body).map_err(io::Error::other)
}

struct WasmHost;

impl Host for WasmHost {
    fn send(&self, request: &HostRequest) -> Result<HostResponse, String> {
        let js_request = JsRequest {
            method: &request.method,
            url: &request.url,
            headers: &request.headers,
            body: STANDARD.encode(&request.body),
            timeout_ms: request.timeout.as_millis(),
        };
        let response: JsResponse = call_json(http_send, &js_request)?;
        if let Some(error) = response.error {
            return Err(error);
        }
        let body = STANDARD.decode(response.body).map_err(|e| e.to_string())?;
        Ok(HostResponse {
            status: response.status.unwrap_or_default(),
            headers: response.headers,
            body,
        })
    }

    fn now(&self) -> DateTime<Utc> {
        let ms = unsafe { now_ms() } as i64;
        DateTime::from_timestamp_millis(ms).unwrap_or_default()
    }

    fn sleep(&self, duration: Duration) {
        unsafe { sleep_ms(duration.as_millis() as f64) }
    }

    fn read_file(&self, path: &Path) -> io::Result<Vec<u8>> {
        let request = JsFileRequest {
            path: path.to_string_lossy().into_owned(),
            body: None,
            append: None,
        };
        file_call(fs_read, &request)
    }

    fn write_file(&self, path: &Path, bytes: &[u8], append: bool) -> io::Result<()> {
        let request = JsFileRequest {
            path: path.to_string_lossy().into_owned(),
            body: Some(STANDARD.encode(bytes)),
            append: Some(append),
        };
        file_call(fs_write, &request).map(|_| ())
    }
}

/// Provides randomness to `getrandom` (used for UUID generation) from the host.
///
/// # Safety
///
/// `dest` must be valid for writes of `len` bytes.
#[unsafe(no_mangle)]
unsafe extern "Rust" fn __getrandom_v03_custom(
    dest: *mut u8,
    len: usize,
) -> Result<(), getrandom::Error> {
    unsafe { random_fill(dest, len) };
    Ok(())
}

#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// # Safety
///
/// `ptr` and `len` must come from `alloc` or from a value returned by an export.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    unsafe { drop(Vec::from_raw_parts(ptr, 0, len)) };
}

/// Reads and frees an input string written by the host with `alloc`.
unsafe fn read_input(ptr: *mut u8, len: usize) -> String {
    let bytes = unsafe { Vec::from_raw_parts(ptr, len, len) };
    String::from_utf8_lossy(&bytes).into_owned()
}

fn write_output(value: serde_json::Value) -> u64 {
    let bytes = value.to_string().into_bytes().into_boxed_slice();
    let len = bytes.len() as u64;
    let ptr = Box::into_raw(bytes) as *mut u8 as u64;
    (ptr << 32) | len
}

/// Parses a Hurl file. Input: the Hurl file content. Output: `{"ok": true, "entries": n}` or
/// `{"ok": false, "error": {"line", "column", "message"}}`.
///
/// # Safety
///
/// `ptr` and `len` must come from `alloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hurl_parse(ptr: *mut u8, len: usize) -> u64 {
    let content = unsafe { read_input(ptr, len) };
    write_output(parse(&content))
}

pub fn parse(content: &str) -> serde_json::Value {
    match hurl_core::parser::parse_hurl_file(content) {
        Ok(file) => json!({ "ok": true, "entries": file.entries.len() }),
        Err(error) => parse_error(content, &error),
    }
}

fn parse_error(content: &str, error: &hurl_core::parser::ParseError) -> serde_json::Value {
    let pos = error.source_info().start;
    let message = error.render("", content, None, OutputFormat::Plain);
    json!({
        "ok": false,
        "error": {
            "line": pos.line,
            "column": pos.column,
            "description": error.description(),
            "message": message,
        },
    })
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum FmtInputFormat {
    #[default]
    Hurl,
    Curl,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum FmtOutputFormat {
    #[default]
    Hurl,
    Json,
    Html,
}

/// Options of `hurlfmt`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FmtInput {
    content: String,
    /// `--in`
    #[serde(default)]
    input_format: FmtInputFormat,
    /// `--out`
    #[serde(default)]
    output_format: FmtOutputFormat,
    /// `--standalone`
    #[serde(default)]
    standalone: bool,
}

/// Formats or exports a Hurl file like `hurlfmt`. Input: `{"content", "inputFormat"?,
/// "outputFormat"?, "standalone"?}`.
///
/// # Safety
///
/// `ptr` and `len` must come from `alloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hurl_fmt(ptr: *mut u8, len: usize) -> u64 {
    let input = unsafe { read_input(ptr, len) };
    let output = match serde_json::from_str::<FmtInput>(&input) {
        Ok(input) => fmt(&input),
        Err(e) => json!({ "ok": false, "error": { "message": format!("invalid input: {e}") } }),
    };
    write_output(output)
}

fn fmt(input: &FmtInput) -> serde_json::Value {
    let content = match input.input_format {
        FmtInputFormat::Hurl => input.content.clone(),
        FmtInputFormat::Curl => match hurlfmt::curl::parse(&input.content) {
            Ok(content) => content,
            Err(e) => return json!({ "ok": false, "error": { "message": e } }),
        },
    };
    let file = match hurl_core::parser::parse_hurl_file(&content) {
        Ok(file) => file,
        Err(error) => return parse_error(&content, &error),
    };
    let output = match input.output_format {
        FmtOutputFormat::Hurl => hurlfmt::linter::lint_hurl_file(&file),
        FmtOutputFormat::Json => hurlfmt::format::format_json(&file),
        FmtOutputFormat::Html => hurl_core::format::format_html(&file, input.standalone),
    };
    // Like `hurlfmt --check`: the Hurl file is already formatted.
    let formatted = matches!(input.input_format, FmtInputFormat::Hurl)
        .then(|| hurlfmt::linter::lint_hurl_file(&file) == input.content);
    json!({ "ok": true, "output": output, "formatted": formatted })
}

#[derive(Deserialize)]
struct RunInput {
    content: String,
    #[serde(default)]
    filename: Option<String>,
    #[serde(default)]
    variables: BTreeMap<String, serde_json::Value>,
    /// Secret variables, like `--secret`: their values are redacted from the report and errors.
    #[serde(default)]
    secrets: BTreeMap<String, String>,
    #[serde(default)]
    options: RunOptions,
    /// Directory where the report saves the response bodies, like the `store` directory of
    /// `--report-json`.
    #[serde(default)]
    response_dir: Option<String>,
    /// What to do with the features that `fetch` can not do.
    #[serde(default)]
    unsupported: unsupported::Mode,
}

/// Returns `dir` if it is a relative path inside the file root.
fn response_dir(dir: &str) -> Result<PathBuf, String> {
    let path = Path::new(dir);
    let normal = path.components().all(|c| matches!(c, Component::Normal(_)));
    if dir.is_empty() || !normal {
        return Err(format!(
            "invalid response directory <{dir}>, it must be a relative path inside the file root"
        ));
    }
    Ok(path.to_path_buf())
}

/// Options of a run, like the Hurl CLI options with the same names. The `[Options]` section of
/// an entry overrides them.
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct RunOptions {
    /// `--location`
    location: bool,
    /// `--location-trusted`
    location_trusted: bool,
    /// `--max-redirs` (-1 for no limit)
    max_redirs: Option<i64>,
    /// `--retry` (-1 for no limit)
    retry: Option<i64>,
    /// `--retry-interval`
    retry_interval_ms: Option<u64>,
    /// `--delay`
    delay_ms: Option<u64>,
    /// `--max-time`
    max_time_ms: Option<u64>,
    /// `--user` (`user:password`)
    user: Option<String>,
    /// `--user-agent`
    user_agent: Option<String>,
    /// `--header` (`Name: value`)
    headers: Vec<String>,
    /// `--compressed`
    compressed: bool,
    /// `--continue-on-error`
    continue_on_error: bool,
    /// `--ignore-asserts`
    ignore_asserts: bool,
    /// `--from-entry`
    from_entry: Option<usize>,
    /// `--to-entry`
    to_entry: Option<usize>,
    /// `--repeat` (-1 for no limit)
    repeat: Option<i64>,
    /// `--max-filesize`
    max_filesize: Option<u64>,
    /// Size in bytes of the request and response bodies kept for the report (not a CLI option).
    max_retained_body_size: Option<u64>,
}

fn count(value: i64) -> Count {
    if value < 0 {
        Count::Infinite
    } else {
        Count::Finite(value as usize)
    }
}

fn runner_options(options: &RunOptions) -> Result<RunnerOptionsBuilder, String> {
    // Files are resolved relative to the host file root: paths outside it (`../secret`,
    // `/etc/passwd`) are refused by Hurl before reaching the host. `/hurl` is only a virtual
    // working directory for this check; the host receives the relative path.
    let context_dir = ContextDir::new(Path::new("/hurl"), Path::new(""));
    let mut headers = HeaderVec::new();
    for header in &options.headers {
        match Header::parse(header) {
            Some(header) => headers.push(header),
            None => return Err(format!("invalid header <{header}>, missing `:`")),
        }
    }
    let follow_location = if options.location_trusted {
        FollowLocation::Follow(CredentialForwarding::AllHosts)
    } else if options.location {
        FollowLocation::Follow(CredentialForwarding::OnlyInitialHost)
    } else {
        FollowLocation::No
    };
    let mut builder = RunnerOptionsBuilder::new();
    builder
        .context_dir(&context_dir)
        .follow_location(follow_location)
        .headers(headers)
        .user(options.user.clone())
        .user_agent(options.user_agent.clone())
        .compressed(options.compressed)
        .continue_on_error(options.continue_on_error)
        .no_assert(options.ignore_asserts)
        .from_entry(options.from_entry)
        .to_entry(options.to_entry)
        .repeat(options.repeat.map(count))
        .max_filesize(options.max_filesize)
        .max_retained_body_size(options.max_retained_body_size)
        .retry(options.retry.map(count));
    if let Some(max_redirs) = options.max_redirs {
        builder.max_redirect(count(max_redirs));
    }
    if let Some(ms) = options.retry_interval_ms {
        builder.retry_interval(Duration::from_millis(ms));
    }
    if let Some(ms) = options.delay_ms {
        builder.delay(Duration::from_millis(ms));
    }
    if let Some(ms) = options.max_time_ms {
        builder.timeout(Duration::from_millis(ms));
    }
    Ok(builder)
}

/// Runs a Hurl file. Input: `{"content", "filename"?, "variables"?, "secrets"?, "options"?, "response_dir"?}`. Output:
/// `{"ok": true, "success", "result", "errors"}` where `result` is the Hurl JSON report of the
/// file and `errors` the rendered errors of the entries (failed asserts, file access, HTTP), or
/// `{"ok": false, "error"}`.
///
/// # Safety
///
/// `ptr` and `len` must come from `alloc`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hurl_run(ptr: *mut u8, len: usize) -> u64 {
    let input = unsafe { read_input(ptr, len) };
    write_output(run(&input))
}

pub fn run(input: &str) -> serde_json::Value {
    hurl::host::set_host(Rc::new(WasmHost));
    let input: RunInput = match serde_json::from_str(input) {
        Ok(i) => i,
        Err(e) => return json!({ "ok": false, "error": format!("invalid input: {e}") }),
    };
    let mut variables = VariableSet::new();
    for (name, value) in input.variables {
        let value = match value {
            serde_json::Value::String(s) => Value::String(s),
            serde_json::Value::Bool(b) => Value::Bool(b),
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Number(n) => match (n.as_i64(), n.as_f64()) {
                (Some(i), _) => Value::Number(Number::Integer(i)),
                (None, Some(f)) => Value::Number(Number::Float(f)),
                _ => Value::Number(Number::BigInteger(n.to_string())),
            },
            other => Value::String(other.to_string()),
        };
        variables.insert(name, value);
    }
    for (name, value) in input.secrets {
        variables.insert_secret(name, value);
    }
    let filename = Input::new(input.filename.as_deref().unwrap_or("input.hurl"));
    let runner_options = match runner_options(&input.options) {
        Ok(builder) => builder.build(),
        Err(e) => return json!({ "ok": false, "error": e }),
    };
    let response_dir = match input.response_dir.as_deref().map(response_dir).transpose() {
        Ok(dir) => dir,
        Err(e) => return json!({ "ok": false, "error": e }),
    };
    // A syntax error is reported by the runner.
    let (unsupported, versions) = hurl_core::parser::parse_hurl_file(&input.content)
        .map(|file| unsupported::find(&file))
        .unwrap_or_default();
    if input.unsupported == unsupported::Mode::Error && !unsupported.is_empty() {
        let error = unsupported
            .iter()
            .map(|u| format!("{filename}:{}:{}: {}", u.line, u.column, u.message))
            .collect::<Vec<_>>()
            .join("\n");
        return json!({ "ok": false, "error": error, "unsupported": unsupported });
    }
    // The report and the errors use the original content, with the same positions.
    let content = unsupported::any_version(&input.content, &versions);
    let logger_options = LoggerOptionsBuilder::new().color(false).build();
    let result = runner::run(
        &content,
        Some(&filename),
        &runner_options,
        &variables,
        &logger_options,
    );
    match result {
        Ok(result) => {
            // Like the CLI, the secrets include the values of the `redact` captures.
            let secrets = result.variables.secrets();
            let secrets = secrets.iter().map(String::as_str).collect::<Vec<_>>();
            // If a response body can not be saved, the report is returned without the bodies.
            let (json, report_error) = match result.to_json(
                &input.content,
                &filename,
                response_dir.as_deref(),
                &secrets,
            ) {
                Ok(json) => (json, None),
                Err(e) => match result.to_json(&input.content, &filename, None, &secrets) {
                    Ok(json) => (json, Some(e.to_string())),
                    Err(e) => return json!({ "ok": false, "error": e.to_string() }),
                },
            };
            {
                let errors = result
                    .entries
                    .iter()
                    .flat_map(|entry| {
                        entry.errors.iter().map(|error| {
                            let pos = error.source_info().start;
                            json!({
                                "entry": entry.entry_index.get(),
                                "line": pos.line,
                                "column": pos.column,
                                "assert": error.assert,
                                "description": error.description(),
                                "message": error
                                    .render(
                                        &filename.to_string(),
                                        &input.content,
                                        None,
                                        OutputFormat::Plain,
                                    )
                                    .redact(&secrets),
                            })
                        })
                    })
                    .collect::<Vec<_>>();
                let mut output = json!({ "ok": true, "success": result.success, "result": json, "errors": errors });
                if let Some(e) = report_error {
                    output["reportError"] = json!(e);
                }
                if !unsupported.is_empty() {
                    output["unsupported"] = json!(unsupported);
                }
                output
            }
        }
        Err(e) => json!({ "ok": false, "error": e }),
    }
}
