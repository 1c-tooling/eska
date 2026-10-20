//! Standalone project executable; domain operations remain in shared libraries.

/// Preserve the library's exit status without additional output or wrapper discovery.
fn main() -> std::process::ExitCode {
    eska_project::run()
}
