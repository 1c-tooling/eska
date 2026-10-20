//! Localized presentation of component transport failures.

use crate::localization::{LocalizationValue, Localizer};
use eska_tool_runtime::Error;

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
        Error::Rejected { component, code } => localizer.format(
            "component-rejected",
            &[
                ("tool", LocalizationValue::Text(component.name())),
                ("code", LocalizationValue::Text(code)),
            ],
        ),
    }
}
