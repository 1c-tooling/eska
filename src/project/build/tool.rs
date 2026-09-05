use std::{
    env,
    ffi::{OsStr, OsString},
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use super::PlatformVersion;

const IBCMD_ENV: &str = "ESKA_IBCMD";
const ARCH_ENV: &str = "ESKA_PLATFORM_ARCH";
const DISTROBOX_ENV: &str = "ESKA_DISTROBOX";
static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static SIGNAL_HANDLER: OnceLock<Result<(), String>> = OnceLock::new();

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ToolOptions {
    ibcmd: Option<PathBuf>,
    platform_arch: Option<String>,
    distrobox: Option<String>,
}

impl ToolOptions {
    /// Create machine-local tool overrides supplied by the CLI.
    #[must_use]
    pub const fn new(
        ibcmd: Option<PathBuf>,
        platform_arch: Option<String>,
        distrobox: Option<String>,
    ) -> Self {
        Self {
            ibcmd,
            platform_arch,
            distrobox,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolSource {
    Explicit(PathBuf),
    Path(PathBuf),
    Standard(PathBuf),
    Distrobox { container: String, path: PathBuf },
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Runner {
    Host(PathBuf),
    Distrobox { container: String, path: PathBuf },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ibcmd {
    runner: Runner,
    source: ToolSource,
    version: PlatformVersion,
}

impl Ibcmd {
    /// Discover `ibcmd` and require the exact project platform version.
    ///
    /// Explicit CLI values take precedence over environment values. Distrobox is
    /// considered only when its container is explicitly configured.
    ///
    /// # Errors
    /// Returns a structured error when an override is unsafe, the executable is
    /// unavailable, its version cannot be read, or the exact version differs.
    pub fn discover(expected: &PlatformVersion, options: &ToolOptions) -> Result<Self, ToolError> {
        let arch = options
            .platform_arch
            .clone()
            .or_else(|| env::var(ARCH_ENV).ok())
            .unwrap_or_else(default_architecture);
        validate_component(&arch).map_err(|()| ToolError::InvalidArchitecture(arch.clone()))?;

        let explicit = options
            .ibcmd
            .clone()
            .or_else(|| env::var_os(IBCMD_ENV).map(PathBuf::from));
        let distrobox = options
            .distrobox
            .clone()
            .or_else(|| env::var(DISTROBOX_ENV).ok());

        let (runner, source) = if let Some(path) = explicit {
            validate_executable(&path)?;
            (Runner::Host(path.clone()), ToolSource::Explicit(path))
        } else if let Some(path) = find_on_path("ibcmd") {
            (Runner::Host(path.clone()), ToolSource::Path(path))
        } else {
            let standard = standard_path(&arch, expected);
            if standard.is_file() {
                (
                    Runner::Host(standard.clone()),
                    ToolSource::Standard(standard),
                )
            } else if let Some(container) = distrobox {
                validate_component(&container)
                    .map_err(|()| ToolError::InvalidContainer(container.clone()))?;
                let path = PathBuf::from(format!("/opt/1cv8/{arch}/{}/ibcmd", expected.as_str()));
                (
                    Runner::Distrobox {
                        container: container.clone(),
                        path: path.clone(),
                    },
                    ToolSource::Distrobox { container, path },
                )
            } else {
                return Err(ToolError::NotFound {
                    expected: expected.clone(),
                    standard,
                });
            }
        };

        let output = run_runner(&runner, [OsStr::new("--version")])?;
        if !output.status.success() {
            return Err(ToolError::VersionCommandFailed {
                source,
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            });
        }
        let version = parse_version(&output.stdout, &output.stderr)
            .ok_or_else(|| ToolError::VersionUnreadable(source.clone()))?;
        if version != *expected {
            return Err(ToolError::VersionMismatch {
                expected: expected.clone(),
                actual: version,
                source,
            });
        }
        Ok(Self {
            runner,
            source,
            version,
        })
    }

    #[must_use]
    pub const fn source(&self) -> &ToolSource {
        &self.source
    }

    #[must_use]
    pub const fn version(&self) -> &PlatformVersion {
        &self.version
    }

    /// Install cancellation handling and clear stale state before a complete build pipeline.
    ///
    /// # Errors
    /// Returns an error if another library already owns the process signal handler.
    pub fn begin_interruptible_operation(&self) -> Result<(), RunError> {
        install_signal_handler()?;
        INTERRUPTED.store(false, Ordering::SeqCst);
        Ok(())
    }

    #[must_use]
    pub fn was_interrupted(&self) -> bool {
        INTERRUPTED.load(Ordering::SeqCst)
    }

    /// Run one verified `ibcmd` command without invoking a shell for host paths.
    ///
    /// # Errors
    /// Returns an I/O error if the selected process cannot be started or waited for.
    pub fn run<I, S>(&self, arguments: I) -> Result<Output, io::Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        run_runner(&self.runner, arguments)
    }

    /// Run a build command and terminate its exact child process after Ctrl+C or termination.
    ///
    /// # Errors
    /// Returns a structured error when the signal handler or process lifecycle fails.
    pub fn run_interruptible<I, S>(&self, arguments: I, pid_file: &Path) -> Result<Output, RunError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let arguments: Vec<_> = arguments
            .into_iter()
            .map(|argument| argument.as_ref().to_os_string())
            .collect();
        let mut command = interruptible_runner_command(&self.runner, &arguments, pid_file);
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(RunError::Io)?;
        let stdout = child.stdout.take().ok_or(RunError::MissingPipe)?;
        let stderr = child.stderr.take().ok_or(RunError::MissingPipe)?;
        let stdout = thread::spawn(move || read_pipe(stdout));
        let stderr = thread::spawn(move || read_pipe(stderr));
        let (status, interrupted) = loop {
            if INTERRUPTED.load(Ordering::SeqCst) {
                terminate_distrobox_process(&self.runner, pid_file);
                child.kill().map_err(RunError::Io)?;
                break (child.wait().map_err(RunError::Io)?, true);
            }
            if let Some(status) = child.try_wait().map_err(RunError::Io)? {
                break (status, false);
            }
            thread::sleep(Duration::from_millis(25));
        };
        let output = Output {
            status,
            stdout: join_pipe(stdout)?,
            stderr: join_pipe(stderr)?,
        };
        if interrupted {
            Err(RunError::Interrupted)
        } else {
            Ok(output)
        }
    }
}

#[derive(Debug)]
pub enum RunError {
    SignalHandler(String),
    Io(io::Error),
    MissingPipe,
    ReaderPanicked,
    Interrupted,
}

#[derive(Debug)]
pub enum ToolError {
    InvalidArchitecture(String),
    InvalidContainer(String),
    InvalidExecutable(PathBuf),
    NotFound {
        expected: PlatformVersion,
        standard: PathBuf,
    },
    Run(io::Error),
    VersionCommandFailed {
        source: ToolSource,
        stderr: String,
    },
    VersionUnreadable(ToolSource),
    VersionMismatch {
        expected: PlatformVersion,
        actual: PlatformVersion,
        source: ToolSource,
    },
}

/// Run either the host executable or Distrobox with values passed as distinct arguments.
fn run_runner<I, S>(runner: &Runner, arguments: I) -> Result<Output, io::Error>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let arguments: Vec<OsString> = arguments
        .into_iter()
        .map(|argument| argument.as_ref().to_os_string())
        .collect();
    runner_command(runner, &arguments).output()
}

/// Construct a command without concatenating user-controlled values into shell text.
fn runner_command(runner: &Runner, arguments: &[OsString]) -> Command {
    match runner {
        Runner::Host(path) => {
            let mut command = Command::new(path);
            command.args(arguments);
            command
        }
        Runner::Distrobox { container, path } => {
            let mut command = Command::new("distrobox");
            command
                .args(["enter", "--name", container, "--"])
                .arg(path)
                .args(arguments);
            command
        }
    }
}

/// Wrap Distrobox commands so the exact container-side ibcmd PID is observable.
fn interruptible_runner_command(
    runner: &Runner,
    arguments: &[OsString],
    pid_file: &Path,
) -> Command {
    match runner {
        Runner::Host(_) => runner_command(runner, arguments),
        Runner::Distrobox { container, path } => {
            let mut command = Command::new("distrobox");
            command
                .args(["enter", "--name", container, "--", "sh", "-c"])
                .arg("printf '%s\\n' \"$$\" > \"$1\"; shift; exec \"$@\"")
                .arg("sh")
                .arg(pid_file)
                .arg(path)
                .args(arguments);
            command
        }
    }
}

/// Signal only the recorded container-side ibcmd process before stopping the adapter.
fn terminate_distrobox_process(runner: &Runner, pid_file: &Path) {
    let Runner::Distrobox { container, .. } = runner else {
        return;
    };
    let Ok(pid) = std::fs::read_to_string(pid_file) else {
        return;
    };
    let pid = pid.trim();
    if pid.is_empty() || !pid.bytes().all(|byte| byte.is_ascii_digit()) {
        return;
    }
    let _ = Command::new("distrobox")
        .args([
            "enter",
            "--name",
            container,
            "--",
            "sh",
            "-c",
            "kill -TERM \"$1\"",
            "sh",
            pid,
        ])
        .output();
}

/// Install one process-wide handler that records cancellation for the active build command.
fn install_signal_handler() -> Result<(), RunError> {
    SIGNAL_HANDLER
        .get_or_init(|| {
            ctrlc::set_handler(|| INTERRUPTED.store(true, Ordering::SeqCst))
                .map_err(|error| error.to_string())
        })
        .clone()
        .map_err(RunError::SignalHandler)
}

/// Drain a child pipe concurrently so verbose ibcmd output cannot deadlock the process.
fn read_pipe(mut pipe: impl Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    pipe.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Convert a pipe reader thread into a structured process error.
fn join_pipe(handle: thread::JoinHandle<io::Result<Vec<u8>>>) -> Result<Vec<u8>, RunError> {
    handle
        .join()
        .map_err(|_| RunError::ReaderPanicked)?
        .map_err(RunError::Io)
}

/// Reject values that could be interpreted as paths or command options.
fn validate_component(value: &str) -> Result<(), ()> {
    if value.is_empty()
        || value.starts_with('-')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(());
    }
    Ok(())
}

/// Require an explicit path to name an existing regular file.
fn validate_executable(path: &Path) -> Result<(), ToolError> {
    if path.is_file() {
        Ok(())
    } else {
        Err(ToolError::InvalidExecutable(path.to_owned()))
    }
}

/// Return the first regular file matching an executable name on PATH.
fn find_on_path(name: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|directory| directory.join(executable_name(name)))
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(windows)]
/// Add the native executable suffix on Windows.
fn executable_name(name: &str) -> String {
    format!("{name}.exe")
}

#[cfg(not(windows))]
/// Preserve executable names on Unix-like hosts.
const fn executable_name(name: &str) -> &str {
    name
}

#[cfg(windows)]
/// Resolve the standard Windows 1C installation path.
fn standard_path(arch: &str, version: &PlatformVersion) -> PathBuf {
    let root = if arch == "i386" {
        env::var_os("ProgramFiles(x86)")
    } else {
        env::var_os("ProgramFiles")
    }
    .map_or_else(|| PathBuf::from(r"C:\Program Files"), PathBuf::from);
    root.join("1cv8")
        .join(version.as_str())
        .join("bin")
        .join("ibcmd.exe")
}

#[cfg(not(windows))]
/// Resolve the standard Linux 1C installation path.
fn standard_path(arch: &str, version: &PlatformVersion) -> PathBuf {
    PathBuf::from("/opt/1cv8")
        .join(arch)
        .join(version.as_str())
        .join("ibcmd")
}

/// Map Rust target architectures to 1C distribution directory names.
fn default_architecture() -> String {
    match env::consts::ARCH {
        "x86" => "i386".to_owned(),
        arch => arch.to_owned(),
    }
}

/// Extract exactly one four-component numeric version token from command output.
fn parse_version(stdout: &[u8], stderr: &[u8]) -> Option<PlatformVersion> {
    String::from_utf8_lossy(stdout)
        .split_whitespace()
        .chain(String::from_utf8_lossy(stderr).split_whitespace())
        .find_map(|token| {
            let token = token
                .trim_matches(|character: char| !character.is_ascii_digit() && character != '.');
            PlatformVersion::parse(token).ok()
        })
}

impl From<io::Error> for ToolError {
    /// Preserve process startup and wait failures as structured tool errors.
    fn from(value: io::Error) -> Self {
        Self::Run(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Read a platform version without accepting partial or malformed tokens.
    fn parses_exact_version_token() {
        assert_eq!(
            parse_version(b"1C ibcmd version 8.3.27.2325\n", b"")
                .expect("version")
                .as_str(),
            "8.3.27.2325"
        );
        assert!(parse_version(b"version 8.3.27", b"").is_none());
    }

    #[test]
    /// Reject option-like and path-like architecture and container names.
    fn machine_components_are_single_safe_values() {
        for value in ["", "--help", "../box", "box/name", "box name"] {
            assert!(validate_component(value).is_err());
        }
        assert!(validate_component("1c-ubuntu_env.1").is_ok());
    }
}
