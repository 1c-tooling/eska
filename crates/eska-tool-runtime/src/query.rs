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

/// A caller selects finite limits for its concrete response, without changing other operations.
#[derive(Clone, Copy, Debug)]
pub struct QueryPolicy {
    pub timeout: Duration,
    pub output_limit: u64,
}

impl Default for QueryPolicy {
    /// Ordinary metadata checks retain the original thirty-second, one-MiB bounds per stream.
    fn default() -> Self {
        Self {
            timeout: TIMEOUT,
            output_limit: MAX_OUTPUT,
        }
    }
}

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
    query_with_policy(
        component,
        binary,
        operation,
        root,
        options,
        QueryPolicy::default(),
    )
}

/// Receive one response under explicit operation limits, with the same strict framing and cleanup.
///
/// # Errors
/// Rejects timeouts, cancellation, oversized output, failed children and invalid protocol replies.
pub fn query_with_policy(
    component: Component,
    binary: &Path,
    operation: &str,
    root: &Path,
    options: &[OsString],
    policy: QueryPolicy,
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
        if started.elapsed() >= policy.timeout {
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
    decode(component, status.success(), &stdout, operation)
}

/// Both IPC transports share framing and domain-error validation.
pub fn decode(
    component: Component,
    success: bool,
    stdout: &[u8],
    operation: &str,
) -> Result<Value, Error> {
    if !success {
        if let Ok(Value::Object(document)) = serde_json::from_slice(stdout)
            && document.len() == 4
            && document.get("schema_version") == Some(&Value::from(1))
            && document.get("operation").and_then(Value::as_str) == Some(operation)
            && document.get("data") == Some(&Value::Null)
            && let Some((code, detail)) = document.get("error").and_then(rejection)
        {
            return Err(Error::Rejected {
                component,
                code,
                detail,
            });
        }
        return Err(Error::Failed);
    }
    response(stdout, operation)
}

/// Rich domain failures use a closed shape; legacy string categories remain valid.
fn rejection(value: &Value) -> Option<(String, Option<String>)> {
    let (code, detail) = match value {
        Value::String(code) => (code.as_str(), None),
        Value::Object(error) if error.len() == 2 => (
            error.get("code")?.as_str()?,
            Some(error.get("detail")?.as_str()?.to_owned()),
        ),
        _ => return None,
    };
    if code.is_empty()
        || code.len() > 64
        || !code
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || matches!(byte, b'_' | b'-'))
    {
        return None;
    }
    Some((code.to_owned(), detail))
}

/// Drain one stream concurrently, retaining at most the protocol's fixed maximum.
fn reader(
    component: Component,
    stream: impl Read + Send + 'static,
    kind: Stream,
    sender: mpsc::Sender<(Stream, Result<Vec<u8>, Error>)>,
    limit: u64,
) -> Result<(), Error> {
    thread::Builder::new()
        .name("eska-component-output".into())
        .spawn(move || {
            let result = bounded_with_limit(component, stream, limit);
            let _ = sender.send((kind, result));
        })
        .map(|_| ())
        .map_err(|source| Error::Process { component, source })
}

/// Reading one extra byte distinguishes a complete small response from truncated output.
pub fn bounded(component: Component, stream: impl Read) -> Result<Vec<u8>, Error> {
    bounded_with_limit(component, stream, MAX_OUTPUT)
}

/// One extra byte proves overflow; checked arithmetic also handles caller-supplied limits safely.
fn bounded_with_limit(
    component: Component,
    stream: impl Read,
    limit: u64,
) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    stream
        .take(limit.checked_add(1).ok_or(Error::OutputTooLarge)?)
        .read_to_end(&mut bytes)
        .map_err(|source| Error::Process { component, source })?;
    if bytes.len() as u64 > limit {
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
    use super::{Component, Error, QueryPolicy, decode, query_with_policy};
    use crate::test_support::TestDir;
    use std::{ffi::OsString, fs, path::Path, time::Duration};

    /// Valid domain failures retain details; nonzero partial or malformed replies remain failures.
    #[test]
    fn domain_error_details_do_not_permit_partial_success() {
        let reply = serde_json::json!({"schema_version":1,"operation":"patch_plan","data":null,"error":{"code":"dirty","detail":"module.bsl"}});
        let decode_reply = |value: &serde_json::Value| {
            decode(
                Component::Vcs,
                false,
                &serde_json::to_vec(value).unwrap(),
                "patch_plan",
            )
        };
        assert!(
            matches!(decode_reply(&reply),Err(Error::Rejected{code,detail:Some(detail),..}) if code=="dirty" && detail=="module.bsl")
        );
        let mut partial = reply.clone();
        partial["data"] = serde_json::json!({"prepared":true});
        assert!(matches!(decode_reply(&partial), Err(Error::Failed)));
        let mut unknown = reply.clone();
        unknown["error"]["extra"] = true.into();
        assert!(matches!(decode_reply(&unknown), Err(Error::Failed)));
        let mut malformed = reply;
        malformed["error"]["code"] = "dirty\nhelp".into();
        assert!(matches!(decode_reply(&malformed), Err(Error::Failed)));
    }

    /// A large real JSON reply needs explicit bounds; normal metadata checks retain their limit.
    #[test]
    fn explicit_large_response_policy_does_not_relax_default_bounds() {
        let fixture = TestDir::new();
        let binary =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/component.py");
        let options = [
            OsString::from("--fixture-mode"),
            OsString::from("bridge-large-plan"),
        ];
        assert!(matches!(
            query_with_policy(
                Component::Vcs,
                &binary,
                "provenance",
                &fixture.0,
                &options,
                QueryPolicy::default()
            ),
            Err(Error::OutputTooLarge)
        ));
        let policy = QueryPolicy {
            output_limit: 2 * 1024 * 1024,
            ..QueryPolicy::default()
        };
        let response = query_with_policy(
            Component::Vcs,
            &binary,
            "provenance",
            &fixture.0,
            &options,
            policy,
        )
        .unwrap();
        assert_eq!(response["padding"].as_str().unwrap().len(), 1024 * 1024 + 1);
    }

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
        let result = query_with_policy(
            Component::Vcs,
            &binary,
            "provenance",
            &fixture.0,
            &options,
            QueryPolicy {
                timeout: Duration::from_secs(1),
                ..QueryPolicy::default()
            },
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
