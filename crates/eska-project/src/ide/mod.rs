//! Versioned Metadata API presentation, separate from CLI JSON and the project core.

mod analyzer;
pub(crate) mod dto;
pub(crate) mod editing;
mod envelope;
pub(crate) mod errors;
mod events;
mod framing;
mod metadata;
mod params;
mod property_presentation;
mod property_types;
mod query;
pub(crate) mod renaming;
mod runtime;
mod server;

pub(crate) const API_MINOR: u32 = 13;

/// Run a persistent framed protocol session without human output on stdout.
#[must_use]
pub fn run() -> std::process::ExitCode {
    std::process::ExitCode::from(runtime::run(server::Identity::Legacy))
}

/// Run the standalone project component with its own identity and no monolithic updater capability.
#[must_use]
pub fn serve() -> std::process::ExitCode {
    std::process::ExitCode::from(runtime::run(server::Identity::Project))
}

#[cfg(test)]
mod tests;
