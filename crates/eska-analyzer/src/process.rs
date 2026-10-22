//! Bounded subprocess I/O, cancellation and a pinned published CLI contract.

use std::{
    ffi::OsStr,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{ChildStdout, Command, ExitStatus, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;

use crate::Error;

pub const SUPPORTED_VERSION: &str = "0.3.2";
const MAX_OUTPUT: u64 = 4 * 1024 * 1024;

/// The selected executable and operation limits do not locate a wrapper or a platform.
pub struct Analyzer<'a> {
    pub binary: PathBuf,
    pub timeout: Duration,
    pub cancelled: &'a AtomicBool,
    pub connection_closed: Option<&'a AtomicBool>,
}

/// Captured output is bounded; a successful exit never hides truncation or a failed reader.
pub struct Output {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Streaming consumers choose their own bounded representation instead of buffering raw JSONL.
pub(crate) struct StreamOutput<T> {
    pub status: ExitStatus,
    pub stdout: T,
    pub stderr: Vec<u8>,
}

impl Analyzer<'_> {
    /// A dropped editor connection cancels validation even before a queued shutdown can run.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
            || self
                .connection_closed
                .is_some_and(|closed| closed.load(Ordering::Acquire))
    }
    /// Verify both the selected release and the commands used by this integration.
    ///
    /// # Errors
    /// Rejects unsupported releases, malformed contracts, process failures and cancellation.
    pub fn validate(&self, root: &Path) -> Result<(), Error> {
        let output = self.run(root, [OsStr::new("--version")])?;
        let actual = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !output.status.success() || actual != format!("bsl-analyzer {SUPPORTED_VERSION}") {
            return Err(Error::AnalyzerVersion { actual });
        }
        let output = self.run(root, [OsStr::new("contract")])?;
        let contract: Value =
            serde_json::from_slice(&output.stdout).map_err(|_| Error::AnalyzerContract)?;
        if !output.status.success()
            || contract["build_version"] != SUPPORTED_VERSION
            || contract["contract_version"] != "3.4"
        {
            return Err(Error::AnalyzerContract);
        }
        let commands = contract["cli"]["commands"]
            .as_array()
            .ok_or(Error::AnalyzerContract)?;
        for name in ["check-config", "analyze", "lsp"] {
            let command = commands
                .iter()
                .find(|command| command["name"] == name)
                .ok_or(Error::AnalyzerContract)?;
            if name != "lsp"
                && !command["args"]
                    .as_array()
                    .is_some_and(|args| args.iter().any(|arg| arg["name"] == "config"))
            {
                return Err(Error::AnalyzerContract);
            }
        }
        Ok(())
    }

    /// Validate a config in its final parent directory so every relative policy path keeps its meaning.
    ///
    /// # Errors
    /// Retains upstream validation output and rejects nonzero exits or interrupted validation.
    pub fn check_config(&self, root: &Path, config: &Path) -> Result<(), Error> {
        let output = self.run(
            root,
            [
                OsStr::new("check-config"),
                OsStr::new("--config"),
                config.as_os_str(),
            ],
        )?;
        if output.status.success() {
            Ok(())
        } else {
            Err(Error::CheckConfig {
                exit_code: output.status.code(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        }
    }

    /// Drain both streams concurrently, then reap the child on success, failure, timeout or cancellation.
    ///
    /// # Errors
    /// Rejects spawn/read/wait failures, oversized output, timeout and explicit cancellation.
    pub fn run<'a>(
        &self,
        root: &Path,
        args: impl IntoIterator<Item = &'a OsStr>,
    ) -> Result<Output, Error> {
        let output = self.run_with_stdout(root, args, read_bounded)?;
        Ok(Output {
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }

    /// Share cancellation, bounded stderr and reaping with the incremental JSONL reader.
    pub(crate) fn run_with_stdout<'a, T: Send + 'static>(
        &self,
        root: &Path,
        args: impl IntoIterator<Item = &'a OsStr>,
        read_stdout: impl FnOnce(ChildStdout) -> Result<T, Error> + Send + 'static,
    ) -> Result<StreamOutput<T>, Error> {
        if self.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let mut child = Command::new(&self.binary)
            .args(args)
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| Error::ProcessStart {
                binary: self.binary.clone(),
                source,
            })?;
        let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::ProcessIo(io::Error::other(
                "capture pipes unavailable",
            )));
        };
        let stdout = match thread::Builder::new()
            .name("eska-analyzer-stdout".into())
            .spawn(move || read_stdout(stdout))
        {
            Ok(reader) => reader,
            Err(source) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::ProcessIo(source));
            }
        };
        let stderr = match thread::Builder::new()
            .name("eska-analyzer-stderr".into())
            .spawn(|| read_bounded(stderr))
        {
            Ok(reader) => reader,
            Err(source) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout.join();
                return Err(Error::ProcessIo(source));
            }
        };
        let started = Instant::now();
        let result = loop {
            if self.is_cancelled() {
                break Err(Error::Cancelled);
            }
            if started.elapsed() >= self.timeout {
                break Err(Error::Timeout);
            }
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(source) => break Err(Error::ProcessIo(source)),
            }
        };
        if result.is_err() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let stdout = stdout.join();
        let stderr = stderr.join();
        let stdout =
            stdout.map_err(|_| Error::ProcessIo(io::Error::other("stdout reader stopped")))?;
        let stderr =
            stderr.map_err(|_| Error::ProcessIo(io::Error::other("stderr reader stopped")))?;
        let status = result?;
        Ok(StreamOutput {
            status,
            stdout: stdout?,
            stderr: stderr?,
        })
    }
}

/// Retain a bounded prefix while draining the rest so output saturation cannot deadlock a child.
fn read_bounded(mut stream: impl Read) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    stream
        .by_ref()
        .take(MAX_OUTPUT + 1)
        .read_to_end(&mut bytes)
        .map_err(Error::ProcessIo)?;
    if bytes.len() as u64 > MAX_OUTPUT {
        io::copy(&mut stream, &mut io::sink()).map_err(Error::ProcessIo)?;
        return Err(Error::OutputTooLarge);
    }
    Ok(bytes)
}
