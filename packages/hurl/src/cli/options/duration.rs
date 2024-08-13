/*
 * Hurl (https://hurl.dev)
 * Copyright (C) 2024 Orange
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

use hurl_core::typing::{Duration, DurationUnit};
use regex::Regex;
use std::str::FromStr;

pub fn parse(_s: &str) -> Result<Duration, String> {
    let re = Regex::new(r"^(\d+)([a-zA-Z]*)$").unwrap();
    if let Some(caps) = re.captures(_s) {
        let value = caps
            .get(1)
            .unwrap()
            .as_str()
            .to_string()
            .parse::<u64>()
            .unwrap();
        let unit = caps.get(2).unwrap().as_str();
        let unit = if unit.is_empty() {
            None
        } else {
            Some(DurationUnit::from_str(unit)?)
        };
        Ok(Duration { value, unit })
    } else {
        Err("Invalid duration".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn test_parse_error() {
        assert_eq!(parse("").unwrap_err(), "Invalid duration".to_string());
        assert_eq!(parse("s").unwrap_err(), "Invalid duration".to_string());
        assert_eq!(parse("10s10").unwrap_err(), "Invalid duration".to_string());
        assert_eq!(
            parse("10mm").unwrap_err(),
            "Invalid duration unit mm".to_string()
        );
    }

    #[test]
    pub fn test_parse() {
        assert_eq!(
            parse("10").unwrap(),
            Duration {
                value: 10,
                unit: None
            }
        );
        assert_eq!(
            parse("10s").unwrap(),
            Duration {
                value: 10,
                unit: Some(DurationUnit::Second)
            }
        );
        assert_eq!(
            parse("10000ms").unwrap(),
            Duration {
                value: 10000,
                unit: Some(DurationUnit::MilliSecond)
            }
        );
        assert_eq!(
            parse("5m").unwrap(),
            Duration {
                value: 5,
                unit: Some(DurationUnit::Minute)
            }
        );
    }
}