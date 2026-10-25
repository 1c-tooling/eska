//! Accept one complete start/file/done sequence with exact file and diagnostic accounting.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, BufRead, BufReader, Read},
    path::PathBuf,
};

use serde::Deserialize;

use super::{Diagnostic, Failure, FileResult};
use crate::process::SUPPORTED_VERSION;

const MAX_LINE: u64 = 8 * 1024 * 1024;
// Large configurations can exceed 256 MiB while every individual file event remains small.
const MAX_REPORT: u64 = 512 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Event {
    Start {
        total_files: usize,
        version: String,
    },
    File {
        path: PathBuf,
        diagnostics: Vec<Diagnostic>,
        error: Option<String>,
    },
    Done {
        elapsed_secs: f64,
        total_files: usize,
        total_diagnostics: u64,
        failed_files: u64,
        baseline: Option<Baseline>,
    },
}

#[derive(Deserialize)]
struct Baseline {
    state: String,
    complete: bool,
}

/// Drain rejected stdout until the child exits; the process owner still enforces its deadline.
pub(super) fn read(
    stream: impl Read,
    expected: &BTreeSet<PathBuf>,
) -> Result<Vec<FileResult>, Failure> {
    let mut reader = BufReader::new(stream);
    let result = parse(&mut reader, expected, MAX_REPORT);
    if result.is_err() {
        let _ = io::copy(&mut reader, &mut io::sink());
    }
    result
}

