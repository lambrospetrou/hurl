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
//! Bounds the size of the request and response bodies kept in a run result, see
//! [`super::RunnerOptionsBuilder::max_retained_body_size`].
use super::EntryResult;

/// Keeps the bodies of `entry` if they fit in `max_size` with the already `retained` bodies,
/// otherwise drops them.
pub(super) fn retain_bodies(entry: &mut EntryResult, max_size: Option<u64>, retained: &mut u64) {
    let Some(max_size) = max_size else {
        return;
    };
    let size = entry
        .calls
        .iter()
        .map(|call| (call.request.body.len() + call.response.body.len()) as u64)
        .sum::<u64>();
    if *retained + size <= max_size {
        *retained += size;
        return;
    }
    for call in &mut entry.calls {
        call.request.body = Vec::new();
        call.response.body = Vec::new();
    }
    entry.bodies_dropped = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::Call;

    fn entry_with_body(body: &[u8]) -> EntryResult {
        let url: crate::http::Url = "http://localhost".parse().unwrap();
        let headers = crate::http::HeaderVec::new;
        let response = crate::http::Response {
            version: crate::http::HttpVersion::Http11,
            status: 200,
            headers: headers(),
            body: body.to_vec(),
            duration: Default::default(),
            url: url.clone(),
            certificate: None,
            ip_addr: Default::default(),
        };
        EntryResult {
            calls: vec![Call {
                request: crate::http::Request::new("POST", url, headers(), b"req".to_vec()),
                response,
                timings: Default::default(),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn retain_bodies_until_max_size() {
        let mut retained = 0;
        let mut first = entry_with_body(b"0123456");
        retain_bodies(&mut first, Some(13), &mut retained);
        assert!(!first.bodies_dropped);
        assert_eq!(first.calls[0].response.body, b"0123456");
        assert_eq!(retained, 10);

        let mut second = entry_with_body(b"0123456");
        retain_bodies(&mut second, Some(13), &mut retained);
        assert!(second.bodies_dropped);
        assert!(second.calls[0].request.body.is_empty());
        assert!(second.calls[0].response.body.is_empty());
        assert_eq!(retained, 10);

        let mut third = entry_with_body(b"");
        retain_bodies(&mut third, Some(13), &mut retained);
        assert!(!third.bodies_dropped);
        assert_eq!(retained, 13);

        let mut all = entry_with_body(b"0123456");
        retain_bodies(&mut all, None, &mut retained);
        assert!(!all.bodies_dropped);
    }
}
