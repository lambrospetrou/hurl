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
//! XPath support when Hurl is built without libxml2: every XPath evaluation is unsupported.
use super::value::Value;

/// An error for XPath evaluation.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum XPathError {
    #[allow(dead_code)]
    Eval,
    Unsupported,
}

/// A parsed HTML or XML document. Without libxml2, a document can not be parsed.
#[derive(Clone)]
pub struct Document {
    _private: (),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Format {
    Html,
    Xml,
}

impl Document {
    /// Always fails: parsing HTML or XML requires libxml2 (feature `libxml`).
    pub fn parse(_data: &str, _format: Format) -> Result<Document, String> {
        Err("XPath is not supported in this build of Hurl (feature `libxml` disabled)".to_string())
    }

    /// Always fails with [`XPathError::Unsupported`].
    pub fn eval_xpath(&self, _expr: &str) -> Result<Value, XPathError> {
        Err(XPathError::Unsupported)
    }
}
