//! Features of a Hurl file that `fetch` can not do in Workers.

use hurl_core::ast::{
    BooleanOption, EntryOption, HurlFile, OptionKind, Query, QueryValue, SectionValue, SourceInfo,
    VersionValue,
};
use serde::{Deserialize, Serialize};

/// What a run does with the unsupported features of a file.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// The run fails before sending any request.
    #[default]
    Error,
    /// The options are ignored, and `HTTP/x` in a status line matches any version.
    Ignore,
}

#[derive(Debug, Serialize)]
pub struct Unsupported {
    pub entry: usize,
    pub line: usize,
    pub column: usize,
    pub feature: String,
    pub message: String,
}

/// Returns the unsupported features of `file`, and the HTTP versions of its status lines that
/// `fetch` can not check.
pub fn find(file: &HurlFile) -> (Vec<Unsupported>, Vec<SourceInfo>) {
    let mut found = vec![];
    let mut versions = vec![];
    for (index, entry) in file.entries.iter().enumerate() {
        let entry_index = index + 1;
        let mut add = |source_info: SourceInfo, feature: String, reason: &str| {
            found.push(Unsupported {
                entry: entry_index,
                line: source_info.start.line,
                column: source_info.start.column,
                message: format!("{feature} is not supported in Workers: {reason}"),
                feature,
            });
        };
        let mut sections = entry.request.sections.iter().collect::<Vec<_>>();
        if let Some(response) = &entry.response {
            sections.extend(response.sections.iter());
            let version = &response.version;
            let value = version.value;
            if matches!(
                value,
                VersionValue::Version1 | VersionValue::Version2 | VersionValue::Version3
            ) {
                add(
                    version.source_info,
                    format!("`{value}`"),
                    "fetch does not report the HTTP version, use `HTTP`",
                );
                versions.push(version.source_info);
            }
        }
        for section in sections {
            match &section.value {
                SectionValue::Options(options) => {
                    for option in options {
                        if let Some(reason) = option_reason(option) {
                            add(
                                option.space0.source_info,
                                format!("option `{}`", option.kind.identifier()),
                                reason,
                            );
                        }
                    }
                }
                SectionValue::Captures(captures) => {
                    for capture in captures {
                        if let Some((name, reason)) = query_reason(&capture.query) {
                            add(capture.query.source_info, name, reason);
                        }
                    }
                }
                SectionValue::Asserts(asserts) => {
                    for assert in asserts {
                        if let Some((name, reason)) = query_reason(&assert.query) {
                            add(assert.query.source_info, name, reason);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    found.sort_by_key(|u| (u.line, u.column));
    (found, versions)
}

fn option_reason(option: &EntryOption) -> Option<&'static str> {
    let enabled = |value: &BooleanOption| !matches!(value, BooleanOption::Literal(false));
    match &option.kind {
        OptionKind::Http10(v)
        | OptionKind::Http11(v)
        | OptionKind::Http2(v)
        | OptionKind::Http2PriorKnowledge(v)
        | OptionKind::Http3(v)
            if enabled(v) =>
        {
            Some("fetch does not let you choose the HTTP version")
        }
        OptionKind::IpV4(v) | OptionKind::IpV6(v) if enabled(v) => {
            Some("fetch does not let you choose the IP version")
        }
        OptionKind::Digest(v)
        | OptionKind::Insecure(v)
        | OptionKind::Negotiate(v)
        | OptionKind::NetRc(v)
        | OptionKind::NetRcOptional(v)
        | OptionKind::Ntlm(v)
        | OptionKind::PathAsIs(v)
            if enabled(v) =>
        {
            Some("libcurl only, not available with fetch")
        }
        OptionKind::AwsSigV4(_)
        | OptionKind::CaCertificate(_)
        | OptionKind::ClientCert(_)
        | OptionKind::ClientKey(_)
        | OptionKind::ConnectTo(_)
        | OptionKind::ConnectTimeout(_)
        | OptionKind::LimitRate(_)
        | OptionKind::NetRcFile(_)
        | OptionKind::PinnedPublicKey(_)
        | OptionKind::Proxy(_)
        | OptionKind::Resolve(_)
        | OptionKind::UnixSocket(_) => Some("libcurl only, not available with fetch"),
        _ => None,
    }
}

fn query_reason(query: &Query) -> Option<(String, &'static str)> {
    match &query.value {
        QueryValue::Ip => Some((
            "query `ip`".to_string(),
            "fetch does not report the remote IP address",
        )),
        QueryValue::Certificate { .. } => Some((
            "query `certificate`".to_string(),
            "fetch does not report the TLS certificate",
        )),
        _ => None,
    }
}

/// Replaces the `/x` of the HTTP versions at `versions` with spaces, so that they match any
/// version while the positions of the file stay the same.
pub fn any_version(content: &str, versions: &[SourceInfo]) -> String {
    if versions.is_empty() {
        return content.to_string();
    }
    content
        .split_inclusive('\n')
        .enumerate()
        .map(|(index, line)| {
            let line_number = index + 1;
            line.chars()
                .enumerate()
                .map(|(i, c)| {
                    let column = i + 1;
                    let blank = versions.iter().any(|v| {
                        v.start.line == line_number
                            && column >= v.start.column + "HTTP".len()
                            && column < v.end.column
                    });
                    if blank { ' ' } else { c }
                })
                .collect::<String>()
        })
        .collect()
}
