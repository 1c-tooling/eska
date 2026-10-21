//! Localized presentation of component transport failures.

use crate::localization::{LocalizationValue, Localizer};
use eska_tool_runtime::Error;
pub use eska_tool_runtime::contract::respond as tool_contract;

/// Present structured client failures without putting transport diagnostics on stdout.
#[must_use]
pub fn present(error: &Error, localizer: &Localizer) -> String {
    match error {
        Error::Missing { component } => localizer.format(
            "component-missing",
            &[
                ("tool", LocalizationValue::Text(component.name())),
                ("variable", LocalizationValue::Text(component.variable())),
            ],
        ),
        Error::Invalid {
            component,
            path,
            source,
        } => localizer.format(
            "component-invalid",
            &[
                ("tool", LocalizationValue::Text(component.name())),
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
                ("reason", LocalizationValue::Text(&source.to_string())),
            ],
        ),
        Error::Incompatible { tool, receipt } => localizer.format(
            "component-incompatible",
            &[
                ("tool", LocalizationValue::Text(tool)),
                (
                    "receipt",
                    LocalizationValue::Text(&receipt.to_string_lossy()),
                ),
            ],
        ),
        Error::Signal(source) => localizer.format(
            "component-signal-error",
            &[("reason", LocalizationValue::Text(&source.to_string()))],
        ),
        Error::Process { component, source } => localizer.format(
            "component-process-error",
            &[
                ("tool", LocalizationValue::Text(component.name())),
                ("reason", LocalizationValue::Text(&source.to_string())),
            ],
        ),
        Error::Selection { component, path } => localizer.format(
            "component-invalid",
            &[
                ("tool", LocalizationValue::Text(component.name())),
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
                (
                    "reason",
                    LocalizationValue::Text(&localizer.text("component-executable-required")),
                ),
            ],
        ),
        Error::Timeout => localizer.text("component-timeout"),
        Error::Cancelled => localizer.text("component-cancelled"),
        Error::InvalidResponse => localizer.text("component-response-invalid"),
        Error::OutputTooLarge => localizer.text("component-output-too-large"),
        Error::Failed => localizer.text("component-failed"),
        Error::Rejected {
            component, code, ..
        } => localizer.format(
            "component-rejected",
            &[
                ("tool", LocalizationValue::Text(component.name())),
                ("code", LocalizationValue::Text(code)),
            ],
        ),
    }
}

/// Refuse an incompatible managed installation after help/version parsing, before project writes.
///
/// # Errors
/// Prints the localized installation error and returns the failure status.
pub fn verify_installation(
    tool: &str,
    localizer: &Localizer,
) -> Result<(), std::process::ExitCode> {
    let result = std::env::current_exe()
        .map_err(|_| Error::Failed)
        .and_then(|binary| eska_tool_runtime::installation::verify(&binary, tool));
    result.map_err(|error| {
        eprintln!("{}", present(&error, localizer));
        std::process::ExitCode::FAILURE
    })
}
