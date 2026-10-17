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
//! Clock and sleep, from the system or from the host platform (see `crate::host`).
use std::time::Duration;

use chrono::{DateTime, Utc};

/// Returns the current time.
pub fn now() -> DateTime<Utc> {
    #[cfg(feature = "curl")]
    {
        Utc::now()
    }
    #[cfg(not(feature = "curl"))]
    {
        crate::host::now()
    }
}

/// Pauses the current run for `duration`.
pub fn sleep(duration: Duration) {
    #[cfg(feature = "curl")]
    std::thread::sleep(duration);
    #[cfg(not(feature = "curl"))]
    crate::host::sleep(duration);
}

/// Measures elapsed time.
pub struct Stopwatch {
    #[cfg(feature = "curl")]
    start: std::time::Instant,
    #[cfg(not(feature = "curl"))]
    start: DateTime<Utc>,
}

impl Stopwatch {
    /// Starts a new stopwatch.
    pub fn start() -> Self {
        #[cfg(feature = "curl")]
        let start = std::time::Instant::now();
        #[cfg(not(feature = "curl"))]
        let start = now();
        Stopwatch { start }
    }

    /// Returns the time elapsed since the start of this stopwatch.
    pub fn elapsed(&self) -> Duration {
        #[cfg(feature = "curl")]
        {
            self.start.elapsed()
        }
        #[cfg(not(feature = "curl"))]
        {
            (now() - self.start).to_std().unwrap_or_default()
        }
    }
}
