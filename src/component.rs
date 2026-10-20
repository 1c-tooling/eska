//! Process delegation preserves streams and only signals the child owned by this invocation.

use crate::cli::localization::{LocalizationValue, Localizer};
use std::{
    env,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    process::{Child, Command, ExitCode, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub enum Component {
    Project,
    Build,
    Vcs,
}

impl Component {
    /// Keep executable discovery independent of localized help or command aliases.
    const fn name(self) -> &'static str {
        match self {
            Self::Project => "eska-project",
            Self::Build => "eska-build",
            Self::Vcs => "eska-vcs",
        }
    }

    /// Explicit paths always take precedence and never silently fall back after failure.
    const fn variable(self) -> &'static str {
        match self {
            Self::Project => "ESKA_PROJECT_BINARY",
            Self::Build => "ESKA_BUILD_BINARY",
            Self::Vcs => "ESKA_VCS_BINARY",
        }
    }
}

#[derive(Clone, Copy)]
pub enum Invocation {
    Direct,
    LegacyIde,
    LegacyValidate,
}

enum LocateError {
    Missing,
    Invalid { path: PathBuf, source: io::Error },
}

/// Locate the owner before starting any operation; missing components never trigger installation.
pub fn run(
    component: Component,
    args: &[OsString],
    invocation: Invocation,
    localizer: &Localizer,
) -> ExitCode {
    let binary = match locate(component) {
        Ok(binary) => binary,
        Err(error) => {
            eprintln!("{}", present_location(component, error, localizer));
            return ExitCode::FAILURE;
        }
    };
    let signal = match SignalState::new() {
        Ok(signal) => signal,
        Err(error) => {
            eprintln!(
                "{}",
                localizer.format(
                    "component-signal-error",
                    &[("reason", LocalizationValue::Text(&error.to_string()))]
                )
            );
            return ExitCode::FAILURE;
        }
    };
    let args = forward_args(args, invocation);
    match execute(&binary, &args, &signal) {
        Ok(status) => status_code(status),
        Err(error) => {
            eprintln!(
                "{}",
                localizer.format(
                    "component-process-error",
                    &[
                        ("tool", LocalizationValue::Text(component.name())),
                        ("reason", LocalizationValue::Text(&error.to_string()))
                    ]
                )
            );
            ExitCode::FAILURE
        }
    }
}

/// A selected component is an executable file, never the shell itself or a directory.
fn checked(path: &Path) -> Result<PathBuf, LocateError> {
    let invalid = |source| LocateError::Invalid {
        path: path.to_owned(),
        source,
    };
    let canonical = fs::canonicalize(path).map_err(invalid)?;
    if !canonical.is_file() || env::current_exe().is_ok_and(|shell| shell == canonical) {
        return Err(invalid(io::Error::new(
            io::ErrorKind::InvalidInput,
            "expected a standalone component executable",
        )));
    }
    Ok(canonical)
}

/// Prefer an explicit path, then sibling release tools, PATH and the managed user bin directory.
fn locate(component: Component) -> Result<PathBuf, LocateError> {
    if let Some(path) = env::var_os(component.variable()) {
        return checked(Path::new(&path));
    }
    let name = format!("{}{}", component.name(), env::consts::EXE_SUFFIX);
    let mut candidates = Vec::new();
    if let Ok(shell) = env::current_exe()
        && let Some(directory) = shell.parent()
    {
        candidates.push(directory.join(&name));
    }
    if let Some(path) = env::var_os("PATH") {
        candidates.extend(env::split_paths(&path).map(|directory| directory.join(&name)));
    }
    if let Some(home) = env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        candidates.push(PathBuf::from(home).join(".eska/bin").join(name));
    }
    for candidate in candidates {
        match fs::symlink_metadata(&candidate) {
            Ok(_) => return checked(&candidate),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(LocateError::Invalid {
                    path: candidate,
                    source,
                });
            }
        }
    }
    Err(LocateError::Missing)
}

/// Find the actual command after global option values, preserving native argument bytes.
fn command_position(args: &[OsString]) -> Option<usize> {
    let mut position = 0;
    while let Some(argument) = args.get(position) {
        if argument == "--lang" || argument == "--project-dir" {
            position += 2;
        } else if argument == "--" {
            return args.get(position + 1).map(|_| position + 1);
        } else if argument.to_string_lossy().starts_with('-') {
            position += 1;
        } else {
            return Some(position);
        }
    }
    None
}

