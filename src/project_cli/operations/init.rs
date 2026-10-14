//! Attach existing ESKA sources through the shared read-only plan and exclusive control publication.

use std::path::Path;

use serde_json::{Value, json};

use crate::{
    project_cli::args::Init,
    project_core::{
        ProjectName,
        platform_settings::PlatformVersion,
        source_format::{SourcePath, prepare_project_initialization},
    },
    tool_cli::{
        self, Failure,
        localization::{LocalizationValue, Localizer},
        path_text,
    },
};

/// Present candidates from one explicit initialization; no platform/Git or workspace enrollment runs here.
pub(super) fn initialize(
    base: &Path,
    args: &Init,
    localizer: &Localizer,
) -> Result<(Value, String), Failure> {
    let root = base.join(&args.path);
    path_text(&root)?;
    let source = SourcePath::parse(&args.source)?;
    let name = ProjectName::parse(args.name.clone())
        .map_err(|_| Failure::new("project_name_invalid").at("name"))?;
    let platform = args
        .platform_version
        .as_deref()
        .map(PlatformVersion::parse)
        .transpose()
        .map_err(|_| Failure::new("platform_version_invalid").at("platform_version"))?;
    let approved = if args.accepted_base_sha256.is_some()
        || std::env::var_os("ESKA_ACCEPTED_BASE_SHA256").is_some()
    {
        Some(tool_cli::accepted_ref(
            args.accepted_base_sha256.as_deref(),
        )?)
    } else {
        None
    };
    let plan =
        prepare_project_initialization(&root, source, name, platform.as_ref(), approved.as_ref())?;
    path_text(plan.root())?;
    let initialized = plan.publish()?;
    let root = path_text(initialized.project.root())?;
    let ancestor = initialized
        .initial_ancestor
        .as_ref()
        .ok_or_else(|| Failure::new("source_ancestor_missing"))?;
    let data = json!({"root":root,"name":args.name,"type":initialized.project.configuration().project_type().as_str(),
        "metadata_name":plan.metadata_name(),"version":plan.version().to_string(),"version_scheme":plan.version().scheme().as_str(),
        "source_format":"eska","source_layout":"queries","source_layout_version":plan.source_layout_version(),
        "source":path_text(initialized.project.source())?,"accepted_base":".eska/accepted",
        "accepted_base_sha256":initialized.accepted.sha256,"ancestor_sha256":ancestor.sha256(),
        "manifest_sha256":initialized.manifest_sha256,"accepted_base_created":plan.accepted_base_created(),
        "ancestry_preserved":plan.ancestry_preserved()});
    let human = localizer.format(
        "tool-project-initialized",
        &[
            ("path", LocalizationValue::Text(root)),
            ("pin", LocalizationValue::Text(&initialized.accepted.sha256)),
            ("ancestor", LocalizationValue::Text(ancestor.sha256())),
        ],
    );
    Ok((data, human))
}
