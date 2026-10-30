//! Shared platform selection and diagnostics for project onboarding and native imports.

pub use eska_presentation::arguments::build::artifact::{PlatformArgs, localize};

use std::{
    io::{self, IsTerminal},
    path::Path,
};

use crate::{
    cli::{
        diagnostics,
        interactive::{PromptError, Selector},
        localization::{LocalizationValue, Localizer},
        path_output, platform,
        process_output::{
            self, decorate_status, diagnostic_styling_enabled, progress::ProgressLine,
            result_styling_enabled,
        },
    },
    project::{
        artifact::{ArtifactError, PreparedArtifact},
        build::{Ibcmd, PlatformVersion},
    },
};

/// Keep Unix cancellation alive across short VCS queries and native version probes.
///
/// # Errors
/// Returns signal subscription failures before any command-owned file transaction starts.
#[cfg(unix)]
pub fn cancellation_scope(
    localizer: &Localizer,
) -> Result<Option<eska_tool_runtime::Operation>, Failure> {
    eska_tool_runtime::Operation::new()
        .map(Some)
        .map_err(|error| {
            Failure::new(
                "signal-handler",
                eska_presentation::components::present(&error, localizer),
            )
        })
}

/// Windows retains the native console cancellation controller.
///
/// # Errors
/// This platform requires no additional signal subscription.
#[cfg(not(unix))]
pub const fn cancellation_scope(
    _: &Localizer,
) -> Result<Option<eska_tool_runtime::Operation>, Failure> {
    Ok(None)
}

/// Carry caller cancellation into discovery rather than attaching it after a probe has finished.
#[must_use]
#[cfg(unix)]
pub fn scoped_options(
    options: crate::project::build::ToolOptions,
    scope: Option<&eska_tool_runtime::Operation>,
) -> crate::project::build::ToolOptions {
    match scope {
        Some(scope) => options.with_cancellation(scope.clone()),
        None => options,
    }
}

/// Windows options already use the native console cancellation controller.
#[must_use]
#[cfg(not(unix))]
pub const fn scoped_options(
    options: crate::project::build::ToolOptions,
    _: Option<&eska_tool_runtime::Operation>,
) -> crate::project::build::ToolOptions {
    options
}

/// Attach the already installed Unix scope; preserve the existing Windows console controller.
#[must_use]
#[cfg(unix)]
pub fn attach_cancellation(tool: Ibcmd, scope: Option<&eska_tool_runtime::Operation>) -> Ibcmd {
    match scope {
        Some(scope) => tool.with_cancellation(scope.clone()),
        None => tool,
    }
}

/// Preserve the existing Windows console controller.
#[must_use]
#[cfg(not(unix))]
pub const fn attach_cancellation(tool: Ibcmd, _: Option<&eska_tool_runtime::Operation>) -> Ibcmd {
    tool
}

