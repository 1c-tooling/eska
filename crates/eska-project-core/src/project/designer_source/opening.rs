use std::{
    fs,
    io::{self, Read},
    path::{Component, Path, PathBuf},
};

use super::{DesignerSource, SourceError, SourceIoStats};
use crate::project::metadata_disk_cache::DiskCache;
use crate::project::{
    Project, ProjectType, designer_xml,
    discovery::discover_context,
    metadata_model::{MetadataKind, MetadataObject, MetadataProject, ProjectScope},
    selection::{SelectionIntent, select_projects},
};
use sha2::{Digest, Sha256};

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
    open_projects_cached(start, names, entire_workspace, false)
}

/// Select projects with optional disposable caches; ordinary callers remain read-only.
pub fn open_projects_cached(
    start: &Path,
    names: &[String],
    entire_workspace: bool,
    cached: bool,
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
            open_project(project, scope, cached)
        })
        .collect()
}

/// Reuse the checked root reader after a consumer has already resolved its manifest context.
pub fn open_project(
    project: Project,
    scope: ProjectScope,
    cached: bool,
) -> Result<DesignerSource, SourceError> {
    open_project_root(project, scope, cached, false).map(|(source, _)| source)
}

/// Read analysis identity and compatibility from the same checked root snapshot without parsing its children again.
pub fn open_analysis_project(
    project: Project,
    scope: ProjectScope,
) -> Result<(DesignerSource, Option<String>), SourceError> {
    open_project_root(project, scope, false, true)
}

/// Ordinary metadata callers keep their cache contract; analysis always validates its root without a cache.
fn open_project_root(
    project: Project,
    scope: ProjectScope,
    cached: bool,
    analysis: bool,
) -> Result<(DesignerSource, Option<String>), SourceError> {
    let disk_cache = cached.then(|| DiskCache::new(&project));
    let mut stats = SourceIoStats::default();
    let (descriptor, root, compatibility) =
        read_root(&project, disk_cache.as_ref(), &mut stats, analysis)?;
    Ok((
        DesignerSource {
            project,
            scope,
            root,
            descriptor,
            disk_cache,
            io_stats: std::cell::Cell::new(stats),
        },
        compatibility,
    ))
}

impl DesignerSource {
    /// Reread root identity without reusing cached declarations for a structural mutation.
    pub(crate) fn reopen(&self) -> Result<Self, SourceError> {
        let mut sources = open_projects(self.project.root(), &[], false)?.into_iter();
        let source = sources.next().ok_or_else(|| SourceError::InvalidMetadata {
            path: PathBuf::from(crate::config::FILE_NAME),
            reason: "project selection changed",
        })?;
        if sources.next().is_some() || source.project != self.project || source.scope != self.scope
        {
            return Err(SourceError::InvalidMetadata {
                path: PathBuf::from(crate::config::FILE_NAME),
                reason: "project settings changed",
            });
        }
        Ok(source)
    }
}

