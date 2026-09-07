//! Materialization of generated Designer XML extension sources.

use super::{MD, PatchError, PatchPlan};
use std::{fmt::Write as _, fs, path::Path};

/// Add generated module descriptors and bodies to a platform-created extension skeleton.
pub(super) fn write(plan: &PatchPlan, directory: &Path) -> Result<(), PatchError> {
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
        write_source(
            &directory.join(format!("CommonModules/{}.xml", module.name)),
            &module.descriptor,
        )?;
        write_source(
            &directory.join(format!("CommonModules/{}/Ext/Module.bsl", module.name)),
            &module.code,
        )?;
    }
    xml.insert_str(closing, &children);
    write_source(&path, &xml)
}

/// Keep generated 1C sources in their native BOM/CRLF representation.
fn write_source(path: &Path, text: &str) -> Result<(), PatchError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| PatchError::new("io", e.to_string()))?;
    }
    let text = format!(
        "\u{feff}{}",
        text.trim_start_matches('\u{feff}')
            .replace("\r\n", "\n")
            .replace('\n', "\r\n")
    );
    fs::write(path, text).map_err(|e| PatchError::new("io", e.to_string()))
}
