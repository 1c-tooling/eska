//! Standalone high-level Git CLI.

/// Return the specialized command status unchanged.
fn main() -> std::process::ExitCode {
    eska_vcs::run()
}
