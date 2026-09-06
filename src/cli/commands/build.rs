//! Localized build command with a locale-independent JSON result contract.

use std::{
    ffi::OsStr,
    fmt::Write as _,
    io::{self, IsTerminal, Write as _},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Sender},
    },
    thread,
    time::Duration,
};

use clap::{Args, ValueEnum};
use crossterm::{
    cursor::MoveToColumn,
    queue,
    terminal::{Clear, ClearType},
};
use gix::bstr::ByteSlice;
use serde::Serialize;

use crate::{
    cli::{
        diagnostics,
        localization::{LocalizationValue, Localizer},
    },
    project::{
        Project,
        build::{
            self, BuildError, BuildPlan, BuildStage, Ibcmd, PlanError, RunError, ToolError,
            ToolOptions, ToolSource,
        },
        discovery, metadata,
    },
};

use super::diff;

#[derive(Debug, Args)]
pub(in crate::cli) struct BuildArgs {
    #[arg(long)]
    output: Option<PathBuf>,

    #[arg(long)]
    ibcmd: Option<PathBuf>,

    #[arg(long)]
    platform_arch: Option<String>,

    #[arg(long)]
    distrobox: Option<String>,

    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,

    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum OutputFormat {
    #[default]
    Human,
    Json,
}

impl BuildArgs {
    /// Discover the project and exact ibcmd version, then publish one native artifact.
    pub(super) fn run(&self, project_dir: &Path, localizer: &Localizer) -> ExitCode {
        let project = match discovery::discover(project_dir) {
            Ok(project) => project,
            Err(error) => {
                eprintln!("{}", diagnostics::present_project_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let plan = match BuildPlan::new(&project, self.output.as_deref()) {
            Ok(plan) => plan,
            Err(error) => {
                eprintln!("{}", present_plan_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let options = ToolOptions::new(
            self.ibcmd.clone(),
            self.platform_arch.clone(),
            self.distrobox.clone(),
        );
        let ibcmd = match Ibcmd::discover(plan.platform_version(), &options) {
            Ok(ibcmd) => ibcmd,
            Err(error) => {
                eprintln!("{}", present_tool_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let human = matches!(self.format, OutputFormat::Human);
        let interactive = human && io::stderr().is_terminal();
        let styled = diagnostic_styling_enabled();
        if human
            && let Err(error) = write_build_started(ibcmd.version().as_str(), localizer, styled)
        {
            eprintln!(
                "{}",
                localizer.format(
                    "build-output-write-error",
                    &[("reason", LocalizationValue::Text(&error.to_string()))],
                )
            );
            return ExitCode::FAILURE;
        }
        let mut progress =
            interactive.then(|| ProgressLine::start(localizer.text("build-progress"), styled));
        let mut output_error = None;
        let result = build::execute_streaming(&plan, &ibcmd, |_, _, line| {
            if output_error.is_none()
                && let Err(error) =
                    write_diagnostic(line, &project, localizer, styled, progress.as_ref())
            {
                output_error = Some(error);
            }
        });
        if let Some(error) = progress
            .as_mut()
            .and_then(|progress| progress.finish().err())
            && output_error.is_none()
        {
            output_error = Some(error);
        }
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                eprintln!("{}", present_streamed_build_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        if let Some(error) = output_error {
            eprintln!(
                "{}",
                localizer.format(
                    "build-output-write-error",
                    &[("reason", LocalizationValue::Text(&error.to_string()))],
                )
            );
            return ExitCode::FAILURE;
        }

        write_build_result(self.format, &plan, &result, localizer)
    }
}

/// Render and flush one ibcmd line as soon as it reaches the CLI layer.
fn write_diagnostic(
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
                let owner = diff::render_metadata_path(&path, localizer);
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
fn diagnostic_styling_enabled() -> bool {
    io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
}

/// Enable success styling only when its stdout destination is interactive.
fn result_styling_enabled() -> bool {
    io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty())
}

/// Print the stable build heading before the first tool stage starts.
fn write_build_started(version: &str, localizer: &Localizer, styled: bool) -> io::Result<()> {
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

const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

struct ProgressState {
    output: Mutex<()>,
    frame: AtomicUsize,
    message: String,
    styled: bool,
}

struct ProgressLine {
    state: Arc<ProgressState>,
    stop: Sender<()>,
    worker: Option<thread::JoinHandle<io::Result<()>>>,
    active: bool,
}

impl ProgressLine {
    /// Start a terminal-owned spinner that remains below streamed diagnostics.
    fn start(message: String, styled: bool) -> Self {
        let state = Arc::new(ProgressState {
            output: Mutex::new(()),
            frame: AtomicUsize::new(0),
            message,
            styled,
        });
        let (stop, receiver) = mpsc::channel();
        let worker_state = Arc::clone(&state);
        let worker = thread::spawn(move || {
            loop {
                draw_progress(&worker_state)?;
                match receiver.recv_timeout(Duration::from_millis(80)) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        worker_state.frame.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        });
        Self {
            state,
            stop,
            worker: Some(worker),
            active: true,
        }
    }

    /// Clear the spinner, write one complete diagnostic line, then restore it.
    fn write_diagnostic(&self, line: &[u8]) -> io::Result<()> {
        let _guard = lock_progress_output(&self.state)?;
        let mut stderr = io::stderr().lock();
        clear_progress(&mut stderr)?;
        stderr.write_all(line)?;
        if !line.ends_with(b"\n") {
            stderr.write_all(b"\n")?;
        }
        draw_progress_locked(&mut stderr, &self.state)?;
        stderr.flush()
    }

    /// Stop animation and remove its final line before result presentation.
    fn finish(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        let _ = self.stop.send(());
        let worker_result = self
            .worker
            .take()
            .map(|worker| {
                worker
                    .join()
                    .map_err(|_| io::Error::other("build progress thread panicked"))?
            })
            .transpose();
        let clear_result = {
            let _guard = lock_progress_output(&self.state)?;
            let mut stderr = io::stderr().lock();
            clear_progress(&mut stderr)?;
            stderr.flush()
        };
        worker_result.and(clear_result)
    }
}

impl Drop for ProgressLine {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

/// Lock the multi-write terminal sequence shared with the animation thread.
fn lock_progress_output(state: &ProgressState) -> io::Result<std::sync::MutexGuard<'_, ()>> {
    state
        .output
        .lock()
        .map_err(|_| io::Error::other("build progress output lock was poisoned"))
}

/// Draw the current animation frame while acquiring the shared terminal lock.
fn draw_progress(state: &ProgressState) -> io::Result<()> {
    let _guard = lock_progress_output(state)?;
    let mut stderr = io::stderr().lock();
    draw_progress_locked(&mut stderr, state)?;
    stderr.flush()
}

/// Draw the current animation frame as the terminal's last line.
fn draw_progress_locked(stderr: &mut impl io::Write, state: &ProgressState) -> io::Result<()> {
    let frame = state.frame.load(Ordering::Relaxed) % SPINNER_FRAMES.len();
    let message = decorate_status(SPINNER_FRAMES[frame], &state.message, state.styled, "36");
    queue!(
        stderr,
        MoveToColumn(0),
        Clear(ClearType::CurrentLine),
        crossterm::style::Print(message)
    )
}

/// Remove the transient progress line without affecting completed diagnostics.
fn clear_progress(stderr: &mut impl io::Write) -> io::Result<()> {
    queue!(stderr, MoveToColumn(0), Clear(ClearType::CurrentLine))
}

/// Present the successful build without changing the JSON result contract.
fn write_build_result(
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

pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("build-about"))
        .override_usage(localizer.text("build-usage"))
        .mut_arg("output", |arg| {
            arg.help(localizer.text("build-output-help"))
                .value_name(localizer.text("build-output-value"))
        })
        .mut_arg("ibcmd", |arg| {
            arg.help(localizer.text("build-ibcmd-help"))
                .value_name(localizer.text("build-ibcmd-value"))
        })
        .mut_arg("platform_arch", |arg| {
            arg.help(localizer.text("build-arch-help"))
                .value_name(localizer.text("build-arch-value"))
        })
        .mut_arg("distrobox", |arg| {
            arg.help(localizer.text("build-distrobox-help"))
                .value_name(localizer.text("build-distrobox-value"))
        })
        .mut_arg("format", |arg| {
            arg.help(localizer.text("build-format-help"))
                .value_name(localizer.text("build-format-value"))
        })
        .mut_arg("help", |arg| arg.help(localizer.text("cli-help")))
}

/// Render output-plan errors without changing their stable domain representation.
fn present_plan_error(error: &PlanError, localizer: &Localizer) -> String {
    let (key, path) = match error {
        PlanError::ProjectNameMissing => return localizer.text("build-project-name-missing"),
        PlanError::InvalidOutput { path } => ("build-output-invalid", path),
        PlanError::UnexpectedExtension { path, expected } => {
            return localizer.format(
                "build-output-extension",
                &[
                    ("path", LocalizationValue::Text(&path.to_string_lossy())),
                    ("extension", LocalizationValue::Text(expected)),
                ],
            );
        }
    };
    localizer.format(
        key,
        &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
    )
}

/// Render discovery and exact-version failures with actionable machine-local settings.
fn present_tool_error(error: &ToolError, localizer: &Localizer) -> String {
    match error {
        ToolError::InvalidArchitecture(value) => localizer.format(
            "build-arch-invalid",
            &[("value", LocalizationValue::Text(value))],
        ),
        ToolError::InvalidContainer(value) => localizer.format(
            "build-distrobox-invalid",
            &[("value", LocalizationValue::Text(value))],
        ),
        ToolError::InvalidExecutable(path) => localizer.format(
            "build-ibcmd-invalid",
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
        ToolError::NotFound { expected, standard } => localizer.format(
            "build-ibcmd-missing",
            &[
                ("version", LocalizationValue::Text(expected.as_str())),
                ("path", LocalizationValue::Text(&standard.to_string_lossy())),
            ],
        ),
        ToolError::Run(error) => localizer.format(
            "build-ibcmd-run-error",
            &[("reason", LocalizationValue::Text(&error.to_string()))],
        ),
        ToolError::VersionCommandFailed { source, stderr } => localizer.format(
            "build-version-command-error",
            &[
                ("source", LocalizationValue::Text(&tool_source(source))),
                ("reason", LocalizationValue::Text(stderr)),
            ],
        ),
        ToolError::VersionUnreadable(source) => localizer.format(
            "build-version-unreadable",
            &[("source", LocalizationValue::Text(&tool_source(source)))],
        ),
        ToolError::VersionMismatch {
            expected,
            actual,
            source,
        } => localizer.format(
            "build-version-mismatch",
            &[
                ("expected", LocalizationValue::Text(expected.as_str())),
                ("actual", LocalizationValue::Text(actual.as_str())),
                ("source", LocalizationValue::Text(&tool_source(source))),
            ],
        ),
    }
}

/// Render build execution failures and identify the failing stage.
fn present_build_error(error: &BuildError, localizer: &Localizer) -> String {
    match error {
        BuildError::OutputParentMissing(path)
        | BuildError::InvalidExistingOutput(path)
        | BuildError::ArtifactMissing(path)
        | BuildError::ArtifactEmpty(path) => localizer.format(
            match error {
                BuildError::InvalidExistingOutput(_) => "build-output-existing-invalid",
                BuildError::ArtifactMissing(_) => "build-artifact-missing",
                BuildError::ArtifactEmpty(_) => "build-artifact-empty",
                _ => "build-output-invalid",
            },
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
        BuildError::ConfiguredOutputOutsideProject { root, output } => localizer.format(
            "build-output-outside-project",
            &[
                ("root", LocalizationValue::Text(&root.to_string_lossy())),
                ("path", LocalizationValue::Text(&output.to_string_lossy())),
            ],
        ),
        BuildError::CreateDirectory { path, source }
        | BuildError::CreateWorkspace { path, source }
        | BuildError::Publish { path, source }
        | BuildError::Restore { path, source }
        | BuildError::DescriptorDirectory { path, source }
        | BuildError::DescriptorRead { path, source } => localizer.format(
            "build-filesystem-error",
            &[
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
                ("reason", LocalizationValue::Text(&source.to_string())),
            ],
        ),
        BuildError::Run { stage, source } => localizer.format(
            match source {
                RunError::Interrupted => "build-interrupted",
                _ => "build-process-error",
            },
            &[
                (
                    "stage",
                    LocalizationValue::Text(&stage_name(*stage, localizer)),
                ),
                ("reason", LocalizationValue::Text(&format!("{source:?}"))),
            ],
        ),
        BuildError::CommandFailed { stage, stderr } => localizer.format(
            "build-command-failed",
            &[
                (
                    "stage",
                    LocalizationValue::Text(&stage_name(*stage, localizer)),
                ),
                ("reason", LocalizationValue::Text(stderr)),
            ],
        ),
        BuildError::DescriptorInvalid { path } => localizer.format(
            "build-descriptor-invalid",
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
        BuildError::DescriptorMissing(path) => localizer.format(
            "build-descriptor-missing",
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
        BuildError::DescriptorsMultiple(path) => localizer.format(
            "build-descriptors-multiple",
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
    }
}

/// Avoid repeating process output that the streaming renderer has already emitted.
fn present_streamed_build_error(error: &BuildError, localizer: &Localizer) -> String {
    if let BuildError::CommandFailed { stage, .. } = error {
        localizer.format(
            "build-command-failed-streamed",
            &[(
                "stage",
                LocalizationValue::Text(&stage_name(*stage, localizer)),
            )],
        )
    } else {
        present_build_error(error, localizer)
    }
}

/// Return the selected executable location for diagnostics only.
fn tool_source(source: &ToolSource) -> String {
    match source {
        ToolSource::Explicit(path) | ToolSource::Path(path) | ToolSource::Standard(path) => {
            path.to_string_lossy().into_owned()
        }
        ToolSource::Distrobox { container, path } => {
            format!("{container}:{}", path.to_string_lossy())
        }
    }
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

/// Localize the fixed pipeline stage while retaining enum-based control flow.
fn stage_name(stage: BuildStage, localizer: &Localizer) -> String {
    localizer.text(match stage {
        BuildStage::CreateInfobase => "build-stage-create-infobase",
        BuildStage::ImportSources => "build-stage-import-sources",
    })
}

#[derive(Serialize)]
struct BuildDocument {
    schema_version: u8,
    artifact: ArtifactDocument,
    platform: PlatformDocument,
    duration_ms: u128,
}

#[derive(Serialize)]
struct ArtifactDocument {
    r#type: &'static str,
    path: String,
    path_encoding: &'static str,
}

#[derive(Serialize)]
struct PlatformDocument {
    version: String,
}

impl BuildDocument {
    /// Build schema version 1 without locale-dependent values.
    fn new(plan: &BuildPlan, result: &build::BuildResult) -> Self {
        let (path, path_encoding) = json_path(result.output().as_os_str());
        Self {
            schema_version: 1,
            artifact: ArtifactDocument {
                r#type: plan.artifact_type().as_str(),
                path,
                path_encoding,
            },
            platform: PlatformDocument {
                version: plan.platform_version().as_str().to_owned(),
            },
            duration_ms: result.duration().as_millis(),
        }
    }
}

/// Preserve a UTF-8 path directly and use a reversible platform encoding otherwise.
fn json_path(path: &OsStr) -> (String, &'static str) {
    path.to_str()
        .map_or_else(|| encoded_path(path), |value| (value.to_owned(), "utf-8"))
}

#[cfg(unix)]
/// Percent-encode every raw Unix path byte when it is not valid UTF-8.
fn encoded_path(path: &OsStr) -> (String, &'static str) {
    use std::os::unix::ffi::OsStrExt;
    let mut encoded = String::with_capacity(path.as_bytes().len() * 3);
    for byte in path.as_bytes() {
        write!(encoded, "%{byte:02X}").expect("writing to String cannot fail");
    }
    (encoded, "percent")
}

#[cfg(windows)]
/// Percent-encode every UTF-16 code unit when a Windows path is not Unicode scalar text.
fn encoded_path(path: &OsStr) -> (String, &'static str) {
    use std::os::windows::ffi::OsStrExt;
    let mut encoded = String::new();
    for unit in path.encode_wide() {
        write!(encoded, "%{unit:04X}").expect("writing to String cannot fail");
    }
    (encoded, "utf-16-percent")
}
