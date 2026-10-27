//! Stable machine results contain no localized text; human rendering uses the shared locale catalogue.

use eska_presentation::{
    arguments::{check::Format, formatting::FmtArgs},
    localization::{LocalizationValue, Localizer},
};
use eska_project_core::formatting::{
    Effective, Error, Language,
    protocol::{Edit, Request},
};
use serde::Serialize;
use serde_json::json;
use std::{
    io::{self, Read, Write},
    process::ExitCode,
};

#[derive(Serialize)]
pub struct File {
    pub path: String,
    pub language: Language,
    pub source_sha256: Option<String>,
    pub changed: bool,
    pub status: &'static str,
    pub edits: Vec<Edit>,
    pub effective: Effective,
    pub diff: Option<String>,
}

#[derive(Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub formatter_version: &'static str,
    pub mode: &'static str,
    pub files: Vec<File>,
    pub migration: Option<serde_json::Value>,
    pub error: Option<Error>,
}

impl Report {
    /// Operation mode is explicit even when preflight rejects the first file.
    pub const fn new(args: &FmtArgs) -> Self {
        Self {
            schema_version: 1,
            formatter_version: env!("CARGO_PKG_VERSION"),
            mode: if args.check() {
                "check"
            } else if args.diff() {
                "diff"
            } else if args.dump_config() {
                "dump_config"
            } else if args.migrate_config() {
                "migrate_config"
            } else if args.apply_migration() {
                "apply_migration"
            } else {
                "apply"
            },
            files: Vec::new(),
            migration: None,
            error: None,
        }
    }
}

/// JSON remains identical across locales; error diagnostics never contaminate stdout.
pub fn write(report: &Report, args: &FmtArgs, localizer: &Localizer) -> io::Result<()> {
    let mut output = io::stdout().lock();
    if matches!(args.format, Format::Json) {
        serde_json::to_writer(&mut output, report)?;
        return writeln!(output);
    }
    if args.dump_config() || args.migrate_config() || args.apply_migration() {
        serde_json::to_writer_pretty(&mut output, report)?;
        writeln!(output)?;
    } else {
        for file in &report.files {
            if let Some(diff) = &file.diff {
                write!(output, "{diff}")?;
            } else if file.changed {
                writeln!(
                    output,
                    "{}: {}",
                    file.path,
                    localizer.text(&format!("fmt-state-{}", file.status))
                )?;
            }
        }
        writeln!(
            output,
            "{}",
            localizer.format(
                "fmt-result",
                &[
                    (
                        "files",
                        LocalizationValue::Number(
                            i64::try_from(report.files.len()).unwrap_or(i64::MAX)
                        )
                    ),
                    (
                        "changed",
                        LocalizationValue::Number(
                            i64::try_from(report.files.iter().filter(|file| file.changed).count())
                                .unwrap_or(i64::MAX)
                        )
                    )
                ]
            )
        )?;
    }
    if let Some(error) = &report.error {
        eprintln!(
            "{}",
            localizer.format(
                "fmt-error",
                &[
                    ("code", LocalizationValue::Text(&error.code)),
                    ("field", LocalizationValue::Text(&error.field))
                ]
            )
        );
    }
    Ok(())
}

/// One bounded pure request is a complete transaction; errors carry the same structured formatter failure.
pub fn pure() -> ExitCode {
    let result = read_request().and_then(|request| eska_fmt::format(&request));
    let value = match &result {
        Ok(response) => serde_json::to_value(response).unwrap_or(serde_json::Value::Null),
        Err(error) => json!({"schema_version":1,"error":error}),
    };
    let mut output = io::stdout().lock();
    let written = serde_json::to_writer(&mut output, &value)
        .map_err(io::Error::other)
        .and_then(|()| output.write_all(b"\n"));
    ExitCode::from(if result.is_ok() && written.is_ok() {
        0
    } else {
        2
    })
}

/// Parsing bounds include worst-case JSON escaping while the request validates the decoded four-MiB limit.
fn read_request() -> Result<Request, Error> {
    let binary =
        std::env::current_exe().map_err(|_| Error::new("component_invalid", "eska-fmt"))?;
    eska_tool_runtime::installation::verify(&binary, "eska-fmt")
        .map_err(|_| Error::new("component_incompatible", "eska-fmt"))?;
    let mut input = Vec::new();
    io::stdin()
        .lock()
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut input)
        .map_err(|_| Error::new("request_io", "stdin"))?;
    if input.len() > 32 * 1024 * 1024 {
        return Err(Error::new("request_too_large", "stdin"));
    }
    serde_json::from_slice(&input).map_err(|_| Error::new("invalid_request", "request"))
}

/// A full-file unified preview is deterministic and independent of installed system Git or locale.
pub fn diff(path: &str, before: &str, after: &str) -> String {
    let mut output = format!(
        "--- {path}\n+++ {path}\n@@ -1,{} +1,{} @@\n",
        before.lines().count(),
        after.lines().count()
    );
    for (prefix, text) in [("-", before), ("+", after)] {
        for line in text.lines() {
            output.push_str(prefix);
            output.push_str(line);
            output.push('\n');
        }
        if !text.ends_with('\n') {
            output.push_str("\\ No newline at end of file\n");
        }
    }
    output
}
