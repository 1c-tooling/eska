//! Read-only IDE protocol presentation, separate from CLI JSON and the metadata core.

pub(in crate::cli) mod dto;
pub(in crate::cli) mod editing;
mod envelope;
mod errors;
mod events;
mod framing;
mod metadata;
mod params;
mod property_presentation;
mod property_types;
mod runtime;
mod server;

/// Run a persistent framed protocol session without human output on stdout.
pub(super) fn run() -> std::process::ExitCode {
    std::process::ExitCode::from(runtime::run())
}

#[cfg(test)]
mod tests;
