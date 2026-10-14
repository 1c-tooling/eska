//! One verified companion executable owns the delegated build and its streams.

mod probe;

use std::{
    env,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::{self, Command, ExitCode, ExitStatus, Stdio},
};

use crate::project::ProjectType;

/// Only the CLI presents these process failures; source/build errors belong to the child.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Error {
    Missing,
    Unavailable,
    ProbeFailed,
    ProbeTimeout,
    ProbeOversized,
    Incompatible,
    ExecutionFailed,
}

impl Error {
    /// Preserve stable machine identifiers without exposing localized operating-system errors.
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::Missing => "build-tool-missing",
            Self::Unavailable => "build-tool-unavailable",
            Self::ProbeFailed => "build-tool-probe-failed",
            Self::ProbeTimeout => "build-tool-probe-timeout",
            Self::ProbeOversized => "build-tool-probe-oversized",
            Self::Incompatible => "build-tool-incompatible",
            Self::ExecutionFailed => "build-tool-execution-failed",
        }
    }

    /// Localization stays at the shell boundary, separate from the child's result bytes.
    pub(super) const fn message_key(self) -> &'static str {
        match self {
            Self::Missing => "build-delegation-tool-missing",
            Self::Unavailable => "build-delegation-tool-unavailable",
            Self::ProbeFailed => "build-delegation-probe-failed",
            Self::ProbeTimeout => "build-delegation-probe-timeout",
            Self::ProbeOversized => "build-delegation-probe-oversized",
            Self::Incompatible => "build-delegation-tool-incompatible",
            Self::ExecutionFailed => "build-delegation-execution-failed",
        }
    }
}

/// The same resolved executable is probed and executed; incompatible siblings never fall through.
pub(super) struct Tool {
    path: PathBuf,
}

impl Tool {
    /// Resolve the installed sibling before searching PATH, without launching a shell.
    pub(super) fn discover() -> Result<Self, Error> {
        let executable = env::current_exe().map_err(|_| Error::Unavailable)?;
        let path = locate(&executable, env::var_os("PATH").as_deref()).ok_or(Error::Missing)?;
        Ok(Self { path })
    }

    /// Confirm package/protocol compatibility before a platform or build operation can start.
    pub(super) fn verify(&self, locale: &str, project_type: ProjectType) -> Result<(), Error> {
        let bytes = probe::capture(&self.path, locale)?;
        probe::validate(&bytes, project_type)
    }

    /// Inherit the caller's terminal and return the child's exact numeric process status.
    ///
    /// No parent publication transaction or signal handler is active here. `process::exit`
    /// preserves Windows i32 exit codes that cannot be represented by `ExitCode::from(u8)`;
    /// all temporary resources and native build cleanup belong to the completed child.
    pub(super) fn execute(&self, arguments: &[OsString]) -> Result<ExitCode, Error> {
        let status = Command::new(&self.path)
            .args(arguments)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|_| Error::ExecutionFailed)?;
        process::exit(status_code(status))
    }
}

/// Existing files take precedence, including a sibling whose execution will fail explicitly.
fn locate(executable: &Path, search_path: Option<&OsStr>) -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "eska-build.exe"
    } else {
        "eska-build"
    };
    let sibling = executable.parent()?.join(name);
    if sibling.is_file() {
        return Some(sibling);
    }
    search_path.and_then(|paths| {
        env::split_paths(paths)
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
    })
}

/// Numeric statuses remain exact; signal termination follows the conventional Unix shell value.
fn status_code(status: ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    1
}
