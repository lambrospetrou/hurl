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
//! Redirection for the host HTTP client. It is a copy of the redirection of `client.rs`, so that
//! `client.rs` stays unchanged.
use hurl_core::types::Count;

use super::call::Call;
use super::error::HttpError;
use super::header::LOCATION;
use super::options::ClientOptions;
use super::request::{CredentialForwarding, FollowLocation};
use super::request_spec::{Body, Method, RequestSpec};
use super::response::Response;
use super::url::Url;
use crate::util::logger::Logger;

/// Executes an HTTP request `request_spec` with `execute`, optionally following redirection, and
/// returns a list of [`Call`].
pub(crate) fn execute_with_redirect<F>(
    request_spec: &RequestSpec,
    options: &ClientOptions,
    logger: &mut Logger,
    mut execute: F,
) -> Result<Vec<Call>, HttpError>
where
    F: FnMut(&RequestSpec, &ClientOptions, &mut Logger) -> Result<Call, HttpError>,
{
    let mut calls = vec![];

    let original_url = &request_spec.url;
    let mut request_spec = request_spec.clone();
    let mut options = options.clone();

    // We keep a pristine copy of the original request's credentials-related headers/cookies
    // and of the original options' headers/user. On each redirect hop, credentials are
    // stripped or restored based on a comparison between `original_url` and the redirect
    // target, so a chain like host A -> host B -> host A forwards credentials again once
    // back on the original host, mirroring libcurl's behaviour.
    let original_headers = request_spec.headers.clone();
    let original_cookies = request_spec.cookies.clone();
    let original_options_headers = options.headers.clone();
    let original_options_user = options.user.clone();

    // Unfortunately, follow-location feature from libcurl can not be used as libcurl returns a
    // single list of headers for the 2 responses and Hurl needs to keep every header of every
    // response.
    let mut redirect_count = 0;
    loop {
        let call = execute(&request_spec, &options, logger)?;
        // If we don't follow redirection, we can early exit here.
        if matches!(options.follow_location, FollowLocation::No) {
            calls.push(call);
            break;
        }
        let request_url = call.request.url.clone();
        let status = call.response.status;
        let redirect_url = follow_location(&request_url, &call.response)?;
        calls.push(call);
        if redirect_url.is_none() {
            break;
        }
        let redirect_url = redirect_url.unwrap();
        logger.debug("");
        logger.debug(&format!("=> Redirect to {redirect_url}"));
        logger.debug("");
        redirect_count += 1;
        if let Count::Finite(max_redirect) = options.max_redirect
            && redirect_count > max_redirect
        {
            return Err(HttpError::TooManyRedirect);
        };

        let redirect_method = redirect_method(status, &request_spec.method);

        // When following redirection, we filter `Authorization` and `Cookie` headers if the
        // hostname changes unless the user explicitly trusts the redirected host with `--location-trusted`.
        // <https://curl.se/libcurl/c/CURLOPT_FOLLOWLOCATION.html>:
        //
        // > By default, libcurl only sends Authentication: or explicitly set Cookie: headers
        // > to the initial host given in the original URL, to avoid leaking username + password
        // > to other sites.
        //
        // We always recompute headers, cookies, user (everything that can have credentials)
        // from original request rather than from the previous redirection step. A redirect chain
        // A -> B -> A will forward credentials on the final step.
        let mut headers = original_headers.clone();
        let mut cookies = original_cookies.clone();
        options.headers = original_options_headers.clone();
        options.user = original_options_user.clone();
        if should_strip_credentials_on_redirect(
            original_url,
            &redirect_url,
            options.follow_location,
        ) {
            headers.remove_credentials();
            cookies.clear();
            options.headers.remove_credentials();
            options.user = None;
        }

        // If the request method has changed due to redirection, the body is dropped from the
        // request, otherwise we keep it. We follow libcurl implementation <https://curl.se/libcurl/c/CURLOPT_FOLLOWLOCATION.html>:
        //
        // > When libcurl switches method to GET, it then uses that method without sending any
        // > request body. If it does not change the method, it sends the subsequent request the
        // > same way as the previous one; including the request body if one was provided.
        let (form, multipart, body, implicit_content_type) =
            if redirect_method != request_spec.method {
                (vec![], vec![], Body::Binary(vec![]), None)
            } else {
                (
                    request_spec.form,
                    request_spec.multipart,
                    request_spec.body,
                    request_spec.implicit_content_type,
                )
            };
        request_spec = RequestSpec {
            method: redirect_method,
            url: redirect_url,
            headers,
            querystring: vec![],
            form,
            multipart,
            cookies,
            body,
            implicit_content_type,
        };
    }
    Ok(calls)
}

