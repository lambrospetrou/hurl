/*
 * Hurl (https://hurl.dev)
 * Copyright (C) 2026 Orange
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *          http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
 */
//! HTTP client that sends requests through the host platform (see [`crate::host`]), used when Hurl
//! is built without libcurl.
//!
//! The host sends one request and does not follow redirection. This client manages redirection,
//! request headers, form and multipart bodies, and a cookie store. Options that need a socket or
//! TLS access (proxy, client certificates, `--resolve`, `--connect-to`, HTTP version, etc...) are
//! not available.
use std::str::FromStr;

use base64::Engine;
use base64::engine::general_purpose;
use chrono::DateTime;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

use super::call::Call;
use super::cookie_store::{Cookie, CookieStore};
use super::curl_cmd::CurlCmd;
use super::debug;
use super::error::HttpError;
use super::header::{
    ACCEPT_ENCODING, AUTHORIZATION, CONTENT_TYPE, COOKIE, Header, HeaderVec, SET_COOKIE, USER_AGENT,
};
use super::ip::IpAddr;
use super::options::{ClientOptions, Verbosity};
use super::param::Param;
use super::redirect;
use super::request::Request;
use super::request_spec::{FileParam, MultipartParam, RequestSpec};
use super::response::{HttpVersion, Response};
use super::response_cookie::ResponseCookie;
use super::timings::Timings;
use super::url::Url;
use crate::host::{self, HostRequest};
use crate::runner::Output;
use crate::util::clock;
use crate::util::logger::Logger;
use crate::util::path::ContextDir;

/// Characters encoded in URL query and form parameters (like `curl_easy_escape`).
const URL_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

pub struct Client {
    cookies: Vec<Cookie>,
}

impl Client {
    /// Creates HTTP Hurl client.
    pub fn new() -> Client {
        Client { cookies: vec![] }
    }

    /// Executes an HTTP request `request_spec`, optionally follows redirection and returns a
    /// list of [`Call`].
    pub fn execute_with_redirect(
        &mut self,
        request_spec: &RequestSpec,
        options: &ClientOptions,
        logger: &mut Logger,
    ) -> Result<Vec<Call>, HttpError> {
        redirect::execute_with_redirect(request_spec, options, logger, |spec, opts, logger| {
            self.execute(spec, opts, logger)
        })
    }

    /// Executes an HTTP request `request_spec`, without following redirection.
    pub fn execute(
        &mut self,
        request_spec: &RequestSpec,
        options: &ClientOptions,
        logger: &mut Logger,
    ) -> Result<Call, HttpError> {
        let url = generate_url(&request_spec.url, &request_spec.querystring);
        let url = Url::from_str(&url)?;
        let method = request_spec.method.to_string();
        let very_verbose = options.verbosity == Some(Verbosity::VeryVerbose);

        let (body, multipart_content_type) = request_body(request_spec);
        let mut headers = self.request_headers(request_spec, &url, options);
        if let Some(content_type) = multipart_content_type
            && !request_spec.headers.contains_key(CONTENT_TYPE)
        {
            headers.retain(|h| !h.name_eq(CONTENT_TYPE));
            headers.push(Header::new(CONTENT_TYPE, &content_type));
        }

        logger.debug_method_version_out(&format!("{method} {} HTTP/1.1", url.raw()));
        for header in headers.iter() {
            logger.debug_header_out(&header.name, &header.value);
        }
        logger.debug_header_out_end();
        if very_verbose {
            logger.debug_important("Request body:");
            debug::log_body(&body, &headers, true, logger);
        }

        let host_request = HostRequest {
            method: method.clone(),
            url: url.raw(),
            headers: headers
                .iter()
                .map(|h| (h.name.clone(), h.value.clone()))
                .collect(),
            body: body.clone(),
            timeout: options.timeout,
        };
        let start = clock::now();
        let host_response = host::send(&host_request).map_err(HttpError::Host)?;
        let end = clock::now();
        let duration = (end - start).to_std().unwrap_or_default();

        if let Some(max_filesize) = options.max_filesize
            && host_response.body.len() as u64 > max_filesize
        {
            return Err(HttpError::AllowedResponseSizeExceeded(max_filesize));
        }

        let mut response_headers = HeaderVec::new();
        for (name, value) in &host_response.headers {
            response_headers.push(Header::new(name, value));
        }
        logger.debug_important("Response:");
        logger.debug("");
        logger.debug_status_version_in(&format!("HTTP/1.1 {}", host_response.status));
        for header in response_headers.iter() {
            logger.debug_header_in(&header.name, &header.value);
        }
        logger.debug_header_in_end();

        if options.use_cookie_store {
            self.store_cookies(&url, &response_headers, logger);
        }

        let length = host_response.body.len();
        let request = Request::new(&method, url.clone(), headers, body);
        let response = Response::new(
            HttpVersion::Http11,
            host_response.status,
            response_headers,
            host_response.body,
            duration,
            url,
            None,
            IpAddr::new(String::new()),
        );
        let timings = Timings {
            begin_call: start,
            end_call: end,
            total: duration,
            ..Default::default()
        };
        if options.verbosity.is_some() {
            let duration = duration.as_millis() as u64;
            logger.debug(&format!("Received {length} bytes in {duration} ms"));
            if very_verbose {
                logger.debug("");
                logger.debug_important("Response body:");
                response.log_body(true, logger);
            }
        }
        Ok(Call {
            request,
            response,
            timings,
        })
    }

