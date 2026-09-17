use std::{
    fs,
    io::{self, Read},
    path::{Component, Path, PathBuf},
};

use super::{DesignerSource, SourceError};
use crate::project::{
    Project, ProjectType, designer_xml,
    discovery::discover_context,
    metadata_model::{MetadataKind, MetadataObject, MetadataProject, ProjectScope},
    selection::{SelectionIntent, select_projects},
};

pub(super) const MD_NAMESPACE: &str = "http://v8.1c.ru/8.3/MDClasses";
const MAX_DESCRIPTOR_BYTES: u64 = 64 * 1024 * 1024;

/// Open selected projects through existing manifest discovery and workspace selection.
///
/// Only selected root descriptors are read. No Git operation, platform command,
/// source write or recursive source discovery is performed.
///
/// # Errors
/// Returns missing/invalid manifests, selectors, root descriptors or source containment errors.
pub fn open_projects(
    start: &Path,
    names: &[String],
    entire_workspace: bool,
) -> Result<Vec<DesignerSource>, SourceError> {
    let context = discover_context(start).map_err(SourceError::Discovery)?;
    let selection = select_projects(&context, names, entire_workspace, SelectionIntent::ReadOnly)
        .map_err(SourceError::Selection)?;
    selection
        .projects()
        .iter()
        .map(|selected| {
            let project = selected.project().clone();
            let scope = selected.name().map_or(ProjectScope::Standalone, |name| {
                ProjectScope::Member(name.clone())
            });
            let (descriptor, root) = read_root(&project)?;
            Ok(DesignerSource {
                project,
                scope,
                root,
                descriptor,
            })
        })
        .collect()
}

/// Find a root descriptor without opening any child directory or assuming an external filename.
fn read_root(project: &Project) -> Result<(PathBuf, MetadataObject), SourceError> {
    let candidates = if matches!(
        project.configuration().project_type(),
        ProjectType::Configuration | ProjectType::Extension
    ) {
        vec![PathBuf::from("Configuration.xml")]
    } else {
        let mut paths = Vec::new();
        for entry in fs::read_dir(project.source()).map_err(|source| SourceError::Io {
            path: project.source().to_owned(),
            source,
        })? {
            let entry = entry.map_err(|source| SourceError::Io {
                path: project.source().to_owned(),
                source,
            })?;
            let path = PathBuf::from(entry.file_name());
            if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("xml"))
                && path != Path::new("ConfigDumpInfo.xml")
            {
                paths.push(path);
            }
        }
        paths.sort();
        paths
    };
    let mut roots = Vec::new();
    for path in candidates {
        let Some(physical) = existing_file(project.source(), &path)? else {
            continue;
        };
        let input = read_descriptor(&physical)?;
        let document = roxmltree::Document::parse_with_options(
            &input,
            roxmltree::ParsingOptions {
                nodes_limit: 1_000_000,
                ..Default::default()
            },
        )
        .map_err(|source| SourceError::InvalidRoot {
            path: path.clone(),
            source: crate::project::metadata_model::MetadataProjectError::InvalidXml(source),
        })?;
        let detected = designer_xml::project_type_from_root(document.root_element());
        let Some(detected) = detected else {
            if path == Path::new("Configuration.xml") {
                return Err(SourceError::InvalidMetadata {
                    path,
                    reason: "unsupported root descriptor",
                });
            }
            continue;
        };
        MetadataProject::from_detected_type(project.configuration(), detected).map_err(
            |source| SourceError::InvalidRoot {
                path: path.clone(),
                source,
            },
        )?;
        roots.push((path.clone(), root_metadata(&path, document.root_element())?));
    }
    if roots.len() > 1 {
        return Err(SourceError::AmbiguousRoot {
            paths: roots.into_iter().map(|(path, _)| path).collect(),
        });
    }
    roots.pop().ok_or(SourceError::MissingRoot)
}

/// Extract the logical root's name and UUID after checking the descriptor's project type.
fn root_metadata(
    path: &Path,
    root: roxmltree::Node<'_, '_>,
) -> Result<MetadataObject, SourceError> {
    let invalid = || SourceError::InvalidMetadata {
        path: path.to_owned(),
        reason: "root metadata name, kind or UUID is missing",
    };
    let object = root
        .children()
        .find(roxmltree::Node::is_element)
        .ok_or_else(invalid)?;
    let kind = MetadataKind::from_xml_tag(object.tag_name().name()).map_err(|_| invalid())?;
    let name = object
        .children()
        .find(|node| node.has_tag_name((MD_NAMESPACE, "Properties")))
        .and_then(|properties| {
            properties
                .children()
                .find(|node| node.has_tag_name((MD_NAMESPACE, "Name")))
        })
        .and_then(|node| node.text())
        .filter(|name| !name.is_empty())
        .ok_or_else(invalid)?;
    let uuid = object
        .attribute("uuid")
        .filter(|uuid| !uuid.is_empty())
        .ok_or_else(invalid)?;
    MetadataObject::new(kind, name.to_owned(), uuid.to_owned(), None).map_err(|_| invalid())
}

/// Read a bounded XML descriptor; modules and payloads never pass through this function.
fn read_descriptor(path: &Path) -> Result<String, SourceError> {
    let mut input = String::new();
    fs::File::open(path)
        .and_then(|file| {
            file.take(MAX_DESCRIPTOR_BYTES + 1)
                .read_to_string(&mut input)
        })
        .map_err(|source| SourceError::Io {
            path: path.to_owned(),
            source,
        })?;
    if input.len() as u64 > MAX_DESCRIPTOR_BYTES {
        return Err(SourceError::DescriptorTooLarge {
            path: path.to_owned(),
        });
    }
    Ok(input)
}

/// Check a source-relative candidate without treating a missing file as an existing location.
pub(super) fn existing_file(
    source: &Path,
    relative: &Path,
) -> Result<Option<PathBuf>, SourceError> {
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(SourceError::OutsideSource {
            path: relative.to_owned(),
        });
    }
    let physical = match fs::canonicalize(source.join(relative)) {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(SourceError::Io {
                path: relative.to_owned(),
                source,
            });
        }
    };
    if !physical.starts_with(source) {
        return Err(SourceError::OutsideSource {
            path: relative.to_owned(),
        });
    }
    if !physical.is_file() {
        return Err(SourceError::NotFile {
            path: relative.to_owned(),
        });
    }
    Ok(Some(physical))
}
