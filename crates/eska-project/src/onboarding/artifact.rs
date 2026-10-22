//! Onboarding exchanges typed results with the native owner without linking a platform runtime.

pub use eska_presentation::arguments::project::artifact::PlatformArgs;

use eska_presentation::{
    components,
    localization::{LocalizationValue, Localizer},
    path_output,
};
use eska_project_core::{
    artifact::{self, Identity, Staging},
    project::build::PlatformVersion,
};
use eska_terminal::{
    interactive::PromptError,
    process_output::{decorate_status, diagnostic_styling_enabled, result_styling_enabled},
};
use eska_tool_runtime::{Component, Error, Operation, exchange, resolve as resolve_component};
use std::{
    ffi::OsString,
    io::{self, IsTerminal},
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct Failure {
    pub code: String,
    pub message: String,
}

impl Failure {
    /// Keep already-emitted native diagnostics separate from local presentation.
    pub fn new(code: impl Into<String>, message: String) -> Self {
        Self {
            code: code.into(),
            message,
        }
    }
    /// Report local failures once; a native owner's diagnostic has already reached inherited stderr.
    pub fn report(&self) {
        if self.message.is_empty() {
            return;
        }
        if self.code == "cancelled" {
            report("↩", &self.message, "33");
        } else {
            report("✗", &self.message, "31");
        }
    }
}

pub struct ToolSelection {
    binary: PathBuf,
    version: PlatformVersion,
    options: Vec<OsString>,
    language: &'static str,
    scope: Operation,
}

impl ToolSelection {
    /// Retain the verified exact version across the following native preparation.
    pub const fn version(&self) -> &PlatformVersion {
        &self.version
    }
}

pub struct PreparedSources {
    staging: Staging,
    identity: Identity,
    scope: Operation,
}

impl PreparedSources {
    /// Return the identity checked against the actual exported XML.
    #[must_use]
    pub const fn identity(&self) -> &Identity {
        &self.identity
    }
    /// Keep the transfer namespace owned until the project transaction moves its sources.
    #[must_use]
    pub fn sources(&self) -> PathBuf {
        self.staging.path().join("sources")
    }

    /// Retain the same sticky cancellation flag through source publication and enrollment.
    #[must_use]
    pub fn was_interrupted(&self) -> bool {
        self.scope.is_cancelled()
    }
}

struct PlatformHandler<'a> {
    args: &'a PlatformArgs,
}

impl PlatformHandler<'_> {
    /// Ask the build owner to select and verify a platform before claiming a project.
    pub fn resolve(
        &self,
        inherited: Option<&PlatformVersion>,
        localizer: &Localizer,
        scope: &Operation,
    ) -> Result<ToolSelection, Failure> {
        self.select(inherited, false, localizer, scope)
    }
    /// Keep initialization-specific selection and cancellation in the native owner.
    pub fn resolve_for_init(
        &self,
        localizer: &Localizer,
        scope: &Operation,
    ) -> Result<ToolSelection, Failure> {
        self.select(None, true, localizer, scope)
    }
    /// Bind one executable and one sticky cancellation scope across both native requests.
    fn select(
        &self,
        inherited: Option<&PlatformVersion>,
        init: bool,
        localizer: &Localizer,
        scope: &Operation,
    ) -> Result<ToolSelection, Failure> {
        let cancellation = if init {
            "init-cancelled"
        } else {
            "artifact-cancelled"
        };
        let failure = |error| component_failure(error, localizer, cancellation);
        let binary = resolve_component(Component::Build).map_err(failure)?;
        let language = localizer.locale().as_str();
        let options = self.options();
        let mut args: Vec<OsString> = ["--lang", language, "__bridge", "select-platform"]
            .into_iter()
            .map(Into::into)
            .collect();
        args.extend(options.iter().cloned());
        if let Some(version) = &self.args.platform_version {
            args.extend(["--platform-version".into(), version.as_str().into()]);
        }
        if self.args.select_platform {
            args.push("--select-platform".into());
        }
        if init {
            args.push("--init".into());
        }
        if let Some(version) = inherited {
            args.extend(["--inherited".into(), version.as_str().into()]);
        }
        let data = exchange(Component::Build, &binary, &args, "select_platform", scope)
            .map_err(failure)?;
        let version = data
            .as_object()
            .filter(|object| object.len() == 1)
            .and_then(|object| object.get("platform_version"))
            .and_then(serde_json::Value::as_str)
            .and_then(|version| PlatformVersion::parse(version).ok())
            .ok_or_else(|| failure(Error::InvalidResponse))?;
        Ok(ToolSelection {
            binary,
            version,
            options,
            language,
            scope: scope.clone(),
        })
    }
    /// Preserve OS-native option values; command lines never pass through a shell.
    fn options(&self) -> Vec<OsString> {
        let mut options = Vec::new();
        for (flag, value) in [
            ("--ibcmd", self.args.ibcmd.as_deref().map(Path::as_os_str)),
            (
                "--platform-arch",
                self.args.platform_arch.as_deref().map(std::ffi::OsStr::new),
            ),
            (
                "--distrobox",
                self.args.distrobox.as_deref().map(std::ffi::OsStr::new),
            ),
        ] {
            if let Some(value) = value {
                options.extend([flag.into(), value.to_owned()]);
            }
        }
        options
    }
}

