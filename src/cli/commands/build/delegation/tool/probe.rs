//! A capabilities probe has bounded memory and time; actual build streams are never captured.

use std::{
    io::{self, Read},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;

use super::Error;
use crate::project::ProjectType;

const MAX_BYTES: usize = 65_536;
const DEADLINE: Duration = Duration::from_secs(5);

/// Kill and reap only the probe child on every failed observation or early return.
struct RunningProbe(Child);

impl Drop for RunningProbe {
    /// A successful `try_wait` has already reaped the process; all other paths need cleanup.
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

/// A reader may retain an inherited pipe, so failed probes never wait on its thread indefinitely.
pub(super) fn capture(executable: &Path, locale: &str) -> Result<Vec<u8>, Error> {
    let child = Command::new(executable)
        .args(["--lang", locale, "capabilities", "--format", "json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| Error::Unavailable)?;
    let mut probe = RunningProbe(child);
    let stdout = probe.0.stdout.take().ok_or(Error::ProbeFailed)?;
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::Builder::new()
        .name("eska-build-capabilities".into())
        .spawn(move || {
            let mut bytes = Vec::new();
            let result = stdout
                .take((MAX_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
                .map(|_| bytes);
            let _ = sender.send(result);
        })
        .map_err(|_| Error::ProbeFailed)?;
    await_output(&mut probe, &receiver)
}

/// Both a successful exit and complete bounded stdout are required before decoding any JSON.
fn await_output(
    probe: &mut RunningProbe,
    receiver: &Receiver<io::Result<Vec<u8>>>,
) -> Result<Vec<u8>, Error> {
    let start = Instant::now();
    let mut output = None;
    loop {
        if output.is_none() {
            match receiver.try_recv() {
                Ok(Ok(bytes)) if bytes.len() > MAX_BYTES => return Err(Error::ProbeOversized),
                Ok(Ok(bytes)) => output = Some(bytes),
                Ok(Err(_)) | Err(TryRecvError::Disconnected) => return Err(Error::ProbeFailed),
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some(status) = probe.0.try_wait().map_err(|_| Error::ProbeFailed)? {
            if !status.success() {
                return Err(Error::ProbeFailed);
            }
            if let Some(bytes) = output {
                return Ok(bytes);
            }
        }
        if start.elapsed() >= DEADLINE {
            return Err(Error::ProbeTimeout);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// Package versions must match exactly; a protocol or advertised artifact mismatch is never retried.
pub(super) fn validate(bytes: &[u8], project_type: ProjectType) -> Result<(), Error> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| Error::Incompatible)?;
    let data = &value["data"];
    if value["schema_version"] != 1
        || value["tool"] != "eska-build"
        || value["command"] != "capabilities"
        || value["status"] != "success"
        || data["version"] != env!("CARGO_PKG_VERSION")
        || data["protocol_version"] != 1
        || !advertises(data, "commands", "build")
        || !advertises(data, "source_formats", "eska")
        || !advertises(data, "eska_artifact_types", project_type.as_str())
    {
        return Err(Error::Incompatible);
    }
    Ok(())
}

/// Capability arrays must contain actual JSON strings, without textual coercion or substring matching.
fn advertises(data: &Value, key: &str, expected: &str) -> bool {
    data[key]
        .as_array()
        .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(expected)))
}
