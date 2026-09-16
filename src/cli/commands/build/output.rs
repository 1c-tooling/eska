//! Build previews, streamed diagnostics and result presentation.

use std::{
    fmt::Write as _,
    io::{self, IsTerminal, Write as _},
    path::Path,
    process::ExitCode,
};

use gix::bstr::ByteSlice;

use crate::{
    cli::{
        changes,
        localization::{LocalizationValue, Localizer},
    },
    project::{
        Project,
        build::{self, BuildPlan, Ibcmd, ToolSource},
        metadata,
    },
};

use super::{
    OutputFormat, PreparedBuild,
    json::{BuildDocument, BuildPlanDocument},
};

pub(super) mod progress;
use progress::ProgressLine;

/// Present a fully preflighted plan without starting any build stage.
pub(super) fn write_build_preview(
    format: OutputFormat,
    prepared: &[PreparedBuild<'_>],
    tools: &[Ibcmd],
    aggregate: bool,
    localizer: &Localizer,
) -> ExitCode {
    match format {
        OutputFormat::Human => write_human_build_preview(prepared, tools, aggregate, localizer),
        OutputFormat::Json => {
            let document = BuildPlanDocument::new(prepared, tools, aggregate);
            let Ok(json) = serde_json::to_string_pretty(&document) else {
                eprintln!("{}", localizer.text("build-json-error"));
                return ExitCode::FAILURE;
            };
            println!("{json}");
        }
    }
    ExitCode::SUCCESS
}

/// Show the scope and ordered members of a preflighted build.
fn write_human_build_preview(
    prepared: &[PreparedBuild<'_>],
    tools: &[Ibcmd],
    aggregate: bool,
    localizer: &Localizer,
) {
    println!("{}", localizer.text("build-preview-title"));
    println!(
        "{}",
        localizer.text(if aggregate {
            "build-preview-scope-workspace"
        } else {
            "build-preview-scope-project"
        })
    );
    println!(
        "{}",
        localizer.format(
            "build-preview-projects",
            &[(
                "projects",
                LocalizationValue::Number(i64::try_from(prepared.len()).unwrap_or(i64::MAX)),
            )],
        )
    );
    for (index, (item, tool)) in prepared.iter().zip(tools).enumerate() {
        write_project_build_preview(index, item, tool, localizer);
    }
}

/// Render one already validated project from an aggregate or standalone preview.
fn write_project_build_preview(
    index: usize,
    item: &PreparedBuild<'_>,
    tool: &Ibcmd,
    localizer: &Localizer,
) {
    let position = i64::try_from(index + 1).unwrap_or(i64::MAX);
    let name = item.name.map_or_else(
        || project_display_name(item.project),
        |name| name.as_str().to_owned(),
    );
    println!(
        "{}",
        localizer.format(
            "build-preview-project",
            &[
                ("position", LocalizationValue::Number(position)),
                ("name", LocalizationValue::Text(&name)),
            ],
        )
    );
    write_build_preview_field(
        "build-preview-root",
        &display_path(item.plan.project_root()),
        localizer,
    );
    write_build_preview_field(
        "build-preview-source",
        &display_path(item.plan.source()),
        localizer,
    );
    if let Some(path) = item.plan.base_configuration() {
        write_build_preview_field(
            "build-preview-base-configuration",
            &display_path(path),
            localizer,
        );
    }
    write_build_preview_field(
        "build-preview-infobase",
        &display_path(item.plan.infobase_root()),
        localizer,
    );
    write_build_preview_field(
        "build-preview-infobase-mode",
        &localizer.text(if item.plan.recreates_infobase() {
            "build-preview-infobase-recreate"
        } else {
            "build-preview-infobase-reuse"
        }),
        localizer,
    );
    write_build_preview_field(
        "build-preview-artifact-type",
        &localizer.text(artifact_type_key(item.plan.artifact_type())),
        localizer,
    );
    write_build_preview_field(
        "build-preview-artifact-path",
        &display_path(item.plan.output()),
        localizer,
    );
    write_build_preview_field(
        "build-preview-replaces-existing",
        &localizer.text(if item.plan.output().is_file() {
            "build-preview-yes"
        } else {
            "build-preview-no"
        }),
        localizer,
    );
    write_build_preview_field(
        "build-preview-required-platform",
        item.plan.platform_version().as_str(),
        localizer,
    );
    write_build_preview_field(
        "build-preview-found-platform",
        tool.version().as_str(),
        localizer,
    );
    write_build_preview_field(
        "build-preview-runner",
        &localizer.text(runner_key(tool.source())),
        localizer,
    );
}

/// Print one localized field without changing its value.
fn write_build_preview_field(key: &str, value: &str, localizer: &Localizer) {
    println!(
        "{}",
        localizer.format(key, &[("value", LocalizationValue::Text(value))])
    );
}

/// Use the project directory name when no workspace member name is available.
fn project_display_name(project: &Project) -> String {
    project
        .root()
        .file_name()
        .unwrap_or_else(|| project.root().as_os_str())
        .to_string_lossy()
        .into_owned()
}

/// Select the human label for the planned artifact type.
const fn artifact_type_key(artifact_type: build::ArtifactType) -> &'static str {
    match artifact_type {
        build::ArtifactType::Configuration => "build-preview-type-configuration",
        build::ArtifactType::Extension => "build-preview-type-extension",
        build::ArtifactType::Processing => "build-preview-type-processing",
        build::ArtifactType::Report => "build-preview-type-report",
    }
}

/// Select the human label for a host or container platform runner.
const fn runner_key(source: &ToolSource) -> &'static str {
    match source {
        ToolSource::Explicit(_) | ToolSource::Path(_) | ToolSource::Standard(_) => {
            "build-preview-runner-host"
        }
        ToolSource::Distrobox { .. } => "build-preview-runner-distrobox",
    }
}

