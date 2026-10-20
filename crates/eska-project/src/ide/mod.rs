//! Read-only IDE protocol presentation, separate from CLI JSON and the metadata core.

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
pub(crate) mod renaming;
mod runtime;
mod server;

/// Run a persistent framed protocol session without human output on stdout.
#[must_use]
pub fn run() -> std::process::ExitCode {
    std::process::ExitCode::from(runtime::run())
}

#[cfg(test)]
mod tests;
