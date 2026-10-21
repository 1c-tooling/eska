//! Bounded raw process capture shares the same ownership and cancellation rules as private IPC.

use crate::{
    Component, Error, Operation, QueryPolicy,
    process::OwnedChild,
    query::{Stream, reader},
};
#[cfg(unix)]
const TERMINATE: i32 = signal_hook::consts::signal::SIGTERM;
#[cfg(not(unix))]
const TERMINATE: i32 = 0;

use std::{
    process::{Command, Output, Stdio},
    sync::mpsc,
    thread,
    time::Instant,
};

/// Capture one owned subprocess, invoking native cleanup before the final kill/reap on failure.
///
/// # Errors
/// Returns process, stream-limit, timeout or cancellation failures; the exact child is reaped.
pub fn capture(
    component: Component,
    command: &mut Command,
    scope: &Operation,
    policy: QueryPolicy,
    on_failure: impl FnOnce(),
) -> Result<Output, Error> {
    run(component, command, Some(scope), policy, on_failure)
}

/// Queries on Windows preserve their existing console controller rather than installing another.
pub fn run(
    component: Component,
    command: &mut Command,
    scope: Option<&Operation>,
    policy: QueryPolicy,
    on_failure: impl FnOnce(),
) -> Result<Output, Error> {
    if scope.is_some_and(Operation::is_cancelled) {
        return Err(Error::Cancelled);
    }
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| Error::Process { component, source })?;
    let mut child = OwnedChild(child);
    let result = receive(component, &mut child, scope, policy);
    if result.is_err() && !matches!(child.0.try_wait(), Ok(Some(_))) {
        on_failure();
    }
    result
}

/// Drain both pipes concurrently; a graceful owner can clean descendants before its own exit.
fn receive(
    component: Component,
    child: &mut OwnedChild,
    scope: Option<&Operation>,
    policy: QueryPolicy,
) -> Result<Output, Error> {
    let stdout = child.0.stdout.take().ok_or(Error::InvalidResponse)?;
    let stderr = child.0.stderr.take().ok_or(Error::InvalidResponse)?;
    let (sender, receiver) = mpsc::channel();
    reader(
        component,
        stdout,
        Stream::Stdout,
        sender.clone(),
        policy.output_limit,
    )?;
    reader(
        component,
        stderr,
        Stream::Stderr,
        sender,
        policy.output_limit,
    )?;
    let started = Instant::now();
    let mut stopping = None;
    let mut status = None;
    let mut stdout = None;
    let mut stderr = None;
    loop {
        while let Ok((stream, result)) = receiver.try_recv() {
            match stream {
                Stream::Stdout => stdout = Some(result?),
                Stream::Stderr => stderr = Some(result?),
            }
        }
        let cancelled = scope.is_some_and(Operation::is_cancelled);
        if stopping.is_none() && (cancelled || started.elapsed() >= policy.timeout) {
            if policy.cancellation_grace.is_zero() {
                return Err(reason(cancelled));
            }
            signal(
                child,
                scope
                    .and_then(Operation::received_signal)
                    .unwrap_or(TERMINATE),
            )?;
            stopping = Some((Instant::now(), cancelled));
        }
        if status.is_none() {
            status = child
                .0
                .try_wait()
                .map_err(|source| Error::Process { component, source })?;
        }
        if let Some((started, cancelled)) = stopping {
            if status.is_some() || started.elapsed() >= policy.cancellation_grace {
                return Err(reason(cancelled));
            }
        } else if let Some(status) = status
            && stdout.is_some()
            && stderr.is_some()
        {
            return Ok(Output {
                status,
                stdout: stdout.take().ok_or(Error::InvalidResponse)?,
                stderr: stderr.take().ok_or(Error::InvalidResponse)?,
            });
        }
        thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Preserve the initiating failure even when the child exits successfully during cleanup.
const fn reason(cancelled: bool) -> Error {
    if cancelled {
        Error::Cancelled
    } else {
        Error::Timeout
    }
}

/// Signal only the owned process; its native owner handles any container-side PID.
#[cfg(unix)]
fn signal(child: &OwnedChild, signal: i32) -> Result<(), Error> {
    use nix::{
        errno::Errno,
        sys::signal::{Signal, kill},
        unistd::Pid,
    };
    let pid =
        i32::try_from(child.0.id()).map_err(|error| Error::Signal(std::io::Error::other(error)))?;
    let signal = Signal::try_from(signal).map_err(|error| Error::Signal(error.into()))?;
    match kill(Pid::from_raw(pid), signal) {
        Ok(()) | Err(Errno::ESRCH) => Ok(()),
        Err(error) => Err(Error::Signal(error.into())),
    }
}

/// Windows timeout fallback retains the previous direct-child termination behavior.
#[cfg(not(unix))]
fn signal(child: &mut OwnedChild, _: i32) -> Result<(), Error> {
    child.0.kill().map_err(Error::Signal)
}