/// Stream platform diagnostics with the same color and progress policy used by build.
///
/// # Errors
/// Returns native input, runner, cancellation or output failures before publication.
pub fn unpack(
    input: &Path,
    destination: &Path,
    tool: &Ibcmd,
    human: bool,
    localizer: &Localizer,
) -> Result<PreparedArtifact, ArtifactError> {
    let styled = diagnostic_styling_enabled();
    if human {
        let heading = localizer.format(
            "artifact-unpack-started",
            &[("version", LocalizationValue::Text(tool.version().as_str()))],
        );
        process_output::write_diagnostic(
            decorate_status("▶", &heading, styled, "36").as_bytes(),
            false,
            None,
        )
        .map_err(ArtifactError::Output)?;
    }
    let mut progress = (human && io::stderr().is_terminal())
        .then(|| ProgressLine::start(localizer.text("artifact-unpack-progress"), styled));
    let mut output_error = None;
    let result = PreparedArtifact::unpack(input, destination, tool, |_, line| {
        if output_error.is_none()
            && let Err(error) = process_output::write_diagnostic(line, styled, progress.as_ref())
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
    let prepared = result?;
    if let Some(error) = output_error {
        return Err(ArtifactError::Output(error));
    }
    Ok(prepared)
}

pub struct Failure {
    pub code: &'static str,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

impl Failure {
    /// Keep stable error codes independent from localized messages.
    #[must_use]
    pub const fn new(code: &'static str, message: String) -> Self {
        Self {
            code,
            message,
            details: None,
        }
    }

    /// Include a machine-readable preview when an import requires a decision.
    #[must_use]
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    /// Distinguish cancellation from failure without decorating nested error reasons twice.
    pub fn report(&self) {
        if self.code == "cancelled" {
            report("↩", &self.message, "33");
        } else {
            report("✗", &self.message, "31");
        }
    }
}

/// Give final success messages the same marker and stdout color policy as build.
pub fn success(message: &str) {
    println!(
        "{}",
        decorate_status("✓", message, result_styling_enabled(), "32")
    );
}

/// Decorate CLI diagnostics while native platform severity lines remain unchanged.
pub fn report(marker: &str, message: &str, color: &str) {
    eprintln!(
        "{}",
        decorate_status(marker, message, diagnostic_styling_enabled(), color)
    );
}

struct PlatformHandler<'a> {
    options: &'a PlatformArgs,
}

impl PlatformHandler<'_> {
    /// Resolve an explicit, inherited or interactively selected platform without hidden defaults.
    ///
    /// # Errors
    /// Returns runner selection, configuration, cancellation or version mismatch errors.
    pub fn resolve(
        &self,
        inherited: Option<&PlatformVersion>,
        interactive: bool,
        localizer: &Localizer,
    ) -> Result<Ibcmd, Failure> {
        let scope = cancellation_scope(localizer)?;
        self.resolve_with_cancellation(inherited, interactive, localizer, false, scope.as_ref())
            .and_then(|tool| {
                tool.ok_or_else(|| {
                    Failure::new(
                        "platform-required",
                        localizer.text("artifact-platform-required"),
                    )
                })
            })
    }

    /// Allow configuration-only onboarding to select a platform or leave its version empty.
    ///
    /// # Errors
    /// Returns the same machine selection and validation failures as `resolve`.
    pub fn resolve_for_init(&self, localizer: &Localizer) -> Result<Option<Ibcmd>, Failure> {
        let scope = cancellation_scope(localizer)?;
        self.resolve_with_cancellation(None, true, localizer, true, scope.as_ref())
    }

    /// Keep selection and installed-version verification identical across onboarding commands.
    fn resolve_with_cancellation(
        &self,
        inherited: Option<&PlatformVersion>,
        interactive: bool,
        localizer: &Localizer,
        initializing: bool,
        scope: Option<&eska_tool_runtime::Operation>,
    ) -> Result<Option<Ibcmd>, Failure> {
        let cancellation_key = if initializing {
            "init-cancelled"
        } else {
            "artifact-cancelled"
        };
        let options = platform::tool_options(
            self.options.ibcmd.clone(),
            self.options.platform_arch.clone(),
            self.options.distrobox.clone(),
        )
        .map_err(|error| {
            Failure::new(
                "global-config",
                diagnostics::present_global_config_error_with_links(
                    &error,
                    localizer,
                    io::stderr().is_terminal(),
                ),
            )
        })?;
        let options = scoped_options(options, scope);
        let version = if let Some(value) = &self.options.platform_version {
            value.clone()
        } else if !self.options.select_platform
            && let Some(version) = inherited
        {
            version.as_str().to_owned()
        } else {
            if !interactive || !terminal() {
                if initializing && !self.options.select_platform {
                    return Ok(None);
                }
                return Err(Failure::new(
                    "platform-required",
                    localizer.text("artifact-platform-required"),
                ));
            }
            let installed = Ibcmd::installed(&options).map_err(|error| {
                Failure::new(
                    "platform-discovery",
                    diagnostics::present_tool_error_with_links(
                        &error,
                        localizer,
                        io::stderr().is_terminal(),
                    ),
                )
            })?;
            if installed.is_empty() && !initializing {
                return Err(Failure::new(
                    "platform-not-found",
                    localizer.text("platform-none"),
                ));
            }
            let mut choices = installed
                .iter()
                .map(|value| {
                    let version = value.version().as_str().to_owned();
                    (version.clone(), version)
                })
                .collect::<Vec<_>>();
            if initializing {
                choices.push((String::new(), localizer.text("artifact-platform-later")));
            }
            let mut selector = Selector::start("build-platform-tui-title")
                .map_err(|error| platform_prompt_error(error, localizer, cancellation_key))?;
            let selected = selector
                .choose_values(localizer, "artifact-platform-menu", &choices)
                .map_err(|error| platform_prompt_error(error, localizer, cancellation_key))?;
            selector
                .finish()
                .map_err(|_| platform_prompt_error(PromptError::Io, localizer, cancellation_key))?;
            selected
        };
        if initializing && version.is_empty() {
            return Ok(None);
        }
        let version = PlatformVersion::parse(&version).map_err(|_| {
            Failure::new(
                "platform-version-invalid",
                localizer.text("artifact-version-invalid"),
            )
        })?;
        Ibcmd::discover(&version, &options)
            .map(Some)
            .map_err(|error| {
                Failure::new(
                    "platform-discovery",
                    diagnostics::present_tool_error_with_links(
                        &error,
                        localizer,
                        io::stderr().is_terminal(),
                    ),
                )
            })
    }
}

/// Localize selector cancellation for the command that owns the interaction.
fn platform_prompt_error(
    error: PromptError,
    localizer: &Localizer,
    cancellation_key: &str,
) -> Failure {
    match error {
        PromptError::Cancelled => Failure::new("cancelled", localizer.text(cancellation_key)),
        PromptError::Io => prompt_error(PromptError::Io, localizer),
    }
}

/// Require input and diagnostic output to remain visible during interactive decisions.
#[must_use]
pub fn terminal() -> bool {
    io::stdin().is_terminal() && io::stderr().is_terminal()
}

/// Reuse the existing prompt cancellation and terminal-error vocabulary.
pub fn prompt_error(error: PromptError, localizer: &Localizer) -> Failure {
    Failure::new(
        "cancelled",
        localizer.text(match error {
            PromptError::Cancelled => "artifact-cancelled",
            PromptError::Io => "new-prompt-error",
        }),
    )
}

/// Translate structured import failures only at the CLI boundary.
pub fn present(error: &ArtifactError, localizer: &Localizer) -> Failure {
    let hyperlinks = io::stderr().is_terminal();
    let (code, key, path) = match error {
        ArtifactError::Io { path, source } => {
            return Failure::new(
                "io",
                localizer.format(
                    "artifact-io",
                    &[
                        (
                            "path",
                            LocalizationValue::Text(&path_output::render(path, hyperlinks)),
                        ),
                        ("reason", LocalizationValue::Text(&source.to_string())),
                    ],
                ),
            );
        }
        ArtifactError::UnsupportedFile(path) => {
            ("unsupported-file", "artifact-unsupported-file", path)
        }
        ArtifactError::InvalidDescriptor(path) => {
            ("invalid-descriptor", "artifact-invalid-descriptor", path)
        }
        ArtifactError::MissingDescriptor(path) => {
            ("missing-descriptor", "artifact-missing-descriptor", path)
        }
        ArtifactError::AmbiguousDescriptor(path) => (
            "ambiguous-descriptor",
            "artifact-ambiguous-descriptor",
            path,
        ),
        ArtifactError::UnsafePath(path) => ("unsafe-path", "artifact-unsafe-path", path),
        ArtifactError::SourceChanged(path) => ("source-changed", "artifact-source-changed", path),
        ArtifactError::Config(_) => {
            return Failure::new("project-config", localizer.text("artifact-config-error"));
        }
        ArtifactError::Publication { path, source } => {
            if matches!(
                source.as_ref(),
                eska_project_core::project::create::CreationError::Rollback { .. }
            ) {
                (
                    "publication-rollback",
                    "artifact-new-publication-cleanup",
                    path,
                )
            } else {
                ("publication", "artifact-new-publication-error", path)
            }
        }
        ArtifactError::Cancelled => {
            return Failure::new("cancelled", localizer.text("artifact-cancelled"));
        }
        ArtifactError::ChangeInspection => {
            return Failure::new("repository", localizer.text("artifact-repository-error"));
        }
        ArtifactError::ConfirmationRequired => {
            return Failure::new(
                "confirmation-required",
                localizer.text("artifact-confirmation-required"),
            );
        }
        ArtifactError::Rollback { backup, .. } => {
            return Failure::new(
                "rollback",
                localizer.format(
                    "artifact-rollback-error",
                    &[(
                        "path",
                        LocalizationValue::Text(&path_output::render(backup, hyperlinks)),
                    )],
                ),
            );
        }
        ArtifactError::TypeMismatch { .. } => {
            return Failure::new("type-mismatch", localizer.text("artifact-type-mismatch"));
        }
        ArtifactError::Run(_) => {
            return Failure::new("platform-run", localizer.text("artifact-platform-run"));
        }
        ArtifactError::Output(_) => {
            return Failure::new("output", localizer.text("artifact-output-error"));
        }
        ArtifactError::Platform { .. } => {
            return Failure::new(
                "platform-failed",
                localizer.text("artifact-platform-failed"),
            );
        }
    };
    Failure::new(
        code,
        localizer.format(
            key,
            &[(
                "path",
                LocalizationValue::Text(&path_output::render(path, hyperlinks)),
            )],
        ),
    )
}

/// Resolve parsed platform choices while retaining the selector's cancellation scope.
///
/// # Errors
/// Returns machine, selection, probe or cancellation failures before native work.
pub fn resolve(
    args: &PlatformArgs,
    inherited: Option<&PlatformVersion>,
    interactive: bool,
    localizer: &Localizer,
) -> Result<Ibcmd, Failure> {
    PlatformHandler { options: args }.resolve(inherited, interactive, localizer)
}

/// Resolve initialization choices with their existing cancellation vocabulary.
///
/// # Errors
/// Returns the same discovery and selection failures as resolve.
pub fn resolve_for_init(
    args: &PlatformArgs,
    localizer: &Localizer,
) -> Result<Option<Ibcmd>, Failure> {
    PlatformHandler { options: args }.resolve_for_init(localizer)
}
