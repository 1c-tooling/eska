//! Delegated interactive processes inherit terminal streams and are always reaped.

use crate::{Component, Error, resolve};
use std::{
    ffi::OsString,
    io,
    process::{Child, Command, ExitStatus},
    sync::{Arc, atomic::AtomicUsize},
};
#[cfg(windows)]
use std::{
    path::Path,
    process::Stdio,
    sync::atomic::Ordering,
    thread,
    time::{Duration, Instant},
};

/// Replace the Unix shell with its owner, or await the attached Windows process.
///
/// # Errors
/// Returns discovery/exec failures on Unix, or discovery/handler/process failures on Windows.
pub fn delegate(component: Component, args: &[OsString]) -> Result<ExitStatus, Error> {
    let binary = resolve(component)?;
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(Error::Process {
            component,
            source: Command::new(binary).args(args).exec(),
        })
    }
    #[cfg(windows)]
    {
        let signal = SignalState::new().map_err(Error::Signal)?;
        execute(&binary, args, &signal).map_err(|source| Error::Process { component, source })
    }
}

/// Keep a child attached to the same terminal; the parent never reads or decodes its streams.
#[cfg(windows)]
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
            cancellation = Some(Instant::now());
        }
        if cancellation.is_some_and(|started| started.elapsed() >= Duration::from_secs(10)) {
            child.0.kill()?;
            return child.0.wait();
        }
        thread::sleep(Duration::from_millis(10));
    }
}

pub struct OwnedChild(pub Child);

impl Drop for OwnedChild {
    /// An early wait/signal failure still reaps the exact child, without searching by process name.
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

pub struct SignalState {
    pub received: Arc<AtomicUsize>,
    #[cfg(unix)]
    handlers: Vec<signal_hook::SigId>,
}

impl SignalState {
    /// Record the exact Unix signal without doing I/O in a signal handler.
    #[cfg(unix)]
    pub fn new() -> io::Result<Self> {
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
    pub fn new() -> io::Result<Self> {
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
