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
use std::path::PathBuf;
use std::{fmt, io};

use hurl_core::ast::SourceInfo;

use crate::util::path::ContextDir;
use crate::util::term::Stdout;

use super::error::{RunnerError, RunnerErrorKind};

/// Represents the output of write operation: can be either a file or standard output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Output {
    /// Write to file.
    File(PathBuf),
    /// Write to standard output.
    Stdout,
}

impl fmt::Display for Output {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let output = match self {
            Output::File(file) => file.to_string_lossy().to_string(),
            Output::Stdout => "-".to_string(),
        };
        write!(f, "{output}")
    }
}

impl Output {
    /// Creates a new output from a string filename.
    pub fn new(filename: &str) -> Self {
        if filename == "-" {
            Output::Stdout
        } else {
            Output::File(PathBuf::from(filename))
        }
    }

    /// Consume an output and returns a new output given a `context_dir`.
    ///
    /// If the output is a file, and the file is not allowed to write given this `context_dir`, this
    /// method returns an error.
    pub fn try_with(
        self,
        context_dir: &ContextDir,
        source_info: SourceInfo,
    ) -> Result<Self, RunnerError> {
        let output = match self {
            Output::Stdout => Output::Stdout,
            Output::File(filename) => {
                if !context_dir.is_access_allowed(&filename) {
                    let kind = RunnerErrorKind::UnauthorizedFileAccess { path: filename };
                    let error = RunnerError::new(source_info, kind, false);
                    return Err(error);
                }
                let path = context_dir.resolved_path(&filename);
                Output::File(path)
            }
        };
        Ok(output)
    }

    /// Writes these `bytes` to the output.
    ///
    /// If output is a standard output variant, `stdout` is used to write the bytes. If `append`
    /// is true, the output file is created in append mode, else any existing file will be truncated
    /// before writing data.
    pub fn write(&self, bytes: &[u8], stdout: &mut Stdout, append: bool) -> Result<(), io::Error> {
        match self {
            Output::Stdout => stdout.write_all(bytes).map_err(|e| {
                let filename = "stdout".to_string();
                let message = format!("{filename} can not be written ({e})");
                io::Error::other(message)
            }),
            Output::File(filename) => {
                crate::util::fs::write(filename, bytes, append).map_err(|e| {
                    let filename = filename.display().to_string();
                    let message = format!("{filename} can not be written ({e})");
                    io::Error::other(message)
                })
            }
        }
    }
}
