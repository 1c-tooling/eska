//! Unix discovery owns both the adapter and its recorded container-side version probe.

use super::{Runner, interruptible_runner_command, terminate_distrobox_process};
use eska_tool_runtime::{Component, Error, Operation, QueryPolicy};
use std::{
    fs, io,
    os::unix::fs::DirBuilderExt,
    path::PathBuf,
    process::Output,
    time::{SystemTime, UNIX_EPOCH},
};

struct Directory(PathBuf);

impl Drop for Directory {
    /// Remove only the exclusively claimed control namespace after its process has been reaped.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A version probe has finite output/duration and records cancellation before native setup.
pub(super) fn output(
    runner: &Runner,
    arguments: &[std::ffi::OsString],
    outer: Option<&Operation>,
) -> io::Result<Output> {
    let scope = outer.map_or_else(
        || Operation::new().map_err(io_error),
        |scope| Ok(scope.clone()),
    )?;
    let directory = if matches!(runner, Runner::Distrobox { .. }) {
        Some(directory()?)
    } else {
        None
    };
    let pid = directory
        .as_ref()
        .map_or_else(PathBuf::new, |directory| directory.0.join("process.pid"));
    let mut command = interruptible_runner_command(runner, arguments, &pid);
    eska_tool_runtime::capture(
        Component::Build,
        &mut command,
        &scope,
        QueryPolicy::default(),
        || {
            terminate_distrobox_process(runner, &pid);
        },
    )
    .map_err(io_error)
}

/// Keep Distrobox control files in shared user storage; private container /tmp is not shared.
fn directory() -> io::Result<Directory> {
    let base = control_root()?;
    fs::create_dir_all(&base)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    for attempt in 0..32 {
        let path = base.join(format!(
            "eska-probe-{}-{stamp}-{attempt}",
            std::process::id()
        ));
        match fs::DirBuilder::new().mode(0o700).create(&path) {
            Ok(()) => {
                let mut directory = Directory(path);
                directory.0 = directory.0.canonicalize()?;
                return Ok(directory);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::ErrorKind::AlreadyExists.into())
}

/// An isolated machine profile keeps its transient control files in the same shared test stand.
fn control_root() -> io::Result<PathBuf> {
    if let Some(config) = std::env::var_os("ESKA_CONFIG_DIR") {
        let config = PathBuf::from(config);
        return Ok(config
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(".eska-probes"));
    }
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".cache/eska/probes"))
        .ok_or_else(|| io::ErrorKind::NotFound.into())
}

/// Preserve OS causes while cancellation, timeout and size failures retain stable IO categories.
fn io_error(error: Error) -> io::Error {
    match error {
        Error::Signal(source) | Error::Process { source, .. } => source,
        Error::Cancelled => io::ErrorKind::Interrupted.into(),
        Error::Timeout => io::ErrorKind::TimedOut.into(),
        Error::OutputTooLarge | Error::InvalidResponse => io::ErrorKind::InvalidData.into(),
        error => io::Error::other(error.code()),
    }
}
