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
//! File access, from the filesystem or from the host platform (see `crate::host`).
use std::io;
use std::path::Path;

/// Reads the whole content of the file at `path`.
pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    #[cfg(feature = "curl")]
    {
        std::fs::read(path)
    }
    #[cfg(not(feature = "curl"))]
    {
        crate::host::read_file(path)
    }
}

/// Writes `bytes` to the file at `path`, creating it if needed. If `append` is false, an existing
/// file is truncated.
pub fn write(path: &Path, bytes: &[u8], append: bool) -> io::Result<()> {
    #[cfg(feature = "curl")]
    {
        use std::io::Write;
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(path)?
            .write_all(bytes)
    }
    #[cfg(not(feature = "curl"))]
    {
        crate::host::write_file(path, bytes, append)
    }
}