/// Render and flush one ibcmd line as soon as it reaches the CLI layer.
pub(super) fn write_diagnostic(
    line: &[u8],
    project: &Project,
    localizer: &Localizer,
    styled: bool,
    progress: Option<&ProgressLine>,
) -> io::Result<()> {
    let Ok(line) = std::str::from_utf8(line) else {
        return write_diagnostic_bytes(line, progress);
    };
    let line = humanize_source_paths(line, project, localizer);
    let line = style_diagnostic_level(&line, styled);
    write_diagnostic_bytes(line.as_bytes(), progress)
}

/// Write one diagnostic while keeping an interactive progress line at the bottom.
fn write_diagnostic_bytes(line: &[u8], progress: Option<&ProgressLine>) -> io::Result<()> {
    if let Some(progress) = progress {
        return progress.write_diagnostic(line);
    }
    let mut stderr = io::stderr().lock();
    stderr.write_all(line)?;
    if !line.ends_with(b"\n") {
        stderr.write_all(b"\n")?;
    }
    stderr.flush()
}

/// Replace absolute source files with their existing Configurator-style ownership.
fn humanize_source_paths(message: &str, project: &Project, localizer: &Localizer) -> String {
    let source = project.source();
    let source_text = source.to_string_lossy();
    let mut rendered = String::with_capacity(message.len());
    let mut remaining = message;
    while let Some(position) = remaining.find(source_text.as_ref()) {
        rendered.push_str(&remaining[..position]);
        let after_source = &remaining[position + source_text.len()..];
        let Some((relative, consumed)) = existing_source_path(after_source, source) else {
            rendered.push_str(&source_text);
            remaining = after_source;
            continue;
        };
        let normalized = relative.replace('\\', "/");
        let display = metadata::from_path(
            project.configuration().project_type(),
            normalized.as_bytes().as_bstr(),
        )
        .map_or_else(
            || normalized.clone(),
            |path| {
                let owner = changes::render_metadata_path(&path, localizer);
                diagnostic_path_tail(&normalized)
                    .map_or_else(|| owner.clone(), |tail| format!("{owner} · {tail}"))
            },
        );
        rendered.push_str(&display);
        remaining = &after_source[consumed..];
    }
    rendered.push_str(remaining);
    rendered
}

