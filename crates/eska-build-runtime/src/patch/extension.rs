//! Materialization of generated Designer XML extension sources.

use super::{MD, PatchDescription, PatchError};
use std::{fmt::Write as _, fs, path::Path};

/// Add generated module descriptors and bodies to a platform-created extension skeleton.
pub(super) fn write(
    plan: &PatchDescription,
    sources: &Path,
    directory: &Path,
) -> Result<(), PatchError> {
    let path = directory.join("Configuration.xml");
    let mut xml = fs::read_to_string(&path).map_err(|e| PatchError::new("io", e.to_string()))?;
    let document = roxmltree::Document::parse(&xml)
        .map_err(|e| PatchError::new("descriptor", e.to_string()))?;
    let child = document
        .descendants()
        .find(|node| node.has_tag_name((MD, "ChildObjects")))
        .ok_or_else(|| PatchError::new("descriptor", "ChildObjects"))?;
    let end = child.range().end;
    let closing = xml[..end]
        .rfind("</")
        .ok_or_else(|| PatchError::new("descriptor", "ChildObjects"))?;
    let mut children = String::new();
    for module in &plan.modules {
        write!(
            children,
            "<CommonModule xmlns=\"{MD}\">{}</CommonModule>",
            module.name
        )
        .expect("writing to String cannot fail");
        copy_source(
            &sources.join(format!("{}.xml", module.name)),
            &directory.join(format!("CommonModules/{}.xml", module.name)),
        )?;
        copy_source(
            &sources.join(format!("{}/Ext/Module.bsl", module.name)),
            &directory.join(format!("CommonModules/{}/Ext/Module.bsl", module.name)),
        )?;
    }
    xml.insert_str(closing, &children);
    fs::write(
        &path,
        eska_project_core::patch::normalize_designer_text(&xml),
    )
    .map_err(|error| PatchError::new("io", error.to_string()))
}

/// Copy already normalized prepared sources without rewriting their BOM or CRLF bytes.
fn copy_source(source: &Path, destination: &Path) -> Result<(), PatchError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| PatchError::new("io", error.to_string()))?;
    }
    fs::copy(source, destination)
        .map(|_| ())
        .map_err(|error| PatchError::new("io", error.to_string()))
}