/// Prepare into a caller-owned namespace; failed transport and invalid replies remove only it.
pub fn unpack(
    input: &Path,
    destination: &Path,
    tool: &ToolSelection,
    _: bool,
    localizer: &Localizer,
) -> Result<PreparedSources, Failure> {
    if tool.scope.is_cancelled() {
        return Err(component_failure(
            Error::Cancelled,
            localizer,
            "artifact-cancelled",
        ));
    }
    let staging =
        Staging::create(destination).map_err(|error| structural_failure(&error, localizer))?;
    let output = staging.path().join("sources");
    let mut args: Vec<OsString> = ["--lang", tool.language, "__bridge", "unpack"]
        .into_iter()
        .map(Into::into)
        .collect();
    args.extend([
        input.as_os_str().to_owned(),
        output.as_os_str().to_owned(),
        "--stage-near".into(),
        destination.as_os_str().to_owned(),
        "--platform-version".into(),
        tool.version.as_str().into(),
    ]);
    args.extend(tool.options.iter().cloned());
    let failure = |error| component_failure(error, localizer, "artifact-cancelled");
    let data =
        exchange(Component::Build, &tool.binary, &args, "unpack", &tool.scope).map_err(failure)?;
    let identity = artifact::inspect_identity(&output)
        .map_err(|error| structural_failure(&error, localizer))?
        .ok_or_else(|| failure(Error::InvalidResponse))?;
    let expected = serde_json::json!({"identity":{"type":identity.project_type.as_str(),"uuid":identity.uuid,"name":identity.name}});
    if data != expected {
        return Err(failure(Error::InvalidResponse));
    }
    Ok(PreparedSources {
        staging,
        identity,
        scope: tool.scope.clone(),
    })
}

/// Preserve known native error codes after their localized text has already been emitted.
fn component_failure(error: Error, localizer: &Localizer, cancellation: &str) -> Failure {
    match error {
        Error::Rejected { code, .. } if code != "cancelled" => Failure::new(code, String::new()),
        Error::Cancelled | Error::Rejected { .. } => {
            Failure::new("cancelled", localizer.text(cancellation))
        }
        error => Failure::new(error.code(), components::present(&error, localizer)),
    }
}

/// Structural transfer failures use the same onboarding vocabulary as native preparation.
fn structural_failure(error: &artifact::Error, localizer: &Localizer) -> Failure {
    let (code, key, path) = match error {
        artifact::Error::Io { path, source } => {
            return Failure::new(
                "io",
                localizer.format(
                    "artifact-io",
                    &[
                        (
                            "path",
                            LocalizationValue::Text(&path_output::render(
                                path,
                                io::stderr().is_terminal(),
                            )),
                        ),
                        ("reason", LocalizationValue::Text(&source.to_string())),
                    ],
                ),
            );
        }
        artifact::Error::UnsafePath(path) => ("unsafe-path", "artifact-unsafe-path", path),
        artifact::Error::InvalidDescriptor(path) => {
            ("invalid-descriptor", "artifact-invalid-descriptor", path)
        }
        artifact::Error::AmbiguousDescriptor(path) => (
            "ambiguous-descriptor",
            "artifact-ambiguous-descriptor",
            path,
        ),
    };
    Failure::new(
        code,
        localizer.format(
            key,
            &[(
                "path",
                LocalizationValue::Text(&path_output::render(path, io::stderr().is_terminal())),
            )],
        ),
    )
}

/// Shared marker policy retains the existing stdout success format.
pub fn success(message: &str) {
    println!(
        "{}",
        decorate_status("✓", message, result_styling_enabled(), "32")
    );
}
/// Native lines remain untouched; only command diagnostics receive a marker.
pub fn report(marker: &str, message: &str, color: &str) {
    eprintln!(
        "{}",
        decorate_status(marker, message, diagnostic_styling_enabled(), color)
    );
}
/// Interactive selection requires both input and diagnostics on a terminal.
pub fn terminal() -> bool {
    io::stdin().is_terminal() && io::stderr().is_terminal()
}
/// Preserve the previous prompt failure categories in the project owner.
pub fn prompt_error(error: PromptError, localizer: &Localizer) -> Failure {
    Failure::new(
        "cancelled",
        localizer.text(match error {
            PromptError::Cancelled => "artifact-cancelled",
            PromptError::Io => "new-prompt-error",
        }),
    )
}

/// Resolve a platform through the native owner while retaining caller cancellation.
///
/// # Errors
/// Returns discovery, selection, protocol or cancellation failures without writing a project.
pub fn resolve(
    args: &PlatformArgs,
    inherited: Option<&eska_project_core::project::build::PlatformVersion>,
    localizer: &Localizer,
    scope: &Operation,
) -> Result<ToolSelection, Failure> {
    PlatformHandler { args }.resolve(inherited, localizer, scope)
}

/// Resolve initialization's platform choice with the original cancellation behavior.
///
/// # Errors
/// Returns the same native-owner selection failures as resolve.
pub fn resolve_for_init(
    args: &PlatformArgs,
    localizer: &Localizer,
    scope: &Operation,
) -> Result<ToolSelection, Failure> {
    PlatformHandler { args }.resolve_for_init(localizer, scope)
}