/// Retain the concrete help payload below its concise logical metadata owner.
fn diagnostic_path_tail(relative: &str) -> Option<&str> {
    relative
        .find("/Ext/Help/")
        .map(|position| &relative[position + 1..])
}

/// Find the longest existing path immediately below the configured source directory.
fn existing_source_path<'a>(suffix: &'a str, source: &Path) -> Option<(&'a str, usize)> {
    let separator_length = if suffix.starts_with('/') || suffix.starts_with('\\') {
        1
    } else {
        return None;
    };
    let path = &suffix[separator_length..];
    path.char_indices()
        .map(|(index, _)| index)
        .chain([path.len()])
        .rev()
        .find_map(|end| {
            let relative = &path[..end];
            (!relative.is_empty() && source.join(relative).exists())
                .then_some((relative, separator_length + end))
        })
}

/// Highlight a recognized ibcmd severity prefix while keeping its message unchanged.
fn style_diagnostic_level(message: &str, styled: bool) -> String {
    if !styled {
        return message.to_owned();
    }
    for (level, color) in [
        ("[TRACE]", "90"),
        ("[DEBUG]", "34"),
        ("[INFO]", "36"),
        ("[WARN]", "33"),
        ("[ERROR]", "31"),
        ("[FATAL]", "35"),
    ] {
        if let Some(rest) = message.strip_prefix(level) {
            return format!("\x1b[1;{color}m{level}\x1b[0m{rest}");
        }
    }
    message.to_owned()
}

/// Enable diagnostic colors only for an interactive stderr that permits color.
pub(super) fn diagnostic_styling_enabled() -> bool {
    io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
}

/// Enable success styling only when its stdout destination is interactive.
fn result_styling_enabled() -> bool {
    io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
}

/// Print the stable build heading before the first tool stage starts.
pub(super) fn write_build_started(
    version: &str,
    localizer: &Localizer,
    styled: bool,
) -> io::Result<()> {
    let message = localizer.format(
        "build-started",
        &[("version", LocalizationValue::Text(version))],
    );
    let message = decorate_status("▶", &message, styled, "36");
    let mut stderr = io::stderr().lock();
    writeln!(stderr, "{message}")?;
    stderr.flush()
}

/// Add a stable marker and optionally color only that marker.
fn decorate_status(marker: &str, message: &str, styled: bool, color: &str) -> String {
    if styled {
        format!("\x1b[1;{color}m{marker}\x1b[0m {message}")
    } else {
        format!("{marker} {message}")
    }
}

/// Present the successful build without changing the JSON result contract.
pub(super) fn write_build_result(
    format: OutputFormat,
    plan: &BuildPlan,
    result: &build::BuildResult,
    localizer: &Localizer,
) -> ExitCode {
    match format {
        OutputFormat::Human => {
            let interactive = io::stdout().is_terminal();
            println!(
                "{}",
                render_build_success(
                    result.output(),
                    localizer,
                    result_styling_enabled(),
                    interactive,
                )
            );
            if let Some(manifest) = result.manifest() {
                println!(
                    "{} {}",
                    localizer.text("build-manifest-completed-label"),
                    render_artifact_link(manifest, interactive),
                );
            }
        }
        OutputFormat::Json => {
            let document = BuildDocument::new(plan, result);
            let Ok(json) = serde_json::to_string_pretty(&document) else {
                eprintln!("{}", localizer.text("build-json-error"));
                return ExitCode::FAILURE;
            };
            println!("{json}");
        }
    }
    ExitCode::SUCCESS
}

/// Render a prominent label and link the visible artifact path to its directory.
fn render_build_success(
    artifact: &Path,
    localizer: &Localizer,
    styled: bool,
    hyperlink: bool,
) -> String {
    let label = localizer.text("build-completed-label");
    let label = if styled {
        format!("\x1b[1m{label}\x1b[0m")
    } else {
        label
    };
    let artifact = render_artifact_link(artifact, hyperlink);
    decorate_status("✓", &format!("{label} {artifact}"), styled, "32")
}