/// Adapt only legacy entry names; every other argument and standard stream passes through intact.
fn forward_args(args: &[OsString], invocation: Invocation) -> Vec<OsString> {
    let mut args = args.to_vec();
    match invocation {
        Invocation::Direct => {}
        Invocation::LegacyIde => {
            if let Some(position) = command_position(&args) {
                args[position] = OsString::from("serve");
                args.insert(position + 1, OsString::from("--legacy-identity"));
            }
        }
        Invocation::LegacyValidate => {
            let position = args
                .iter()
                .position(|arg| arg == "--")
                .unwrap_or(args.len());
            args.insert(position, OsString::from("__shell-validate"));
        }
    }
    args
}

/// Keep a child attached to the same terminal; the parent never reads or decodes its streams.
fn execute(binary: &Path, args: &[OsString], signal: &SignalState) -> io::Result<ExitStatus> {
    let child = Command::new(binary)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut child = OwnedChild(child);
    let mut cancellation = None;
    loop {
        if let Some(status) = child.0.try_wait()? {
            return Ok(status);
        }
        let received = signal.received.load(Ordering::Acquire);
        if received != 0 && cancellation.is_none() {
            #[cfg(unix)]
            forward_signal(&child.0, received)?;
            cancellation = Some(Instant::now());
        }
        if cancellation.is_some_and(|started| started.elapsed() >= Duration::from_secs(10)) {
            child.0.kill()?;
            return child.0.wait();
        }
        thread::sleep(Duration::from_millis(10));
    }
}

struct OwnedChild(Child);

impl Drop for OwnedChild {
    /// An early wait/signal failure still reaps the exact child, without searching by process name.
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

struct SignalState {
    received: Arc<AtomicUsize>,
    #[cfg(unix)]
    handlers: Vec<signal_hook::SigId>,
}

impl SignalState {
    /// Record the exact Unix signal without doing I/O in a signal handler.
    #[cfg(unix)]
    fn new() -> io::Result<Self> {
        use signal_hook::consts::signal::{SIGHUP, SIGINT, SIGTERM};
        let mut state = Self {
            received: Arc::new(AtomicUsize::new(0)),
            handlers: Vec::new(),
        };
        for signal in [SIGINT, SIGTERM, SIGHUP] {
            state.handlers.push(signal_hook::flag::register_usize(
                signal,
                Arc::clone(&state.received),
                usize::try_from(signal).map_err(io::Error::other)?,
            )?);
        }
        Ok(state)
    }

    /// Windows console events are already delivered to both attached console processes.
    #[cfg(windows)]
    fn new() -> io::Result<Self> {
        let received = Arc::new(AtomicUsize::new(0));
        let flag = Arc::clone(&received);
        ctrlc::set_handler(move || flag.store(2, Ordering::Release)).map_err(io::Error::other)?;
        Ok(Self { received })
    }
}

#[cfg(unix)]
impl Drop for SignalState {
    /// Release this operation's subscriptions before another command owns process cancellation.
    fn drop(&mut self) {
        for handler in self.handlers.drain(..) {
            signal_hook::low_level::unregister(handler);
        }
    }
}

/// Forward only to the owned live child; exec preserves its foreground terminal group.
#[cfg(unix)]
fn forward_signal(child: &Child, signal: usize) -> io::Result<()> {
    use nix::{
        sys::signal::{Signal, kill},
        unistd::Pid,
    };
    let pid = i32::try_from(child.id()).map_err(io::Error::other)?;
    let signal = Signal::try_from(i32::try_from(signal).map_err(io::Error::other)?)
        .map_err(io::Error::other)?;
    match kill(Pid::from_raw(pid), signal) {
        Ok(()) | Err(nix::errno::Errno::ESRCH) => Ok(()),
        Err(error) => Err(io::Error::other(error)),
    }
}

/// Preserve child exit codes and represent Unix signal termination as the shell's 128+signal code.
fn status_code(status: ExitStatus) -> ExitCode {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ExitCode::from(
            u8::try_from(
                status
                    .code()
                    .unwrap_or_else(|| 128 + status.signal().unwrap_or(1)),
            )
            .unwrap_or(1),
        )
    }
    #[cfg(windows)]
    {
        let code = status.code().unwrap_or(1);
        match u8::try_from(code) {
            Ok(code) => ExitCode::from(code),
            // Stable ExitCode exposes only u8; exit preserves a full Windows child status.
            Err(_) => std::process::exit(code),
        }
    }
}

/// Missing versus explicitly broken component paths remain distinguishable for installation repair.
fn present_location(component: Component, error: LocateError, localizer: &Localizer) -> String {
    match error {
        LocateError::Missing => localizer.format(
            "component-missing",
            &[
                ("tool", LocalizationValue::Text(component.name())),
                ("variable", LocalizationValue::Text(component.variable())),
            ],
        ),
        LocateError::Invalid { path, source } => localizer.format(
            "component-invalid",
            &[
                ("tool", LocalizationValue::Text(component.name())),
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
                ("reason", LocalizationValue::Text(&source.to_string())),
            ],
        ),
    }
}
