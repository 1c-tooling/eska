//! Compound creation waits for child completion before returning control to owned staging/onboarding.

use std::{
    io::{self, Read},
    process::{Command, ExitStatus, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

/// The operation result stays separate from transport faults and never terminates the parent process.
pub(in crate::cli) struct Captured {
    pub(in crate::cli) status: ExitStatus,
    pub(in crate::cli) stdout: Vec<u8>,
    pub(in crate::cli) oversized: bool,
}

/// Failed transport cannot authorize cleanup of a destination that the child may have published.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cli) enum Error {
    Spawn,
    Wait,
    Output,
}

/// Drain oversized output without blocking a child that owns a newly published destination.
struct Bytes {
    value: Vec<u8>,
    oversized: bool,
}

/// Operations may be long-running; only post-exit pipe completion has a short transport deadline.
pub(in crate::cli) fn capture(command: &mut Command, max_bytes: usize) -> Result<Captured, Error> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|_| Error::Spawn)?;
    let stdout = child.stdout.take().ok_or(Error::Output);
    let reader = stdout.and_then(|stdout| {
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("eska-new-child-output".into())
            .spawn(move || {
                let _ = sender.send(drain(stdout, max_bytes));
            })
            .map_err(|_| Error::Output)?;
        Ok(receiver)
    });
    // Wait even if the reader cannot start. The child owns publication and must be allowed to
    // finish its cleanup/report; terminating it here would destroy that ownership boundary.
    let status = child.wait().map_err(|_| Error::Wait)?;
    let bytes = reader?
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| Error::Output)?
        .map_err(|_| Error::Output)?;
    Ok(Captured {
        status,
        stdout: bytes.value,
        oversized: bytes.oversized,
    })
}

/// Memory stays within the caller's bound while every remaining stdout byte is consumed.
fn drain(mut output: impl Read, max_bytes: usize) -> io::Result<Bytes> {
    let mut result = Bytes {
        value: Vec::new(),
        oversized: false,
    };
    let mut buffer = [0_u8; 8192];
    loop {
        let read = output.read(&mut buffer)?;
        if read == 0 {
            return Ok(result);
        }
        let retained = read.min(max_bytes.saturating_sub(result.value.len()));
        result.value.extend_from_slice(&buffer[..retained]);
        result.oversized |= retained != read;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// A full child result is drained even after the retained machine-response bound is exceeded.
    fn retains_only_bounded_prefix_and_reports_overflow() {
        let original = vec![b'x'; 20_000];
        let result = drain(original.as_slice(), 100).unwrap();
        assert_eq!(result.value, vec![b'x'; 100]);
        assert!(result.oversized);
        let result = drain(original.as_slice(), original.len()).unwrap();
        assert_eq!(result.value, original);
        assert!(!result.oversized);
    }
}
