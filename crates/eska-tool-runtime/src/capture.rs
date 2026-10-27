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
    io::{self, Write},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicBool, Ordering},
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
    let result = receive(component, &mut child, scope, None, policy);
    if result.is_err() && !matches!(child.0.try_wait(), Ok(Some(_))) {
        on_failure();
    }
    result
}

/// Send one owned request while draining both child streams under timeout and RPC cancellation.
///
/// # Errors
/// Returns process/pipe failures, cancellation and bounded-output failures; the exact child is reaped.
pub fn capture_input(
    component: Component,
    command: &mut Command,
    input: Vec<u8>,
    policy: QueryPolicy,
    cancelled: &AtomicBool,
    closed: &AtomicBool,
) -> Result<Output, Error> {
    if cancelled.load(Ordering::Acquire) || closed.load(Ordering::Acquire) {
        return Err(Error::Cancelled);
    }
    let child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| Error::Process { component, source })?;
    let mut child = OwnedChild(child);
    let mut stdin = child.0.stdin.take().ok_or(Error::InvalidResponse)?;
    let writer = thread::Builder::new()
        .name("eska-component-input".into())
        .spawn(move || stdin.write_all(&input))
        .map_err(|source| Error::Process { component, source })?;
    let result = receive(
        component,
        &mut child,
        None,
        Some((cancelled, closed)),
        policy,
    );
    drop(child);
    let written = writer
        .join()
        .map_err(|_| Error::Process {
            component,
            source: io::Error::other("input writer failed"),
        })
        .and_then(|result| result.map_err(|source| Error::Process { component, source }));
    let output = result?;
    if output.status.success() {
        written?;
    }
    Ok(output)
}

/// Drain both pipes concurrently; a graceful owner can clean descendants before its own exit.
fn receive(
    component: Component,
    child: &mut OwnedChild,
    scope: Option<&Operation>,
    request: Option<(&AtomicBool, &AtomicBool)>,
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
        let cancelled = scope.is_some_and(Operation::is_cancelled)
            || request.is_some_and(|(cancelled, closed)| {
                cancelled.load(Ordering::Acquire) || closed.load(Ordering::Acquire)
            });
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::test_support::TestDir;
    use std::{sync::Arc, time::Duration};

    /// Cancellation also releases a writer blocked by a component that never consumes its input.
    #[test]
    fn cancelling_a_blocked_input_reaps_the_child_and_writer() {
        let directory = TestDir::new();
        let cancelled = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&cancelled);
        let trigger = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            signal.store(true, Ordering::Release);
        });
        let mut command = Command::new("python3");
        command
            .args(["-c", "import time; time.sleep(30)"])
            .current_dir(&directory.0);
        let start = Instant::now();
        let result = capture_input(
            Component::Fmt,
            &mut command,
            vec![b'x'; 1024 * 1024],
            QueryPolicy {
                timeout: Duration::from_secs(3),
                output_limit: 1024,
                cancellation_grace: Duration::ZERO,
            },
            &cancelled,
            &AtomicBool::new(false),
        );
        trigger.join().unwrap();
        assert!(matches!(result, Err(Error::Cancelled)));
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    /// Both pipes drain while the complete input is sent; no partial response or broken-pipe success is accepted.
    #[test]
    fn input_and_output_are_complete_with_bounded_streams() {
        let directory = TestDir::new();
        let input = vec![b'a'; 256 * 1024];
        let mut command = Command::new("python3");
        command.args(["-c","import sys; data=sys.stdin.buffer.read(); sys.stderr.buffer.write(b'checked'); sys.stdout.buffer.write(data)"]).current_dir(&directory.0);
        let output = capture_input(
            Component::Fmt,
            &mut command,
            input.clone(),
            QueryPolicy {
                timeout: Duration::from_secs(3),
                output_limit: 1024 * 1024,
                cancellation_grace: Duration::ZERO,
            },
            &AtomicBool::new(false),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, input);
        assert_eq!(output.stderr, b"checked");
    }
}
