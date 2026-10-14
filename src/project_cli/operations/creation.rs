//! New project publication delegates every layout to the shared offline creation pipeline.

use super::super::args::{Import, ImportLayout};
use crate::{
    project_core::{
        ProjectName, creation, platform_settings::PlatformVersion, source_format::NativeUuid,
    },
    tool_cli::{
        Failure,
        localization::{LocalizationValue, Localizer},
        path_text,
    },
};
use serde_json::{Value, json};
use std::path::Path;

/// Import into an exclusively new directory, returning a pin for independent approval/storage.
pub(super) fn import(args: &Import, localizer: &Localizer) -> Result<(Value, String), Failure> {
    path_text(&args.input)?;
    path_text(&args.destination)?;
    let parent = args
        .destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = parent
        .canonicalize()
        .map_err(|_| Failure::new("source_file_missing"))?;
    path_text(&parent)?;
    let name = ProjectName::parse(args.name.clone())
        .map_err(|_| Failure::new("project_name_invalid").at("name"))?;
    let platform = args
        .platform_version
        .as_deref()
        .map(PlatformVersion::parse)
        .transpose()
        .map_err(|_| Failure::new("platform_version_invalid").at("platform_version"))?;
    let frozen = args
        .freeze
        .iter()
        .map(|uuid| NativeUuid::parse(uuid))
        .collect::<Result<Vec<_>, _>>()?;
    let import = match args.layout {
        ImportLayout::Family => creation::import_project,
        ImportLayout::Readable => creation::import_readable_project,
        ImportLayout::Queries => creation::import_query_project,
    };
    let imported = import(
        &args.input,
        &args.destination,
        name,
        platform.as_ref(),
        &frozen,
    )?;
    let root = path_text(imported.project.root())?;
    let mut human = localizer.format(
        "tool-project-imported",
        &[
            ("path", LocalizationValue::Text(root)),
            ("pin", LocalizationValue::Text(&imported.accepted.sha256)),
        ],
    );
    let mut data = json!({"root":root,"name":args.name,"source_format":"eska",
        "source_layout":args.layout.as_str(),
        "source":path_text(imported.project.source())?,"accepted_base":".eska/accepted",
        "accepted_base_sha256":imported.accepted.sha256});
    if let Some(ancestor) = &imported.initial_ancestor {
        data["manifest_sha256"] = json!(imported.manifest_sha256);
        data["ancestor_sha256"] = json!(ancestor.sha256());
        human.push('\n');
        human.push_str(&localizer.format(
            "tool-project-import-ancestor",
            &[("pin", LocalizationValue::Text(ancestor.sha256()))],
        ));
    }
    Ok((data, human))
}
