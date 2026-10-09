//! A standalone batch consumer of the same analysis session used by IDE queries.

/// Keep parsing, localization and process exit codes outside the shared analysis core.
fn main() -> std::process::ExitCode {
    eska_analysis::check_cli::run()
}