/// Retrieves an optional location to follow
///
/// You need:
/// 1. the option follow_location set to true
/// 2. a 3xx response code
/// 3. a header Location
fn follow_location(request_url: &Url, response: &Response) -> Result<Option<Url>, HttpError> {
    let response_code = response.status;
    if !(300..400).contains(&response_code) {
        return Ok(None);
    }
    let Some(location) = response.headers.get(LOCATION) else {
        return Ok(None);
    };
    let url = request_url.join(&location.value)?;
    Ok(Some(url))
}

/// Tests if credentials (`Authorization:`, `Cookie:` headers) should be filtered when there is a redirection
/// from the first `original_url` to `redirect_url`.
fn should_strip_credentials_on_redirect(
    original_url: &Url,
    redirect_url: &Url,
    follow_location: FollowLocation,
) -> bool {
    if matches!(
        follow_location,
        FollowLocation::Follow(CredentialForwarding::AllHosts)
    ) {
        return false;
    }
    // Different origin != strip credentials
    if original_url.scheme() != redirect_url.scheme() {
        return true;
    }
    if original_url.host() != redirect_url.host() {
        return true;
    }
    // Treat different ports as different origins
    original_url.port() != redirect_url.port()
}

/// Returns the method used for redirecting a request/response with `response_status`.
fn redirect_method(response_status: u32, original_method: &Method) -> Method {
    // This replicates curl's behavior
    match response_status {
        301..=303 => Method("GET".to_string()),
        // Could be only 307 and 308, but curl does this for all 3xx
        // codes not converted to GET above.
        _ => original_method.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_redirect_method() {
        // Status of the response to be redirected | method of the original request | method of the new request
        let data = [
            (301, "GET", "GET"),
            (301, "POST", "GET"),
            (301, "DELETE", "GET"),
            (302, "GET", "GET"),
            (302, "POST", "GET"),
            (302, "DELETE", "GET"),
            (303, "GET", "GET"),
            (303, "POST", "GET"),
            (303, "DELETE", "GET"),
            (304, "GET", "GET"),
            (304, "POST", "POST"),
            (304, "DELETE", "DELETE"),
            (308, "GET", "GET"),
            (308, "POST", "POST"),
            (308, "DELETE", "DELETE"),
        ];
        for (status, original, redirected) in data {
            assert_eq!(
                redirect_method(status, &Method(original.to_string())),
                Method(redirected.to_string())
            );
        }
    }

    #[test]
    fn test_should_strip_credentials_on_redirect() {
        let url1 = Url::from_str("http://example.com").unwrap();
        let url2 = Url::from_str("http://example.com:8080").unwrap();
        let url3 = Url::from_str("https://example.com").unwrap();
        let url4 = Url::from_str("http://other.com").unwrap();

        let follow_location = FollowLocation::Follow(CredentialForwarding::OnlyInitialHost);
        assert!(should_strip_credentials_on_redirect(
            &url1,
            &url2,
            follow_location
        ));
        assert!(should_strip_credentials_on_redirect(
            &url1,
            &url3,
            follow_location
        ));
        assert!(should_strip_credentials_on_redirect(
            &url1,
            &url4,
            follow_location
        ));
        assert!(should_strip_credentials_on_redirect(
            &url1,
            &url3,
            follow_location
        ));

        let follow_location = FollowLocation::Follow(CredentialForwarding::AllHosts);
        assert!(!should_strip_credentials_on_redirect(
            &url1,
            &url2,
            follow_location
        ));
        assert!(!should_strip_credentials_on_redirect(
            &url1,
            &url3,
            follow_location
        ));
        assert!(!should_strip_credentials_on_redirect(
            &url1,
            &url4,
            follow_location
        ));
        assert!(!should_strip_credentials_on_redirect(
            &url1,
            &url3,
            follow_location
        ));
    }
}
