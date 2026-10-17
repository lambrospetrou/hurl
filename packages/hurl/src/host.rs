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
//! Host platform used when Hurl is built without libcurl (feature `curl` disabled).
//!
//! The embedder (for instance a WebAssembly host like Cloudflare Workers) registers a [`Host`]
//! with [`set_host`] before running Hurl files. HTTP requests, clock, sleep and file access then
//! go through it.
use std::cell::RefCell;
use std::io;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use chrono::{DateTime, Utc};

/// An HTTP request sent by the host. Redirections are not followed by the host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub timeout: Duration,
}

/// An HTTP response received by the host. The body is not decoded (`Content-Encoding`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostResponse {
    pub status: u32,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Platform services used by Hurl without libcurl.
pub trait Host {
    /// Sends an HTTP request, without following redirection.
    fn send(&self, request: &HostRequest) -> Result<HostResponse, String>;
    /// Returns the current time.
    fn now(&self) -> DateTime<Utc>;
    /// Pauses the run for `duration`.
    fn sleep(&self, duration: Duration);
    /// Reads the content of the file at `path` (request bodies, `[Multipart]` files, `file,...;`
    /// predicates). `path` has already been checked against the file root.
    fn read_file(&self, path: &Path) -> io::Result<Vec<u8>> {
        let _ = path;
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
    /// Writes `bytes` to the file at `path` (`output:` option, responses of reports), creating
    /// the file if needed. If `append` is false, an existing file is truncated.
    fn write_file(&self, path: &Path, bytes: &[u8], append: bool) -> io::Result<()> {
        let _ = (path, bytes, append);
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}

thread_local! {
    static HOST: RefCell<Option<Rc<dyn Host>>> = const { RefCell::new(None) };
}

/// Registers the host used by this thread.
pub fn set_host(host: Rc<dyn Host>) {
    HOST.with(|h| *h.borrow_mut() = Some(host));
}

fn host() -> Rc<dyn Host> {
    let host = HOST.with(|h| h.borrow().clone());
    #[cfg(test)]
    let host = host.or_else(|| Some(Rc::new(tests::SystemHost)));
    host.expect("hurl::host::set_host must be called before running Hurl without libcurl")
}

pub(crate) fn send(request: &HostRequest) -> Result<HostResponse, String> {
    host().send(request)
}

pub(crate) fn now() -> DateTime<Utc> {
    host().now()
}

pub(crate) fn sleep(duration: Duration) {
    host().sleep(duration);
}

pub(crate) fn read_file(path: &Path) -> io::Result<Vec<u8>> {
    host().read_file(path)
}

pub(crate) fn write_file(path: &Path, bytes: &[u8], append: bool) -> io::Result<()> {
    host().write_file(path, bytes, append)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host with the system clock and no network, for unit tests.
    pub struct SystemHost;

    impl Host for SystemHost {
        fn send(&self, _request: &HostRequest) -> Result<HostResponse, String> {
            Err("no network in unit tests".to_string())
        }

        fn now(&self) -> DateTime<Utc> {
            Utc::now()
        }

        fn sleep(&self, duration: Duration) {
            std::thread::sleep(duration);
        }

        fn read_file(&self, path: &Path) -> io::Result<Vec<u8>> {
            std::fs::read(path)
        }
    }
}
