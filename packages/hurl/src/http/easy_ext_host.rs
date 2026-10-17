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
//! The types of `easy_ext.rs` used without libcurl (feature `curl` disabled).

/// A certificate, see `easy_ext.rs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertInfo {
    pub data: Vec<String>,
    pub value: Option<Pem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pem(String);

impl Pem {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