/// Enforce framing, event ordering and count identities before publishing any diagnostics.
fn parse(
    reader: &mut impl BufRead,
    expected: &BTreeSet<PathBuf>,
    max_report_bytes: u64,
) -> Result<Vec<FileResult>, Failure> {
    let invalid = |reason| Failure::Stream { reason };
    let mut started = false;
    let mut done = false;
    let mut files = BTreeMap::new();
    let mut diagnostics_count = 0_u64;
    let mut bytes_count = 0_u64;
    loop {
        let mut line = Vec::new();
        let count = reader
            .take(MAX_LINE + 1)
            .read_until(b'\n', &mut line)
            .map_err(|_| invalid("read"))?;
        if count == 0 {
            break;
        }
        bytes_count += count as u64;
        if count as u64 > MAX_LINE || bytes_count > max_report_bytes {
            return Err(invalid("output_limit"));
        }
        if line.last() != Some(&b'\n') {
            return Err(invalid("truncated_line"));
        }
        let event: Event = serde_json::from_slice(&line).map_err(|_| invalid("malformed_event"))?;
        if done {
            return Err(invalid("after_done"));
        }
        match event {
            Event::Start {
                total_files,
                version,
            } if !started => {
                if total_files != expected.len() || total_files == 0 {
                    return Err(invalid("source_count"));
                }
                if version != SUPPORTED_VERSION {
                    return Err(invalid("stream_version"));
                }
                started = true;
            }
            Event::File {
                path,
                diagnostics,
                error,
            } if started => {
                if !expected.contains(&path) {
                    return Err(invalid("unexpected_file"));
                }
                if error.is_some() {
                    return Err(invalid("file_error"));
                }
                for diagnostic in &diagnostics {
                    if diagnostic.code.is_empty()
                        || diagnostic.message.is_empty()
                        || diagnostic.start_line > diagnostic.end_line
                        || (diagnostic.start_line == diagnostic.end_line
                            && diagnostic.start_column > diagnostic.end_column)
                    {
                        return Err(invalid("diagnostic"));
                    }
                }
                diagnostics_count += diagnostics.len() as u64;
                if files.insert(path, diagnostics).is_some() {
                    return Err(invalid("duplicate_file"));
                }
            }
            Event::Done {
                elapsed_secs,
                total_files,
                total_diagnostics,
                failed_files,
                baseline,
            } if started => {
                if !elapsed_secs.is_finite() || elapsed_secs < 0.0 || failed_files != 0 {
                    return Err(invalid("failed_files"));
                }
                if total_files != expected.len()
                    || files.len() != expected.len()
                    || total_diagnostics != diagnostics_count
                {
                    return Err(invalid("incomplete_counts"));
                }
                if baseline.is_some_and(|baseline| {
                    baseline.state != "disabled" && (baseline.state != "full" || !baseline.complete)
                }) {
                    return Err(invalid("partial_baseline"));
                }
                done = true;
            }
            _ => return Err(invalid("event_order")),
        }
    }
    if !done {
        return Err(invalid("missing_done"));
    }
    Ok(files
        .into_iter()
        .map(|(path, diagnostics)| FileResult { path, diagnostics })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A reference stream contains a clean file and a finding with exact upstream coordinates.
    fn events() -> Vec<serde_json::Value> {
        vec![
            json!({"type":"start","total_files":1,"version":"0.4.0"}),
            json!({"type":"file","path":"/selected/Module.bsl","diagnostics":[{"code":"Rule","message":"finding","severity":"Error","start_line":0,"start_column":2,"end_line":1,"end_column":0}]}),
            json!({"type":"done","elapsed_secs":0.1,"total_files":1,"total_diagnostics":1,"failed_files":0}),
        ]
    }

    /// Serialize independently of the reader under test.
    fn stream(events: &[serde_json::Value]) -> Vec<u8> {
        let mut output = Vec::new();
        for event in events {
            serde_json::to_writer(&mut output, event).unwrap();
            output.push(b'\n');
        }
        output
    }

    /// Current upstream Sonar severities must survive complete stream validation as public errors.
    #[test]
    fn normalizes_published_error_severities() {
        let expected = BTreeSet::from([PathBuf::from("/selected/Module.bsl")]);
        for severity in ["Blocker", "Critical", "Major", "Error"] {
            let mut values = events();
            values[1]["diagnostics"][0]["severity"] = json!(severity);
            let files = read(stream(&values).as_slice(), &expected).unwrap();
            let diagnostic = &files[0].diagnostics[0];
            assert_eq!(diagnostic.severity, super::super::Severity::Error);
            assert_eq!(
                serde_json::to_value(diagnostic).unwrap()["severity"],
                "Error"
            );
        }
    }

    /// A malformed coordinate, severity, order or trailing event can never become a clean result.
    #[test]
    fn validates_diagnostics_and_event_order() {
        let expected = BTreeSet::from([PathBuf::from("/selected/Module.bsl")]);
        let valid = events();
        assert!(read(stream(&valid).as_slice(), &expected).is_ok());
        let mut invalid = Vec::new();
        let mut values = events();
        values[1]["diagnostics"][0]["severity"] = json!("UnknownSeverity");
        invalid.push(values);
        for field in ["code", "message", "severity"] {
            let mut values = events();
            values[1]["diagnostics"][0][field] = json!("");
            invalid.push(values);
        }
        let mut values = events();
        values[1]["diagnostics"][0]["start_line"] = json!(3);
        invalid.push(values);
        let mut values = events();
        values[1]["diagnostics"][0]["end_line"] = json!(0);
        values[1]["diagnostics"][0]["end_column"] = json!(1);
        invalid.push(values);
        let mut values = events();
        values.swap(0, 1);
        invalid.push(values);
        let mut values = events();
        values.push(values[2].clone());
        invalid.push(values);
        let mut values = events();
        values.insert(1, values[0].clone());
        invalid.push(values);
        for values in invalid {
            assert!(
                read(stream(&values).as_slice(), &expected).is_err(),
                "{values:?}"
            );
        }
    }

    /// A valid final event must fit the aggregate byte budget, including every framing newline.
    #[test]
    fn enforces_total_report_budget_at_the_exact_boundary() {
        let expected = BTreeSet::from([PathBuf::from("/selected/Module.bsl")]);
        let bytes = stream(&events());
        let length = u64::try_from(bytes.len()).unwrap();
        assert!(parse(&mut BufReader::new(bytes.as_slice()), &expected, length).is_ok());
        assert!(matches!(
            parse(&mut BufReader::new(bytes.as_slice()), &expected, length - 1),
            Err(Failure::Stream {
                reason: "output_limit"
            })
        ));
    }

    /// The line bound limits allocations even if the child never emits a newline.
    #[test]
    fn rejects_oversized_and_unterminated_lines() {
        let expected = BTreeSet::from([PathBuf::from("/selected/Module.bsl")]);
        let bytes = vec![b'x'; usize::try_from(MAX_LINE + 1).unwrap()];
        assert!(matches!(
            read(bytes.as_slice(), &expected),
            Err(Failure::Stream {
                reason: "output_limit"
            })
        ));
        let mut bytes = stream(&events());
        bytes.pop();
        assert!(matches!(
            read(bytes.as_slice(), &expected),
            Err(Failure::Stream {
                reason: "truncated_line"
            })
        ));
    }
}