    /// Returns the headers sent for `request_spec`: user headers, implicit content type, user
    /// agent, basic authentication, compression and cookies.
    fn request_headers(
        &self,
        request_spec: &RequestSpec,
        url: &Url,
        options: &ClientOptions,
    ) -> HeaderVec {
        let mut headers = request_spec.headers.clone();
        headers.extend(&options.headers);

        if !headers.contains_key(CONTENT_TYPE)
            && let Some(content_type) = &request_spec.implicit_content_type
        {
            headers.push(Header::new(CONTENT_TYPE, content_type));
        }
        if !headers.contains_key(USER_AGENT) {
            let user_agent = match &options.user_agent {
                Some(u) => u.clone(),
                None => format!("hurl/{}", env!("CARGO_PKG_VERSION")),
            };
            headers.push(Header::new(USER_AGENT, &user_agent));
        }
        if let Some(user) = &options.user
            && !headers.contains_key(AUTHORIZATION)
        {
            let authorization = general_purpose::STANDARD.encode(user.as_bytes());
            headers.push(Header::new(
                AUTHORIZATION,
                &format!("Basic {authorization}"),
            ));
        }
        if options.compressed && !headers.contains_key(ACCEPT_ENCODING) {
            headers.push(Header::new(ACCEPT_ENCODING, "gzip, deflate, br"));
        }

        let mut cookies = vec![];
        if options.use_cookie_store {
            let now = clock::now().timestamp() as u64;
            for cookie in &self.cookies {
                if cookie_match(cookie, url, now) {
                    cookies.push(format!("{}={}", cookie.name(), cookie.value()));
                }
            }
        }
        cookies.extend(request_spec.cookies.iter().map(|c| c.to_string()));
        if !cookies.is_empty() {
            headers.push(Header::new(COOKIE, &cookies.join("; ")));
        }

        headers.retain(|h| !options.no_headers.iter().any(|name| h.name_eq(name)));
        headers
    }

    /// Saves the cookies of the `Set-Cookie` response headers of a request to `url`.
    fn store_cookies(&mut self, url: &Url, headers: &HeaderVec, logger: &mut Logger) {
        let now = clock::now().timestamp();
        for header in headers.get_all(SET_COOKIE) {
            let Some(cookie) = ResponseCookie::parse(&header.value) else {
                logger.warning(&format!("Can not parse cookie <{}>", header.value));
                continue;
            };
            let host = url.host().to_lowercase();
            let (domain, include_subdomain) = match cookie.domain() {
                Some(d) => {
                    let d = d.trim_start_matches('.').to_lowercase();
                    if host != d && !host.ends_with(&format!(".{d}")) {
                        logger.warning(&format!(
                            "Cookie <{}> rejected: invalid domain",
                            cookie.name
                        ));
                        continue;
                    }
                    (d, true)
                }
                None => (host, false),
            };
            let path = match cookie.path() {
                Some(p) if p.starts_with('/') => p,
                _ => default_path(url.path()),
            };
            let expires = if let Some(max_age) = cookie.max_age() {
                Some(now.saturating_add(max_age))
            } else {
                cookie.expires().map(|e| {
                    DateTime::parse_from_rfc2822(&e)
                        .map(|d| d.timestamp())
                        .unwrap_or(0)
                })
            };
            self.cookies
                .retain(|c| !(c.name() == cookie.name && c.domain() == domain && c.path() == path));
            let expires = match expires {
                Some(e) if e <= now => continue,
                Some(e) => e as u64,
                None => 0,
            };
            let line = format!(
                "{}{domain}\t{}\t{path}\t{}\t{expires}\t{}\t{}",
                if cookie.has_httponly() {
                    "#HttpOnly_"
                } else {
                    ""
                },
                bool_str(include_subdomain),
                bool_str(cookie.has_secure()),
                cookie.name,
                cookie.value
            );
            match Cookie::from_netscape(&line) {
                Ok(c) => self.cookies.push(c),
                Err(_) => logger.warning(&format!("Can not store cookie <{line}>")),
            }
        }
    }

    /// Returns cookie store.
    pub fn cookie_store(&mut self, logger: &mut Logger) -> CookieStore {
        let mut cookie_store = CookieStore::new();
        for cookie in &self.cookies {
            let line = cookie.to_netscape();
            if cookie_store.add_cookie(&line).is_err() {
                logger.warning(&format!("Line <{line}> can not be parsed as cookie"));
            }
        }
        cookie_store
    }