/// Find a root descriptor without opening any child directory or assuming an external filename.
fn read_root(
    project: &Project,
    cache: Option<&DiskCache>,
    stats: &mut SourceIoStats,
    analysis: bool,
) -> Result<(PathBuf, MetadataObject, Option<String>), SourceError> {
    let candidates = root_candidates(project, stats)?;
    let mut roots = Vec::new();
    for path in candidates {
        let Some(physical) = existing_file(project.source(), &path, stats)? else {
            continue;
        };
        stats.xml_reads += 1;
        let started = std::time::Instant::now();
        let input = read_descriptor(&physical)?;
        stats.xml_bytes += input.len() as u64;
        stats.xml_read_nanos += started.elapsed().as_nanos();
        // Analysis has no disk cache. Hashing its root again cannot validate or
        // reuse anything; the analyzer configuration fingerprints its own inputs.
        let cache_input = cache.map(|_| {
            let started = std::time::Instant::now();
            let hash: [u8; 32] = Sha256::digest(input.as_bytes()).into();
            stats.root_hash_nanos += started.elapsed().as_nanos();
            let key = format!(
                "root:{:?}:{}",
                path.as_os_str().as_encoded_bytes(),
                project.configuration().project_type().as_str()
            );
            (key, hash)
        });
        if let Some(root) = cache
            .zip(cache_input.as_ref())
            .and_then(|(cache, (key, hash))| cache.get::<MetadataObject>(key, *hash))
        {
            roots.push((path, root, None));
            continue;
        }
        stats.root_parses += 1;
        crate::project::metadata_parser::check_envelope(&input).map_err(|source| {
            SourceError::Parse {
                path: path.clone(),
                source,
            }
        })?;
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
        let root = root_metadata(&path, document.root_element())?;
        let compatibility = if analysis {
            analysis_compatibility(project, &path, document.root_element(), &root)?
        } else {
            None
        };
        if let Some((cache, (key, hash))) = cache.zip(cache_input.as_ref()) {
            cache.put(key, *hash, &root);
        }
        roots.push((path, root, compatibility));
    }
    if roots.len() > 1 {
        return Err(SourceError::AmbiguousRoot {
            paths: roots.into_iter().map(|(path, _, _)| path).collect(),
        });
    }
    roots.pop().ok_or(SourceError::MissingRoot)
}

/// Locate root candidates without opening child directories or guessing external-object filenames.
fn root_candidates(
    project: &Project,
    stats: &mut SourceIoStats,
) -> Result<Vec<PathBuf>, SourceError> {
    let candidates = if matches!(
        project.configuration().project_type(),
        ProjectType::Configuration | ProjectType::Extension
    ) {
        vec![PathBuf::from("Configuration.xml")]
    } else {
        let mut paths = Vec::new();
        stats.directory_reads += 1;
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
    Ok(candidates)
}

/// Analysis needs one scalar property, not an owned model of thousands of child declarations.
fn analysis_compatibility(
    project: &Project,
    path: &Path,
    root: roxmltree::Node<'_, '_>,
    metadata: &MetadataObject,
) -> Result<Option<String>, SourceError> {
    let invalid = || SourceError::InvalidMetadata {
        path: path.to_owned(),
        reason: "invalid_analysis_root",
    };
    let object = root
        .children()
        .find(roxmltree::Node::is_element)
        .ok_or_else(invalid)?;
    let mut property_nodes = object
        .children()
        .filter(|node| node.has_tag_name((MD_NAMESPACE, "Properties")));
    let properties = property_nodes
        .next()
        .filter(|_| property_nodes.next().is_none())
        .ok_or_else(invalid)?;
    let mut names = properties
        .children()
        .filter(|node| node.has_tag_name((MD_NAMESPACE, "Name")));
    let name = names
        .next()
        .and_then(crate::project::metadata_parser::simple_text)
        .filter(|_| names.next().is_none())
        .ok_or_else(invalid)?;
    if metadata.name() != name.trim() {
        return Err(invalid());
    }
    let field = if project.configuration().project_type() == ProjectType::Extension {
        "ConfigurationExtensionCompatibilityMode"
    } else {
        "CompatibilityMode"
    };
    let mut values = properties
        .children()
        .filter(|node| node.has_tag_name((MD_NAMESPACE, field)));
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(invalid());
    }
    crate::project::metadata_parser::simple_text(value)
        .map(Some)
        .ok_or_else(invalid)
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
pub(super) fn read_descriptor(path: &Path) -> Result<String, SourceError> {
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
pub fn existing_file(
    source: &Path,
    relative: &Path,
    stats: &mut SourceIoStats,
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
    stats.path_checks += 1;
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
    stats.metadata_checks += 1;
    if !physical.is_file() {
        return Err(SourceError::NotFile {
            path: relative.to_owned(),
        });
    }
    Ok(Some(physical))
}
