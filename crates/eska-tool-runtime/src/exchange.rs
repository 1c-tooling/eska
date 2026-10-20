//! Interactive composition keeps stderr/stdin attached while receiving one bounded machine reply.

use crate::{
    Component, Error,
    process::{OwnedChild, SignalState},
    query::{bounded, decode},
};
use serde_json::Value;
use std::{
    ffi::OsString,
    io,
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, atomic::Ordering, mpsc},
    thread,
    time::{Duration, Instant},
};

/// One sticky cancellation scope spans selection, preparation, confirmation and publication.
#[derive(Clone)]
pub struct Operation(Arc<SignalState>);

impl std::fmt::Debug for Operation {
    /// Report cancellation state without exposing OS subscription internals.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Operation")
            .field("cancelled", &self.is_cancelled())
            .finish_non_exhaustive()
    }
}

impl Operation {
    /// Install only this operation's signal subscriptions before invoking component work.
    ///
    /// # Errors
    /// Returns OS handler installation failures before creating a child or changing files.
    pub fn new() -> Result<Self, Error> {
        SignalState::new()
            .map(|state| Self(Arc::new(state)))
            .map_err(Error::Signal)
    }

    /// A received cancellation remains set until the entire surrounding operation is dropped.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.received.load(Ordering::Acquire) != 0
    }
}

/// Receive a machine result while the component owns interactive input and live diagnostics.
///
/// Native work retains its owner's duration policy. Cancellation forwards the exact Unix signal
/// to the direct child and gives its rollback thirty seconds before the final kill/reap fallback.
///
/// # Errors
/// Rejects cancellation, invalid replies, nonzero exits, oversized stdout and process failures.
pub fn exchange(
    component: Component,
    binary: &Path,
    args: &[OsString],
    operation: &str,
    scope: &Operation,
) -> Result<Value, Error> {
    if scope.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let child = Command::new(binary)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|source| Error::Process { component, source })?;
    let mut child = OwnedChild(child);
    let stdout = child.0.stdout.take().ok_or(Error::InvalidResponse)?;
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("eska-component-reply".into())
        .spawn(move || {
            let _ = sender.send(bounded(component, stdout));
        })
        .map_err(|source| Error::Process { component, source })?;
    let mut output = None;
    let mut status = None;
    let mut cancellation = None;
    loop {
        match receiver.try_recv() {
            Ok(result) => output = Some(result?),
            Err(mpsc::TryRecvError::Disconnected) if output.is_none() => {
                return Err(Error::InvalidResponse);
            }
            Err(_) => {}
        }
        if status.is_none() {
            status = child
                .0
                .try_wait()
                .map_err(|source| Error::Process { component, source })?;
        }
        if let Some(status) = status
            && let Some(output) = &output
        {
            return if scope.is_cancelled() {
                Err(Error::Cancelled)
            } else {
                decode(component, status.success(), output, operation)
            };
        }
        if scope.is_cancelled() && cancellation.is_none() {
            forward_cancel(&child, scope).map_err(Error::Signal)?;
            cancellation = Some(Instant::now());
        }
        if cancellation.is_some_and(|started| started.elapsed() >= Duration::from_secs(30)) {
            return Err(Error::Cancelled);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// Never broadcast to the caller's group or enumerate unrelated native processes.
#[cfg(unix)]
fn forward_cancel(child: &OwnedChild, scope: &Operation) -> io::Result<()> {
    use nix::{
        errno::Errno,
        sys::signal::{Signal, kill},
        unistd::Pid,
    };
    let pid = i32::try_from(child.0.id()).map_err(io::Error::other)?;
    let signal =
        i32::try_from(scope.0.received.load(Ordering::Acquire)).map_err(io::Error::other)?;
    match kill(
        Pid::from_raw(pid),
        Signal::try_from(signal).map_err(io::Error::other)?,
    ) {
        Ok(()) | Err(Errno::ESRCH) => Ok(()),
        Err(error) => Err(io::Error::from(error)),
    }
}

/// Console cancellation reaches both attached processes on Windows.
#[cfg(windows)]
const fn forward_cancel(_: &OwnedChild, _: &Operation) -> io::Result<()> {
    Ok(())
}
