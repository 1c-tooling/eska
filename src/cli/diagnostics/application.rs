//! Localized application configuration errors; core retains only structured values.

use std::path::Path;

use crate::{
    application::SettingsError,
    cli::{
        localization::{LocalizationValue, Localizer},
        path_output,
    },
};

/// Preserve the manifest location while rendering a specific configuration failure.
pub(super) fn present(
    localizer: &Localizer,
    path: &Path,
    error: &SettingsError,
    hyperlinks: bool,
) -> String {
    let detail = reason(localizer, error);
    localizer.format(
        "analysis-config-invalid",
        &[
            (
                "path",
                LocalizationValue::Text(&path_output::render(path, hyperlinks)),
            ),
            ("detail", LocalizationValue::Text(&detail)),
        ],
    )
}

/// Describe the rejected relationship without turning machine-facing names into translated keys.
fn reason(localizer: &Localizer, error: &SettingsError) -> String {
    match error {
        SettingsError::InvalidName { field, value } => localizer.format(
            "analysis-name-invalid",
            &[
                ("field", LocalizationValue::Text(field)),
                ("value", LocalizationValue::Text(value)),
            ],
        ),
        SettingsError::InvalidPlatformVersion { value } => localizer.format(
            "analysis-platform-invalid",
            &[("value", LocalizationValue::Text(value))],
        ),
        SettingsError::WorkspaceRequired => localizer.text("analysis-workspace-required"),
        SettingsError::UnknownDefaultApplication { name } => localizer.format(
            "analysis-default-unknown",
            &[("name", LocalizationValue::Text(name.as_str()))],
        ),
        SettingsError::DuplicateMember {
            application,
            member,
        } => member_reason(
            localizer,
            "analysis-member-duplicate",
            application.as_str(),
            member.as_str(),
        ),
        SettingsError::DependencyOutsideApplication {
            application,
            member,
        } => member_reason(
            localizer,
            "analysis-dependency-outside",
            application.as_str(),
            member.as_str(),
        ),
        SettingsError::DependencyCycle { application } => localizer.format(
            "analysis-dependency-cycle",
            &[("application", LocalizationValue::Text(application.as_str()))],
        ),
        SettingsError::UnknownMember {
            application,
            member,
        } => member_reason(
            localizer,
            "analysis-member-unknown",
            application.as_str(),
            member.as_str(),
        ),
        SettingsError::WrongMemberType {
            application,
            member,
            expected,
            actual,
        } => {
            let expected = expected
                .project_types()
                .iter()
                .map(|kind| kind.as_str())
                .collect::<Vec<_>>()
                .join("|");
            localizer.format(
                "analysis-member-type",
                &[
                    ("application", LocalizationValue::Text(application.as_str())),
                    ("member", LocalizationValue::Text(member.as_str())),
                    ("expected", LocalizationValue::Text(&expected)),
                    ("actual", LocalizationValue::Text(actual.as_str())),
                ],
            )
        }
        SettingsError::MemberAnalysisUnsupported { member } => localizer.format(
            "analysis-member-settings",
            &[("member", LocalizationValue::Text(member.as_str()))],
        ),
        SettingsError::TargetPlatformScope => localizer.text("analysis-platform-scope"),
    }
}

/// Reuse the same application/member coordinates for duplicate, unknown and invalid edges.
fn member_reason(localizer: &Localizer, key: &str, application: &str, member: &str) -> String {
    localizer.format(
        key,
        &[
            ("application", LocalizationValue::Text(application)),
            ("member", LocalizationValue::Text(member)),
        ],
    )
}