    /// Adds a cookie to the cookie jar (experimental).
    pub fn add_cookie(&mut self, cookie: &Cookie, logger: &mut Logger) {
        logger.debug(&format!("Add to cookie store <{cookie}> (experimental)"));
        self.cookies.retain(|c| {
            !(c.name() == cookie.name()
                && c.domain() == cookie.domain()
                && c.path() == cookie.path())
        });
        self.cookies.push(cookie.clone());
    }

    /// Clears cookie storage (experimental).
    pub fn clear_cookie_storage(&mut self, logger: &mut Logger) {
        logger.debug("Clear cookie storage (experimental)");
        self.cookies.clear();
    }

    /// Returns curl command-line for the HTTP `request_spec` run by this client.
    pub fn curl_command_line(
        &mut self,
        request_spec: &RequestSpec,
        context_dir: &ContextDir,
        output: Option<&Output>,
        options: &ClientOptions,
        logger: &mut Logger,
    ) -> CurlCmd {
        let cookies = self.cookie_store(logger);
        CurlCmd::new(request_spec, &cookies, context_dir, output, options)
    }
}

fn bool_str(value: bool) -> &'static str {
    if value { "TRUE" } else { "FALSE" }
}

/// Returns `true` if `cookie` must be sent to `url` at `now` (seconds since epoch).
fn cookie_match(cookie: &Cookie, url: &Url, now: u64) -> bool {
    if cookie.expires() != 0 && cookie.expires() <= now {
        return false;
    }
    if cookie.https() && url.scheme() != "https" {
        return false;
    }
    let host = url.host().to_lowercase();
    let domain = cookie.domain().trim_start_matches('.');
    let domain_match =
        host == domain || (cookie.include_subdomain() && host.ends_with(&format!(".{domain}")));
    if !domain_match {
        return false;
    }
    let path = url.path();
    let cookie_path = cookie.path();
    path == cookie_path
        || (path.starts_with(cookie_path)
            && (cookie_path.ends_with('/') || path[cookie_path.len()..].starts_with('/')))
}

/// Returns the default cookie path of a request path (RFC 6265, section 5.1.4).
fn default_path(path: &str) -> String {
    match path.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(i) => path[..i].to_string(),
    }
}

fn url_encode(value: &str) -> String {
    utf8_percent_encode(value, URL_ENCODE_SET).to_string()
}

fn url_encode_params(params: &[Param]) -> String {
    params
        .iter()
        .map(|p| format!("{}={}", p.name, url_encode(&p.value)))
        .collect::<Vec<String>>()
        .join("&")
}

/// Generates URL with query parameters.
fn generate_url(url: &Url, params: &[Param]) -> String {
    let url = url.raw();
    if params.is_empty() {
        return url;
    }
    let url = if url.ends_with('?') {
        url
    } else if url.contains('?') {
        format!("{url}&")
    } else {
        format!("{url}?")
    };
    format!("{url}{}", url_encode_params(params))
}

/// Returns the bytes of the request body, and the content type of a multipart body.
fn request_body(request_spec: &RequestSpec) -> (Vec<u8>, Option<String>) {
    if !request_spec.form.is_empty() {
        return (url_encode_params(&request_spec.form).into_bytes(), None);
    }
    if !request_spec.multipart.is_empty() {
        let boundary = format!("------------------------{}", uuid::Uuid::new_v4().simple());
        let mut body = vec![];
        for param in &request_spec.multipart {
            body.extend(format!("--{boundary}\r\n").as_bytes());
            match param {
                MultipartParam::Param(Param { name, value }) => {
                    body.extend(
                        format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n")
                            .as_bytes(),
                    );
                    body.extend(value.as_bytes());
                }
                MultipartParam::FileParam(FileParam {
                    name,
                    filename,
                    data,
                    content_type,
                }) => {
                    body.extend(
                        format!(
                            "Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
                        )
                        .as_bytes(),
                    );
                    body.extend(data);
                }
            }
            body.extend(b"\r\n");
        }
        body.extend(format!("--{boundary}--\r\n").as_bytes());
        let content_type = format!("multipart/form-data; boundary={boundary}");
        return (body, Some(content_type));
    }
    (request_spec.body.bytes(), None)
}

impl Header {
    /// Parses an HTTP header line, see `client.rs`.
    pub fn parse(line: &str) -> Option<Header> {
        match line.find(':') {
            Some(index) => {
                let (name, value) = line.split_at(index);
                Some(Header::new(name.trim(), value[1..].trim()))
            }
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_path() {
        assert_eq!(default_path(""), "/");
        assert_eq!(default_path("/"), "/");
        assert_eq!(default_path("/foo"), "/");
        assert_eq!(default_path("/foo/bar"), "/foo");
    }

    #[test]
    fn test_url_encode() {
        assert_eq!(url_encode("a b&c=d~é"), "a%20b%26c%3Dd~%C3%A9");
    }
}
