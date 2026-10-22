# Hurl in Cloudflare Workers (prototype)

This directory runs Hurl files inside a Cloudflare Worker, with Hurl compiled to WebAssembly.

- [`hurl_wasm`](hurl_wasm): a `cdylib` crate that builds `hurl`, `hurl_core` and `hurlfmt` for
  `wasm32-unknown-unknown` without libcurl (`--no-default-features`), but with libxml2 compiled to
  WebAssembly, for XPath.
- [`libxml2`](libxml2): [`build.sh`](libxml2/build.sh) builds libxml2 with
  [wasi-sdk](https://github.com/WebAssembly/wasi-sdk), and patches the `libxml` crate for wasm32.
- [`worker`](worker): a TypeScript Worker that loads the module, sends the HTTP requests with
  `fetch`, and tests it with the [Workers Vitest integration](https://developers.cloudflare.com/workers/testing/vitest-integration/).

```shell
cd contrib/workers/worker
npm install
npm test          # builds libxml2 and hurl_wasm, then runs the tests in local workerd
npm run typecheck
```

The build needs Rust with the `wasm32-unknown-unknown` target, `curl`, `make`,
[bindgen](https://rust-lang.github.io/rust-bindgen/command-line-usage.html)
(`cargo install bindgen-cli`) and libclang (`apt install libclang-dev`, or `LIBCLANG_PATH`). The
first build downloads wasi-sdk 25 (x86_64 Linux), libxml2 2.13.8 and the `libxml` 0.3.22 crate
into `libxml2/work/` (gitignored, checked with SHA-256) and takes about a minute; the next builds
reuse them.

To keep these tools off your machine, [`docker.sh`](docker.sh) runs the same npm scripts in a
container built from [`Dockerfile`](Dockerfile) (Node 24, Rust with the wasm32 target, bindgen,
libclang). The host only needs Docker; the outputs (`worker/src/hurl_wasm.wasm`,
`vendor/hurl-wasm/`) are written to the repository, and the Cargo target, `node_modules` and the
libxml2 build stay in Docker volumes (`hurl-workers-*`, remove them with `docker volume rm`).

```shell
contrib/workers/docker.sh            # npm run vendor
contrib/workers/docker.sh test       # npm test, in workerd inside the container
contrib/workers/docker.sh typecheck
```

## Use it in your own Worker

`npm run vendor` builds the module and copies what a Worker needs into `vendor/hurl-wasm/`
(gitignored):

```shell
rustup update stable && rustup target add wasm32-unknown-unknown
cd contrib/workers/worker
npm install
npm run vendor
```

| File             | Content                                                               |
|------------------|-----------------------------------------------------------------------|
| `hurl.ts`        | The JS API: `runHurl`, `parseHurl`, `formatHurl`, `MemoryFileSystem`. |
| `hurl_wasm.wasm` | The Hurl module, imported by `hurl.ts`.                               |
| `wasm.d.ts`      | The TypeScript type of `.wasm` imports.                               |
| `LICENSE`        | The Hurl license (Apache 2.0).                                        |
| `LICENSE-libxml2` | The libxml2 license (MIT), libxml2 is compiled in `hurl_wasm.wasm`.  |
| `COMMIT`         | The commit the files were built from.                                 |

1. Copy `vendor/hurl-wasm/` into your project, for example as `src/hurl-wasm/`. Wrangler imports
   `.wasm` files as a `WebAssembly.Module`, so no `rules` are needed.
2. In your Wrangler configuration, set the `nodejs_compat` flag (`hurl.ts` uses `node:buffer`) and
   a recent compatibility date (the tests use `2026-10-01`):

   ```jsonc
   {
     "compatibility_date": "2026-10-01",
     "compatibility_flags": ["nodejs_compat"]
   }
   ```

3. Call it:

   ```ts
   import { formatHurl, runHurl } from "./hurl-wasm/hurl";

   const result = await runHurl(content, {
     variables: { host: "https://api.example.com", retries: 3 }, // strings, numbers, booleans, null
     secrets: { api_key: env.API_KEY }, // like --secret: redacted from result and errors
     files: {
       read: async (path) => (await env.BUCKET.get(path))?.body,
       write: (path, data) => env.BUCKET.put(path, data),
     },
     maxFileSize: 8 * 1024 * 1024,
     responseDir: `runs/${runId}/store`,
     options: { retry: 3, maxRetainedBodySize: 32 * 1024 * 1024 },
     // fetcher: a function like fetch, for example a service binding or a mock in tests.
   });
   // result.success, result.errors, result.fileErrors, and result.result (the --report-json object)

   const { output, formatted } = await formatHurl(content); // hurlfmt, hurlfmt --check
   ```

`hurl.ts` and `hurl_wasm.wasm` use a private interface, so always copy them together from the same
`npm run vendor`. Run it again after each rebase of this fork.

## How it works

`hurl` has two new Cargo features, both on by default:

- `curl`: the libcurl HTTP client, and the `hurl` binary. Without it, the HTTP client
  (`http/client_host.rs`) sends each request through a `hurl::host::Host` that the embedder
  registers with `hurl::host::set_host`. The host also gives the clock, `sleep`, and file access
  (`Host::read_file` and `Host::write_file`, through `hurl::util::fs`).
- `libxml`: XPath queries and filters (`libxml2`). `hurl_core` has the same feature for the
  validation of XML request bodies; without it, `hurl_core` uses a small built-in XML scanner.

`hurl_wasm` keeps `libxml` on, with libxml2 compiled to WebAssembly by
[`libxml2/build.sh`](libxml2/build.sh), thus XPath and XML bodies work like in the CLI (same
libxml2 code):

- libxml2 2.13.8 is built as a static library with wasi-sdk (wasi-libc), without threads, zlib,
  iconv, HTTP, FTP, catalogs and modules. Without iconv is not a limit for Hurl: it decodes the
  response body to text, and gives it to libxml2 as UTF-8. `dup()`, missing in wasi-libc, is defined to fail.
- The `libxml` crate is patched ([`libxml-0.3.22-wasm.patch`](libxml2/libxml-0.3.22-wasm.patch))
  to take bindings generated for wasm32 and link the static library, and to use the error handler
  globals of a libxml2 without threads. `hurl_wasm/Cargo.toml` uses it with `[patch.crates-io]`.
- wasi-libc is linked as a WASI reactor: the Worker calls `_initialize` once per instance, and
  gives the few `wasi_snapshot_preview1` imports (stderr, clock, random; file calls fail, because
  Hurl gives the documents to libxml2 in memory).

The same HTML and XML documents and XPath expressions as the native CLI (libxml2 2.9.13) gave the
same results: 587 of 587 on a corpus of 10 web pages and 7 XML documents (feeds, sitemap, SVG).

The Hurl runner is synchronous, but `fetch` and timers are async in Workers. The module uses
[JSPI](https://v8.dev/blog/jspi): the Worker wraps the async imports with
`WebAssembly.Suspending` and the `hurl_run` export with `WebAssembly.promising`, so the Rust code
blocks while the Worker waits for `fetch`. Each run uses a new instance (own memory and cookies).

The release module is 3.7 MB (1.3 MB gzip), `hurlfmt` and libxml2 included (libxml2 adds 0.5 MB).

## What works

Tested in [`worker/test/hurl.spec.ts`](worker/test/hurl.spec.ts), in local workerd, against a
local HTTP server:

- Parsing (`hurl_core`), with syntax errors and their position, and XML bodies.
- XPath queries and filters on HTML and XML responses, with namespaces (`_:` for the default
  namespace), and the error recovery of libxml2 for broken documents.
- Requests with headers, query strings, JSON/text bodies, `[Form]`, `[Multipart]`, `[BasicAuth]`.
- Asserts and captures: status, headers, body, JSONPath, regex, `sha256`, `duration`, `url`,
  `redirects`, cookies and cookie attributes, filters, `newUuid`, `newDate`, templates and
  variables between entries.
- `[Options]`: `location` (redirects, run by Hurl, not by `fetch`), `retry`, `retry-interval`,
  `delay`, and the other options that do not need libcurl.
- Cookie storage between requests of a file.
- gzip and br responses (workerd decodes them).
- The Hurl JSON report as the result of a run, and the entry errors rendered like the CLI.
- Run-wide options, like the CLI options (`RunOptions.options`): `location`, `locationTrusted`,
  `maxRedirs`, `retry`, `retryIntervalMs`, `delayMs`, `maxTimeMs`, `user`, `userAgent`,
  `headers`, `compressed`, `continueOnError`, `ignoreAsserts`, `fromEntry`, `toEntry`, `repeat`,
  `maxFilesize`. The `[Options]` section of an entry overrides them; unknown options are refused.
- Concurrent runs: each `runHurl` has its own WebAssembly instance, and JSPI suspends it while
  it waits for `fetch`, so `Promise.all` over several runs overlaps their requests. The entries of
  one file run one after another, like the CLI.
- Files through a host file system (`RunOptions.files`, for example a `MemoryFileSystem` filled
  from R2 or KV): request bodies (`file,data.bin;`), `[Multipart]` files, `file,...;` predicates
  (`bytes == file,expected.bin;`), and the `output:` option. Paths outside the file root (`../x`,
  `/x`) are refused by Hurl before the host is called. Without `files`, file access fails.
- File size cap (`RunOptions.maxFileSize`, default 16 MiB): Hurl keeps each file in WebAssembly
  memory, thus a larger read or write fails the entry. `read(path, { maxSize })` can return a
  `Uint8Array` or a `ReadableStream` (for example the `body` of an R2 object); a stream is read
  until the cap and then canceled. The reason of a failed file call is in `RunResult.fileErrors`,
  because Hurl only reports `file x can not be read`.
- `hurlfmt`: `formatHurl(content, { inputFormat, outputFormat, standalone })` formats a Hurl file
  (`--check` is the `formatted` field), exports it to JSON or HTML, or converts curl command lines
  (`--in curl`). `hurlfmt` has a `libxml` feature too, on by default.
- JSON report: `RunResult.result` is the object that the CLI `--report-json` writes for each
  file (the CLI report is an array of these). With `RunOptions.responseDir`, the response bodies
  are saved through `files` like in the `store` directory of the CLI report. If a body can not be
  saved, `result` has no bodies and `reportError` gives the reason.
- Secrets: `RunOptions.secrets` works like `--secret name=value`. Their values, and the values of
  `redact` captures, are replaced by `***` in `result` and `errors`, like the CLI report.
- Memory: Hurl keeps the request and response bodies of all the entries for the report. The
  `maxRetainedBodySize` option (Hurl runner option `max_retained_body_size`, off by default)
  drops the bodies of the next entries once this size is reached; their asserts, captures and
  output run before.

## What does not work, and the options

| Feature | Status in Workers | Option |
| --- | --- | --- |
| `hurl` CLI binary (args, files, terminal, reports on disk) | Not built without `curl` | Call the library through `hurl_wasm`. Build reports from the JSON result. |
| libxml2 build | Needs wasi-sdk, bindgen and libclang, and a patched `libxml` crate | Update `libxml2/build.sh` and the patch with the `libxml` crate of Hurl. |
| `[Options]` for libcurl features: `http1.0`, `http1.1`, `http2`, `http3`, `ipv4`, `ipv6`, `insecure`, `cacert`, `cert`, `key`, `proxy`, `resolve`, `connect-to`, `unix-socket`, `limit-rate`, `aws-sigv4`, `ntlm`, ... | Not available in `fetch` | The run fails, or the options are ignored. See [Unsupported features](#unsupported-features). |
| `HTTP/1.0`, `HTTP/2`, `HTTP/3` in a status line | `fetch` does not report the HTTP version | Same as above. `HTTP` and `HTTP/1.1` work: hurl-wasm always reports HTTP/1.1. |
| `ip` and `certificate` queries | `fetch` does not report the remote IP or the TLS certificate | Same as above. |
| Detailed timings (DNS, connect, TLS) | Only the total time | Not supported. |
| Body on `GET`/`HEAD` | `fetch` refuses it | The Worker drops it. |
| `header "Content-Encoding"` with gzip/br | workerd decodes gzip and br bodies, so the Worker removes this header | Asserts on this header fail for gzip/br. deflate is decoded by Hurl. |
| Repeated response headers (`X-A: 1` and `X-A: 2`) | `fetch` merges them into one `1, 2` value (except `Set-Cookie`) | `header "X-A"` is one string, not a list. |
| Response header names in the report | `fetch` lowercases them | Header queries are case-insensitive, so asserts are not affected. |
| Default cookie path | Without `Path`, libcurl reports `/dir/`, the Worker `/dir` (RFC 6265) | Only the reported path differs. |
| Requests to the Worker itself / same zone, private IPs | Workers platform rules for `fetch` | Platform limit. |
| Files referenced by Hurl (`file,data.bin;`, `[Multipart]` files, `output:`) | No filesystem in the Worker | Supported through the host file system (`RunOptions.files`). `output: -` (stdout) is not returned to the Worker. |
| `--parallel`/`--jobs` (worker threads) | No threads in `wasm32-unknown-unknown` | Run files concurrently from JS (`Promise.all`), one instance per file. |
| Options for libcurl features (`--insecure`, `--cacert`, `--proxy`, `--resolve`, `--http2`, `--ntlm`, ...) | Not available in `fetch` | Not exposed in `RunOptions.options`. |
| CPU time | A run is mostly I/O wait; parsing and asserts use CPU time | Check the Workers CPU limits for large files. |

### Unsupported features

`fetch` does not let a Worker choose the HTTP version or the IP version, and it does not report
the version, the remote IP or the TLS certificate of a response. Local workerd uses HTTP/1.1. On
Cloudflare, the protocol to the origin is not documented, and it can change. hurl-wasm always
reports `HTTP/1.1` and an empty IP.

Before a run, hurl-wasm finds the features of the file that it can not do, and
`RunOptions.unsupported` selects what to do:

- `"error"` (default): the run fails before sending any request. `RunResult.error` gives the
  position of each feature (`input.hurl:3:1: option `http2` is not supported in Workers: ...`),
  and `RunResult.unsupported` lists them.
- `"ignore"`: the run continues and `RunResult.unsupported` lists the features. The options are
  ignored and `fetch` uses its defaults. `HTTP/2 200` checks only the status. `ip` is an empty
  string, and `certificate` fails the assert or capture.

Options set to `false` (`http2: false`) are not reported. Your Worker can also call
`runHurl` with `"error"` and send the files that fail with `unsupported` to the Hurl CLI.

## Compatibility suite

[`worker/compat/`](worker/compat/) runs the Hurl files of [`worker/compat/tests/`](worker/compat/tests/)
with the native Hurl CLI and with hurl-wasm in local workerd, against the same local HTTP server
([`compat/server.ts`](worker/compat/server.ts)), and compares their JSON reports:

```sh
cargo build --release --bin hurl   # from the repository root, or set HURL_BIN
cd contrib/workers/worker
npm run compat                     # COMPAT_FILTER=json npm run compat to run some files only
```

- Each file runs with the same variables (`--variable`) and secrets (`--secret`), and the server
  state is reset before each run.
- The reports are compared without the fields that change between runs or that only one build has
  (times, timings, stored bodies, client headers like `User-Agent`, multipart boundaries), with
  header names lowercased and sorted. Captures, asserts and their messages, requests, responses,
  cookies and `success` must be equal.
- Redaction: no secret, and no value captured with `redact`, may appear in either report, in the
  CLI stderr or in the hurl-wasm errors.
- The table, sorted by file name, is printed and written to `worker/compat/out/report.md`, with
  the normalized reports of each build (`<file>.cli.json`, `<file>.wasm.json`) to look at the
  differences. The times are end to end: the CLI process, and `runHurl` with its WebAssembly
  instantiation (measured by the server, since time doesn't advance in workerd during CPU work).
- To add a test, add a `.hurl` file to `worker/compat/tests/` (files it uses go in
  `tests/data/`), and endpoints to `server.ts` if needed. A file can expect a failure: both builds
  must fail the same way, like `failures.hurl` and `errors.hurl`.
- A file whose first line is `# compat: known-difference <reason>` must differ: these document
  what hurl-wasm can't match yet (`known_difference_*.hurl`). Its test fails once the builds agree.

## Native build

The default features keep the native build the same: `cargo build` and the unit tests of `hurl`
and `hurl_core` pass with libcurl and libxml2. [`bin/check/no_default_features.sh`](../../bin/check/no_default_features.sh)
checks the build without them (clippy, unit tests, and clippy for `wasm32-unknown-unknown`).
Without `libxml` (`hurl_wasm` with `default-features = false` only), XPath fails with an error and
XML request bodies are checked by the built-in scanner of `hurl_core`, which has no DTD, entity, or
encoding checks.
`npm run typecheck` generates `src/worker-configuration.d.ts` with `wrangler types`.

## Keeping the fork up to date

### Branches

| Branch | Content | How it changes |
| --- | --- | --- |
| `master` | An exact copy of upstream `Orange-OpenSource/hurl` `master`. | Only with GitHub's **Sync fork** button. Never commit or merge into it. |
| `fork/workers-wasm` | `master` and the patch commits on top. | Rebased on `master`, never merged. |

- With no commits of its own, `master` always syncs as a fast-forward, so **Sync fork** never
  conflicts.
- Do not merge the patch pull request into `master`. Keep it open to see the patch and its diff
  with upstream.
- Your Worker project does not use the fork when it builds or deploys. It has a copy of
  `vendor/hurl-wasm/`, with the fork commit in `COMMIT`. If a rebase fails, your Worker keeps the
  copy it has.

The patch is:

1. `hurl_core`, `hurl` and `hurlfmt` commits that edit upstream files and `Cargo.toml` files. Only
   these commits can conflict.
2. One commit that only adds files: `contrib/workers` and `.github/workflows/workers-wasm.yml`.

The code for builds without `curl` or `libxml` is in new files, so the edits to upstream files are
mostly `cfg` attributes and one-line calls: `hurl/src/host.rs`, `hurl/src/http/client_host.rs`,
`hurl/src/http/redirect.rs` (a copy of the redirects of `client.rs`), `hurl/src/http/easy_ext_host.rs`,
`hurl/src/runner/xpath_disabled.rs`, `hurl/src/runner/body_retention.rs`, `hurl/src/util/{clock,fs}.rs`
and `hurl_core/src/parser/xml_scanner.rs`. `http/client.rs`, `http/certificate.rs` and
`parser/xml.rs` are not changed.

### Update the fork

1. **Sync `master`.** On the GitHub page of the fork, click **Sync fork**, then **Update branch**.
2. **Rebase `fork/workers-wasm` on `master`**, with one of:
   - **GitHub:** in the patch pull request, open the menu of **Update branch** and click
     **Update with rebase**. Do not use **Update with merge commit**: it adds upstream merge commits
     to the patch, and the next rebases get harder. If GitHub reports a conflict, use the shell.
   - **Shell:**

     ```shell
     git config rerere.enabled true  # once: git records conflict resolutions and uses them again
     git fetch origin
     git switch fork/workers-wasm
     git rebase origin/master
     git range-diff @{upstream}...HEAD  # compare the patch before and after the rebase
     ```

   Each push to `fork/workers-wasm` runs the [`workers-wasm`](../../.github/workflows/workers-wasm.yml)
   workflow: clippy, the unit tests, `bin/check/no_default_features.sh`, the WASM build and the
   workerd tests. Its result shows on the pull request. If the checks pass, it uploads
   `vendor/hurl-wasm/` as the artifact `hurl-wasm-<YYYYMMDD>-<commit>` (on the page of the
   workflow run), with the commit date in UTC. GitHub disables the workflows of a fork until you
   enable them once in the *Actions* tab, and the fork must allow the actions created by GitHub.
3. **Check** (a rebase in the shell):

   ```shell
   cargo clippy --all-targets -- -D warnings
   cargo test --package hurl --package hurl_core --package hurlfmt --lib
   bin/check/no_default_features.sh
   contrib/workers/docker.sh test          # Vitest in local workerd, in Docker
   cargo build --release --bin hurl && (cd contrib/workers/worker && npm run compat)
   git push --force-with-lease
   ```

   `npm run compat` compares the reports of the native CLI and of the Worker. A file that is no
   longer compatible shows an upstream change that the Worker does not have yet.
4. **Update your Worker.** Download the `hurl-wasm-<YYYYMMDD>-<commit>` artifact of the workflow
   run, or run `contrib/workers/docker.sh` (`npm run vendor`) to build
   `contrib/workers/vendor/hurl-wasm/`. Copy it over the copy in your project, run its tests, and
   commit it.

### Conflicts

A conflict stops the rebase on one of the first commits. `git rebase --abort` puts the branch back
as it was. To resolve it:

1. `git status` lists the files. Keep the upstream change, and add the patch lines again (usually
   a `#[cfg(feature = "curl")]` or a call to `util::fs`/`util::clock` in place of `std::fs`/time).
2. `git add <files>` and `git rebase --continue`. Later commits can conflict too.
3. Run the checks of step 3. `bin/check/no_default_features.sh` finds new upstream code that uses
   libcurl, libxml2, `std::fs` or the clock without a `cfg`.

Also check upstream changes that do not conflict:

- Redirect fixes in `http/client.rs` must be copied to `http/redirect.rs` by hand.
- New `[Options]` that use libcurl must be added to `hurl_wasm/src/unsupported.rs`.
