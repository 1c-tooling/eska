//! One bounded internal response, distinct from inherited user-facing command streams.

#[cfg(unix)]
use crate::process::SignalState;
use crate::{Component, Error, process::OwnedChild};
use serde_json::Value;
#[cfg(unix)]
use std::sync::atomic::Ordering;
use std::{
    ffi::OsString,
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const MAX_OUTPUT: u64 = 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}

/// Query a preflighted component without invoking a shell or relying on UTF-8 paths.
///
/// # Errors
/// Rejects malformed schemas, nonzero exits, oversized output, timeout and cancellation.
/// The exact direct child is reaped on every return path.
pub fn query(
    component: Component,
    binary: &Path,
    operation: &str,
    root: &Path,
    options: &[OsString],
) -> Result<Value, Error> {
    query_with_timeout(component, binary, operation, root, options, TIMEOUT)
}

/// Keep the same bounded operation while allowing the process fixture to use a short deadline.
fn query_with_timeout(
    component: Component,
    binary: &Path,
    operation: &str,
    root: &Path,
    options: &[OsString],
    timeout: Duration,
) -> Result<Value, Error> {
    #[cfg(unix)]
    let signal = SignalState::new().map_err(Error::Signal)?;
    let child = Command::new(binary)
        .args(["__bridge", &operation.replace('_', "-"), "--project-dir"])
        .arg(root)
        .args(options)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| Error::Process { component, source })?;
    let mut child = OwnedChild(child);
    let stdout = child.0.stdout.take().ok_or(Error::InvalidResponse)?;
    let stderr = child.0.stderr.take().ok_or(Error::InvalidResponse)?;
    let (sender, receiver) = mpsc::channel();
    reader(component, stdout, Stream::Stdout, sender.clone())?;
    reader(component, stderr, Stream::Stderr, sender)?;
    let started = Instant::now();
    let mut status = None;
    let mut stdout = None;
    let mut stderr = None;
    let (status, stdout, stderr) = loop {
        while let Ok((stream, result)) = receiver.try_recv() {
            let output = Some(result?);
            match stream {
                Stream::Stdout => stdout = output,
                Stream::Stderr => stderr = output,
            }
        }
        if status.is_none() {
            status = child
                .0
                .try_wait()
                .map_err(|source| Error::Process { component, source })?;
        }
        if status.is_some() && stdout.is_some() && stderr.is_some() {
            break (
                status.ok_or(Error::InvalidResponse)?,
                stdout.ok_or(Error::InvalidResponse)?,
                stderr.ok_or(Error::InvalidResponse)?,
            );
        }
        #[cfg(unix)]
        if signal.received.load(Ordering::Acquire) != 0 {
            return Err(Error::Cancelled);
        }
        if started.elapsed() >= timeout {
            return Err(Error::Timeout);
        }
        thread::sleep(Duration::from_millis(10));
    };
    if !stderr.is_empty() {
        std::io::stderr()
            .lock()
            .write_all(&stderr)
            .map_err(|source| Error::Process { component, source })?;
    }
    if !status.success() {
        if let Ok(Value::Object(document)) = serde_json::from_slice(&stdout)
            && document.len() == 4
            && document.get("schema_version") == Some(&Value::from(1))
            && document.get("operation").and_then(Value::as_str) == Some(operation)
            && document.get("data") == Some(&Value::Null)
            && let Some(code) = document.get("error").and_then(Value::as_str)
            && !code.is_empty()
            && code.len() <= 64
            && code
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || matches!(byte, b'_' | b'-'))
        {
            return Err(Error::Rejected {
                component,
                code: code.to_owned(),
            });
        }
        return Err(Error::Failed);
    }
    response(&stdout, operation)
}

/// Drain one stream concurrently, retaining at most the protocol's fixed maximum.
fn reader(
    component: Component,
    stream: impl Read + Send + 'static,
    kind: Stream,
    sender: mpsc::Sender<(Stream, Result<Vec<u8>, Error>)>,
) -> Result<(), Error> {
    thread::Builder::new()
        .name("eska-component-output".into())
        .spawn(move || {
            let result = bounded(component, stream);
            let _ = sender.send((kind, result));
        })
        .map(|_| ())
        .map_err(|source| Error::Process { component, source })
}

/// Reading one extra byte distinguishes a complete small response from truncated output.
fn bounded(component: Component, stream: impl Read) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    stream
        .take(MAX_OUTPUT + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| Error::Process { component, source })?;
    if bytes.len() as u64 > MAX_OUTPUT {
        return Err(Error::OutputTooLarge);
    }
    Ok(bytes)
}

/// Both framing and the domain's success flag must be valid before any payload is trusted.
fn response(stdout: &[u8], operation: &str) -> Result<Value, Error> {
    let Value::Object(mut document) =
        serde_json::from_slice(stdout).map_err(|_| Error::InvalidResponse)?
    else {
        return Err(Error::InvalidResponse);
    };
    if document.len() != 4
        || document.get("schema_version") != Some(&Value::from(1))
        || document.get("operation").and_then(Value::as_str) != Some(operation)
        || document.get("error") != Some(&Value::Null)
    {
        return Err(Error::InvalidResponse);
    }
    document
        .remove("data")
        .filter(|value| !value.is_null())
        .ok_or(Error::InvalidResponse)
}

#[cfg(all(test, unix))]
mod tests {
    use super::{Component, Error, query_with_timeout};
    use crate::test_support::TestDir;
    use std::{ffi::OsString, fs, path::Path, time::Duration};

    /// A stalled real process cannot keep the caller or its direct child alive after timeout.
    #[test]
    fn timed_out_component_is_reaped() {
        let fixture = TestDir::new();
        let ready = fixture.0.join("ready");
        let binary =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/component.py");
        let options = [
            OsString::from("--fixture-mode"),
            OsString::from("bridge-timeout"),
            OsString::from("--fixture-ready"),
            ready.clone().into_os_string(),
        ];
        let result = query_with_timeout(
            Component::Vcs,
            &binary,
            "provenance",
            &fixture.0,
            &options,
            Duration::from_secs(1),
        );
        assert!(matches!(result, Err(Error::Timeout)), "{result:?}");
        let pid: u32 = fs::read_to_string(ready).unwrap().parse().unwrap();
        // /proc is a Linux observation; the same timeout test still runs on other Unix hosts.
        #[cfg(target_os = "linux")]
        assert!(!Path::new("/proc").join(pid.to_string()).exists());
        #[cfg(not(target_os = "linux"))]
        assert!(pid > 0);
    }
}
