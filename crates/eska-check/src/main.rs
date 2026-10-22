//! Standalone CI entry point; no full CLI, metadata server, Git or platform runtime is linked.

mod cli;
mod report;

/// Preserve the check result as a process exit status for CI runners.
fn main() -> std::process::ExitCode {
    cli::run()
}
