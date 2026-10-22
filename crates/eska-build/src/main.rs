//! Native tooling starts directly without locating the full ESKA CLI.

/// Preserve the specialized command's status and output contract.
fn main() -> std::process::ExitCode {
    eska_build::run()
}
