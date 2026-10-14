//! Single delegated operations own their journals/resources and retain the caller's raw streams.

use std::{
    convert::Infallible,
    process::{self, Command, ExitStatus, Stdio},
};

/// The shell owns no child journal; failure to launch/wait cannot grant cleanup authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::cli) struct ExecutionFailed;

/// Wait for the complete child, then preserve its exact numeric status at the CLI boundary.
///
/// The caller must hold no publication transaction, owned temporary directory or interrupt
/// handler that needs Drop. Compound new/init use captured results instead. Windows i32
/// statuses cannot be represented by `ExitCode::from(u8)`, so this boundary exits explicitly.
pub(in crate::cli) fn execute(command: &mut Command) -> Result<Infallible, ExecutionFailed> {
    let status = command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|_| ExecutionFailed)?;
    process::exit(status_code(status))
}

/// Unix signal termination follows its shell convention; Windows numeric values remain exact.
fn status_code(status: ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    /// A waited numeric result and signal use the same mapping as the existing build boundary.
    fn maps_numeric_and_signal_statuses() {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(status_code(ExitStatus::from_raw(23 << 8)), 23);
        assert_eq!(status_code(ExitStatus::from_raw(15)), 143);
    }

    #[cfg(windows)]
    #[test]
    /// Windows status conversion must not silently truncate high numeric child exit values.
    fn retains_full_windows_numeric_status() {
        use std::os::windows::process::ExitStatusExt;
        assert_eq!(status_code(ExitStatus::from_raw(1025)), 1025);
    }
}
