//! Standalone process owns parsing and presentation; formatting is the same pure library used by acceptance tests.

mod cli;

/// Preserve check/error/cancellation status for the optional facade and CI.
fn main() -> std::process::ExitCode {
    cli::run()
}
