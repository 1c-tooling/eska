//! New project reporting retains separate support and initial ancestry candidates for independent confirmation.

use serde_json::{Value, json};
use std::path::Path;

use crate::{
    project_cli::args::New,
    project_core::{
        ProjectName,
        creation::{NewProjectOptions, new_project},
        native_version::ProjectVersion,
        platform_settings::PlatformVersion,
    },
    tool_cli::{
        Failure,
        localization::{LocalizationValue, Localizer},
        path_text,
    },
};

/// Populate one exclusively new minimal project through the same authoring/authority pipeline as import.
pub(super) fn create(
    base: &Path,
    args: &New,
    localizer: &Localizer,
) -> Result<(Value, String), Failure> {
    let destination = base.join(&args.destination);
    path_text(&destination)?;
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = parent
        .canonicalize()
        .map_err(|_| Failure::new("source_file_missing"))?;
    path_text(&parent)?;
    let name = ProjectName::parse(args.name.clone())
        .map_err(|_| Failure::new("project_name_invalid").at("name"))?;
    let version = ProjectVersion::parse(&args.initial_version)
        .map_err(|_| Failure::new("source_initial_version_invalid").at("initial_version"))?;
    let platform = args
        .platform_version
        .as_deref()
        .map(PlatformVersion::parse)
        .transpose()
        .map_err(|_| Failure::new("platform_version_invalid").at("platform_version"))?;
    let mut options =
        NewProjectOptions::new(args.project_type.into(), version).with_platform(platform);
    if let Some(metadata_name) = &args.metadata_name {
        options = options.with_metadata_name(metadata_name.clone())?;
    }
    let metadata_name = options.metadata_name(&name);
    let imported = new_project(&destination, name, &options)?;
    let root = path_text(imported.project.root())?;
    let ancestor = imported
        .initial_ancestor
        .as_ref()
        .ok_or_else(|| Failure::new("source_ancestor_missing"))?;
    let mut human = localizer.format(
        "tool-project-imported",
        &[
            ("path", LocalizationValue::Text(root)),
            ("pin", LocalizationValue::Text(&imported.accepted.sha256)),
        ],
    );
    human.push('\n');
    human.push_str(&localizer.format(
        "tool-project-import-ancestor",
        &[("pin", LocalizationValue::Text(ancestor.sha256()))],
    ));
    Ok((
        json!({"root":root,"name":args.name,"type":imported.project.configuration().project_type().as_str(),
        "metadata_name":metadata_name,"initial_version":options.version().to_string(),
        "version_scheme":options.version().scheme().as_str(),
        "manifest_sha256":imported.manifest_sha256,
        "source_format":"eska","source_layout":"queries","source":path_text(imported.project.source())?,
        "accepted_base":".eska/accepted","accepted_base_sha256":imported.accepted.sha256,
        "ancestor_sha256":ancestor.sha256()}),
        human,
    ))
}
