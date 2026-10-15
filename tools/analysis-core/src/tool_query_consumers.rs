//! Presentation prose is shared by IDE and human checker output; machine reports contain no locale.

use eska::tool_cli::localization::{LocalizationValue, Localizer};

use crate::QueryConsumerDiagnosticReport;

/// A configured binding's identifier is displayed without guessing query or resource semantics.
pub fn message(finding: &QueryConsumerDiagnosticReport, localizer: &Localizer) -> String {
    localizer.format(
        "tool-query-consumer-missing-field",
        &[("field", LocalizationValue::Text(&finding.output_field))],
    )
}
