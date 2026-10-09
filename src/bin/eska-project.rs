//! Standalone offline project operations; the facade and platform are not required.

/// Run the shared CLI presentation and the project-owned operations.
fn main() -> std::process::ExitCode {
    eska::project_cli::run()
}