/// Link the artifact label to its parent directory in an interactive terminal.
fn render_artifact_link(artifact: &Path, hyperlink: bool) -> String {
    let label = display_path(artifact);
    let Some(parent) = artifact.parent().filter(|_| hyperlink) else {
        return label;
    };
    let target = file_uri(parent);
    format!("\x1b]8;;{target}\x1b\\{label}\x1b]8;;\x1b\\")
}

/// Escape control characters without making a normal filesystem path less readable.
fn display_path(path: &Path) -> String {
    let mut display = String::new();
    for character in path.to_string_lossy().chars() {
        if character.is_control() {
            display.extend(character.escape_default());
        } else {
            display.push(character);
        }
    }
    display
}

/// Encode an absolute local directory as a safe file URI for an OSC 8 target.
fn file_uri(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    let prefix = if normalized.starts_with("//") {
        "file:"
    } else if normalized.starts_with('/') {
        "file://"
    } else {
        "file:///"
    };
    format!("{prefix}{}", percent_encode_uri_path(normalized.as_bytes()))
}

/// Percent-encode bytes that are not safe inside a hierarchical file URI path.
fn percent_encode_uri_path(path: &[u8]) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':') {
            encoded.push(*byte as char);
        } else {
            write!(encoded, "%{byte:02X}").expect("writing to String cannot fail");
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::{
        decorate_status, diagnostic_path_tail, render_build_success, style_diagnostic_level,
    };
    use crate::cli::localization::{Locale, Localizer};
    use std::path::Path;

    #[test]
    /// Color only the recognized diagnostic prefix and preserve the message bytes.
    fn styles_known_diagnostic_levels() {
        for (level, color) in [
            ("TRACE", "90"),
            ("DEBUG", "34"),
            ("INFO", "36"),
            ("WARN", "33"),
            ("ERROR", "31"),
            ("FATAL", "35"),
        ] {
            let message = format!("[{level}] diagnostic\n");
            assert_eq!(
                style_diagnostic_level(&message, true),
                format!("\x1b[1;{color}m[{level}]\x1b[0m diagnostic\n")
            );
            assert_eq!(style_diagnostic_level(&message, false), message);
        }
    }

    #[test]
    /// Leave unknown prefixes unchanged even when terminal styling is enabled.
    fn preserves_unknown_diagnostic_prefixes() {
        assert_eq!(
            style_diagnostic_level("plain diagnostic\n", true),
            "plain diagnostic\n"
        );
    }

    #[test]
    /// Preserve the concrete help file without restoring its absolute source path.
    fn keeps_help_payload_tail_after_logical_owner() {
        assert_eq!(
            diagnostic_path_tail("DataProcessors/Files/Forms/AttachedFile/Ext/Help/ru.html"),
            Some("Ext/Help/ru.html")
        );
        assert_eq!(
            diagnostic_path_tail("DataProcessors/Files/Ext/ObjectModule.bsl"),
            None
        );
    }

    #[test]
    /// Keep status markers in redirected output and color only the interactive marker.
    fn decorates_build_status_without_changing_its_message() {
        assert_eq!(
            decorate_status("✓", "Built artifact", false, "32"),
            "✓ Built artifact"
        );
        assert_eq!(
            decorate_status("✓", "Built artifact", true, "32"),
            "\x1b[1;32m✓\x1b[0m Built artifact"
        );
    }

    #[test]
    /// Link the visible artifact to its directory and bold only the result label.
    fn build_result_links_to_artifact_directory() {
        let localizer = Localizer::try_new(Locale::RuRu).expect("locale");
        let artifact = Path::new("/tmp/build dir/demo.cf");
        assert_eq!(
            render_build_success(artifact, &localizer, true, true),
            concat!(
                "\x1b[1;32m✓\x1b[0m \x1b[1mСобран\x1b[0m ",
                "\x1b]8;;file:///tmp/build%20dir\x1b\\",
                "/tmp/build dir/demo.cf",
                "\x1b]8;;\x1b\\"
            )
        );
        assert_eq!(
            render_build_success(artifact, &localizer, false, false),
            "✓ Собран /tmp/build dir/demo.cf"
        );
    }
}
