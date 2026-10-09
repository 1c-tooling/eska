//! Independent native builds through the shared runner and protected source pipeline.

/// Run build-owned operations without the facade, Git or language-analysis runtime.
fn main() -> std::process::ExitCode {
    eska::build_cli::run()
}
