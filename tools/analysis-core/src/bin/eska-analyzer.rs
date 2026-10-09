//! Standalone IDE transport uses the shared analysis session without platform or Git orchestration.

/// Keep process setup and localized help in the transport consumer.
fn main() -> std::process::ExitCode {
    eska_analysis::analyzer_cli::run()
}
