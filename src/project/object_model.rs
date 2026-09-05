//! Logical objects discovered from a Designer XML source tree.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use gix::bstr::ByteSlice;

use super::{Project, metadata};

const MD_NAMESPACE: &str = "http://v8.1c.ru/8.3/MDClasses";
const MAX_DESCRIPTOR_BYTES: u64 = 64 * 1024 * 1024;

/// Stable readable identity built from the logical metadata hierarchy.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ObjectId(String);

impl ObjectId {
    /// Return the stable machine-facing hierarchical identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ObjectId {
    /// Write the stable machine-facing identifier.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// One Designer metadata object and all source paths owned by it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogicalObject {
    id: ObjectId,
    metadata_type: &'static str,
    name: String,
    uuid: String,
    parent: Option<ObjectId>,
    descriptor_path: PathBuf,
    paths: BTreeSet<PathBuf>,
    module_paths: BTreeSet<PathBuf>,
    form_paths: BTreeSet<PathBuf>,
}

impl LogicalObject {
    /// Return the stable Designer object identifier.
    #[must_use]
    pub const fn id(&self) -> &ObjectId {
        &self.id
    }

    /// Return the stable machine-facing metadata type.
    #[must_use]
    pub const fn metadata_type(&self) -> &'static str {
        self.metadata_type
    }

    /// Return the metadata name stored in the descriptor.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the Designer UUID as auxiliary, non-unique metadata.
    #[must_use]
    pub fn uuid(&self) -> &str {
        &self.uuid
    }

    /// Return the containing metadata object, if this is a nested object.
    #[must_use]
    pub const fn parent(&self) -> Option<&ObjectId> {
        self.parent.as_ref()
    }

    /// Return the descriptor path relative to the project source directory.
    #[must_use]
    pub fn descriptor_path(&self) -> &Path {
        &self.descriptor_path
    }

    /// Return every source path that stores or implements this logical object.
    #[must_use]
    pub fn paths(&self) -> impl ExactSizeIterator<Item = &Path> {
        self.paths.iter().map(PathBuf::as_path)
    }

    /// Return implementation module paths directly owned by this object.
    #[must_use]
    pub fn module_paths(&self) -> impl ExactSizeIterator<Item = &Path> {
        self.module_paths.iter().map(PathBuf::as_path)
    }

    /// Return descriptor paths of forms directly contained by this object.
    #[must_use]
    pub fn form_paths(&self) -> impl ExactSizeIterator<Item = &Path> {
        self.form_paths.iter().map(PathBuf::as_path)
    }
}

/// Read-only bidirectional index of Designer XML objects and source paths.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectModel {
    project_type: super::ProjectType,
    objects: BTreeMap<ObjectId, LogicalObject>,
    by_logical_path: BTreeMap<metadata::MetadataPath, ObjectId>,
    by_source_path: BTreeMap<PathBuf, BTreeSet<ObjectId>>,
}

impl ObjectModel {
    /// Return discovered objects in deterministic `ObjectId` order.
    #[must_use]
    pub fn objects(&self) -> impl ExactSizeIterator<Item = &LogicalObject> {
        self.objects.values()
    }

    /// Find one object by its stable identifier.
    #[must_use]
    pub fn object(&self, id: &ObjectId) -> Option<&LogicalObject> {
        self.objects.get(id)
    }

    /// Map a source-relative changed path to its nearest logical object owners.
    #[must_use]
    pub fn objects_for_changed_path(&self, path: &Path) -> Vec<&LogicalObject> {
        if let Some(ids) = self.by_source_path.get(path) {
            return ids.iter().filter_map(|id| self.objects.get(id)).collect();
        }
        let Some(mut logical) = logical_path_for_source(self.project_type, path) else {
            return Vec::new();
        };
        loop {
            if let Some(id) = self.by_logical_path.get(&logical) {
                return self.objects.get(id).into_iter().collect();
            }
            if logical.parts.pop().is_none() {
                return Vec::new();
            }
        }
    }

    /// Return paths that store or implement one stable object identifier.
    #[must_use]
    pub fn paths_for_object(&self, id: &ObjectId) -> Option<Vec<&Path>> {
        self.objects.get(id).map(|object| object.paths().collect())
    }
}

/// Structured failures produced while indexing a Designer XML source tree.
#[derive(Debug)]
pub enum ObjectModelError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    PathOutsideSource {
        path: PathBuf,
    },
    DescriptorTooLarge {
        path: PathBuf,
    },
    InvalidXml {
        path: PathBuf,
        source: roxmltree::Error,
    },
    InvalidDescriptor {
        path: PathBuf,
        reason: &'static str,
    },
    DuplicateObjectId {
        id: ObjectId,
        path: PathBuf,
    },
    DuplicateLogicalPath {
        path: PathBuf,
    },
}

impl fmt::Display for ObjectModelError {
    /// Render a locale-independent diagnostic for library callers.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::PathOutsideSource { path } => {
                write!(formatter, "path escapes source: {}", path.display())
            }
            Self::DescriptorTooLarge { path } => {
                write!(formatter, "descriptor is too large: {}", path.display())
            }
            Self::InvalidXml { path, source } => {
                write!(formatter, "invalid XML {}: {source}", path.display())
            }
            Self::InvalidDescriptor { path, reason } => {
                write!(formatter, "invalid descriptor {}: {reason}", path.display())
            }
            Self::DuplicateObjectId { id, path } => {
                write!(formatter, "duplicate object {id} in {}", path.display())
            }
            Self::DuplicateLogicalPath { path } => {
                write!(
                    formatter,
                    "duplicate logical object path: {}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for ObjectModelError {
    /// Preserve parser and filesystem causes for diagnostic chains.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::InvalidXml { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Discover logical objects without changing the source tree or creating a cache.
///
/// # Errors
/// Returns a structured error for unreadable paths, unsafe symlinks, malformed
/// descriptors, missing identity fields or duplicate stable identities.
pub fn discover(project: &Project) -> Result<ObjectModel, ObjectModelError> {
    let files = collect_source_files(project.source())?;
    let project_type = project.configuration().project_type();
    let mut objects: BTreeMap<ObjectId, LogicalObject> = BTreeMap::new();
    let mut by_logical_path = BTreeMap::new();
    let mut by_source_path: BTreeMap<PathBuf, BTreeSet<ObjectId>> = BTreeMap::new();

    for file in &files {
        let Some(relative) = file.relative.to_str() else {
            continue;
        };
        let relative_bytes = relative.as_bytes().as_bstr();
        if !metadata::is_object_descriptor(project_type, relative_bytes) {
            continue;
        }
        let logical =
            logical_path_for_source(project_type, Path::new(relative)).ok_or_else(|| {
                ObjectModelError::InvalidDescriptor {
                    path: file.relative.clone(),
                    reason: "descriptor path has no logical owner",
                }
            })?;
        let contents = read_descriptor(file)?;
        let parent = nearest_logical_owner(&logical, &by_logical_path).cloned();
        let drafts = parse_descriptor(&file.relative, &contents, &logical, parent)?;
        for draft in drafts {
            by_source_path
                .entry(file.relative.clone())
                .or_default()
                .insert(draft.object.id.clone());
            if let Some(existing) = objects.get_mut(&draft.object.id) {
                if by_logical_path.get(&draft.logical_path) != Some(&draft.object.id)
                    || existing.uuid != draft.object.uuid
                {
                    return Err(ObjectModelError::DuplicateObjectId {
                        id: draft.object.id,
                        path: file.relative.clone(),
                    });
                }
                existing.paths.extend(draft.object.paths);
                if draft.standalone {
                    existing.descriptor_path = draft.object.descriptor_path;
                    existing.metadata_type = draft.object.metadata_type;
                    existing.name = draft.object.name;
                }
                continue;
            }
            if by_logical_path
                .insert(draft.logical_path.clone(), draft.object.id.clone())
                .is_some()
            {
                return Err(ObjectModelError::DuplicateLogicalPath {
                    path: file.relative.clone(),
                });
            }
            objects.insert(draft.object.id.clone(), draft.object);
        }
    }

    assign_source_paths(
        project_type,
        &files,
        &by_logical_path,
        &mut by_source_path,
        &mut objects,
    );
    assign_form_paths(&mut objects);

    Ok(ObjectModel {
        project_type,
        objects,
        by_logical_path,
        by_source_path,
    })
}

#[derive(Debug)]
struct SourceFile {
    relative: PathBuf,
    physical: PathBuf,
}

/// Collect files deterministically while preventing symlink escapes and cycles.
fn collect_source_files(source: &Path) -> Result<Vec<SourceFile>, ObjectModelError> {
    let source = fs::canonicalize(source).map_err(|source_error| ObjectModelError::Io {
        path: source.to_path_buf(),
        source: source_error,
    })?;
    let mut files = Vec::new();
    let mut visited = BTreeSet::new();
    collect_directory(&source, Path::new(""), &source, &mut visited, &mut files)?;
    files.sort_by(|left, right| {
        left.relative
            .components()
            .count()
            .cmp(&right.relative.components().count())
            .then_with(|| left.relative.cmp(&right.relative))
    });
    Ok(files)
}

/// Walk one physical directory while retaining its logical source-relative prefix.
fn collect_directory(
    directory: &Path,
    logical_prefix: &Path,
    source: &Path,
    visited: &mut BTreeSet<PathBuf>,
    files: &mut Vec<SourceFile>,
) -> Result<(), ObjectModelError> {
    let canonical = fs::canonicalize(directory).map_err(|source_error| ObjectModelError::Io {
        path: directory.to_path_buf(),
        source: source_error,
    })?;
    if !canonical.starts_with(source) {
        return Err(ObjectModelError::PathOutsideSource {
            path: directory.to_path_buf(),
        });
    }
    if !visited.insert(canonical.clone()) {
        return Ok(());
    }
    let entries = fs::read_dir(&canonical).map_err(|source_error| ObjectModelError::Io {
        path: canonical.clone(),
        source: source_error,
    })?;
    let mut entries = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source_error| ObjectModelError::Io {
            path: canonical.clone(),
            source: source_error,
        })?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let logical = logical_prefix.join(entry.file_name());
        let physical = entry.path();
        let resolved =
            fs::canonicalize(&physical).map_err(|source_error| ObjectModelError::Io {
                path: physical.clone(),
                source: source_error,
            })?;
        if !resolved.starts_with(source) {
            return Err(ObjectModelError::PathOutsideSource { path: physical });
        }
        if resolved.is_dir() {
            collect_directory(&resolved, &logical, source, visited, files)?;
        } else if resolved.is_file() {
            files.push(SourceFile {
                relative: logical,
                physical: resolved,
            });
        }
    }
    Ok(())
}

/// Read one bounded UTF-8 metadata descriptor.
fn read_descriptor(file: &SourceFile) -> Result<String, ObjectModelError> {
    let mut contents = String::new();
    fs::File::open(&file.physical)
        .and_then(|input| {
            input
                .take(MAX_DESCRIPTOR_BYTES + 1)
                .read_to_string(&mut contents)
        })
        .map_err(|source| ObjectModelError::Io {
            path: file.relative.clone(),
            source,
        })?;
    if contents.len() as u64 > MAX_DESCRIPTOR_BYTES {
        return Err(ObjectModelError::DescriptorTooLarge {
            path: file.relative.clone(),
        });
    }
    Ok(contents)
}

#[derive(Debug)]
struct DraftObject {
    logical_path: metadata::MetadataPath,
    object: LogicalObject,
    standalone: bool,
}

/// Parse one descriptor and every inline child object carrying its own UUID.
fn parse_descriptor(
    path: &Path,
    contents: &str,
    logical_path: &metadata::MetadataPath,
    parent: Option<ObjectId>,
) -> Result<Vec<DraftObject>, ObjectModelError> {
    let document = roxmltree::Document::parse_with_options(
        contents.trim_start_matches('\u{feff}'),
        roxmltree::ParsingOptions {
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )
    .map_err(|source| ObjectModelError::InvalidXml {
        path: path.to_path_buf(),
        source,
    })?;
    let root = document.root_element();
    if !root.has_tag_name((MD_NAMESPACE, "MetaDataObject")) {
        return Err(ObjectModelError::InvalidDescriptor {
            path: path.to_path_buf(),
            reason: "root is not MDClasses MetaDataObject",
        });
    }
    let mut roots = root.children().filter(roxmltree::Node::is_element);
    let object = roots
        .next()
        .ok_or_else(|| ObjectModelError::InvalidDescriptor {
            path: path.to_path_buf(),
            reason: "metadata object is missing",
        })?;
    if roots.next().is_some() || object.tag_name().namespace() != Some(MD_NAMESPACE) {
        return Err(ObjectModelError::InvalidDescriptor {
            path: path.to_path_buf(),
            reason: "descriptor must contain exactly one MDClasses object",
        });
    }
    let expected = logical_path
        .parts
        .last()
        .map(|part| part.kind)
        .ok_or_else(|| ObjectModelError::InvalidDescriptor {
            path: path.to_path_buf(),
            reason: "logical object path is empty",
        })?;
    if metadata::kind_from_tag(object.tag_name().name()) != Some(expected) {
        return Err(ObjectModelError::InvalidDescriptor {
            path: path.to_path_buf(),
            reason: "metadata type does not match descriptor path",
        });
    }
    let mut drafts = Vec::new();
    collect_descriptor_objects(path, object, logical_path, parent, true, &mut drafts)?;
    Ok(drafts)
}

/// Add one XML object and recursively add supported inline child objects.
fn collect_descriptor_objects(
    descriptor: &Path,
    node: roxmltree::Node<'_, '_>,
    logical_path: &metadata::MetadataPath,
    parent: Option<ObjectId>,
    standalone: bool,
    drafts: &mut Vec<DraftObject>,
) -> Result<(), ObjectModelError> {
    let metadata_type = metadata::kind_from_tag(node.tag_name().name()).ok_or_else(|| {
        ObjectModelError::InvalidDescriptor {
            path: descriptor.to_path_buf(),
            reason: "unsupported metadata object type",
        }
    })?;
    let uuid = node
        .attribute("uuid")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ObjectModelError::InvalidDescriptor {
            path: descriptor.to_path_buf(),
            reason: "metadata object UUID is missing",
        })?;
    let properties = node
        .children()
        .find(|child| child.has_tag_name((MD_NAMESPACE, "Properties")))
        .ok_or_else(|| ObjectModelError::InvalidDescriptor {
            path: descriptor.to_path_buf(),
            reason: "metadata properties are missing",
        })?;
    let name = properties
        .children()
        .find(|child| child.has_tag_name((MD_NAMESPACE, "Name")))
        .and_then(|child| child.text())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ObjectModelError::InvalidDescriptor {
            path: descriptor.to_path_buf(),
            reason: "metadata name is missing",
        })?
        .to_owned();
    let id = object_id(parent.as_ref(), metadata_type, &name);
    let mut paths = BTreeSet::new();
    paths.insert(descriptor.to_path_buf());
    drafts.push(DraftObject {
        logical_path: logical_path.clone(),
        object: LogicalObject {
            id: id.clone(),
            metadata_type,
            name,
            uuid: uuid.to_owned(),
            parent,
            descriptor_path: descriptor.to_path_buf(),
            paths,
            module_paths: BTreeSet::new(),
            form_paths: BTreeSet::new(),
        },
        standalone,
    });

    let Some(children) = node
        .children()
        .find(|child| child.has_tag_name((MD_NAMESPACE, "ChildObjects")))
    else {
        return Ok(());
    };
    for child in children.children().filter(roxmltree::Node::is_element) {
        let Some(kind) = metadata::kind_from_tag(child.tag_name().name()) else {
            continue;
        };
        let Some(child_name) = child
            .children()
            .find(|item| item.has_tag_name((MD_NAMESPACE, "Properties")))
            .and_then(|item| {
                item.children()
                    .find(|property| property.has_tag_name((MD_NAMESPACE, "Name")))
            })
            .and_then(|item| item.text())
        else {
            continue;
        };
        let suffix = metadata::MetadataPart {
            kind,
            name: Some(child_name.to_owned()),
        };
        collect_descriptor_objects(
            descriptor,
            child,
            &logical_path.with_suffix(&[suffix]),
            Some(id.clone()),
            false,
            drafts,
        )?;
    }
    Ok(())
}

/// Build a deterministic readable identifier from an object's logical ancestry.
fn object_id(parent: Option<&ObjectId>, metadata_type: &str, name: &str) -> ObjectId {
    let segment = format!("{metadata_type}:{}", escape_id_name(name));
    ObjectId(parent.map_or_else(|| segment.clone(), |parent| format!("{parent}/{segment}")))
}

/// Escape structural `ObjectId` separators while keeping Unicode names readable.
fn escape_id_name(name: &str) -> String {
    name.replace('%', "%25")
        .replace('/', "%2F")
        .replace(':', "%3A")
}

/// Find the nearest already discovered logical container of a descriptor.
fn nearest_logical_owner<'a>(
    logical: &metadata::MetadataPath,
    by_logical_path: &'a BTreeMap<metadata::MetadataPath, ObjectId>,
) -> Option<&'a ObjectId> {
    let mut parent_path = logical.clone();
    parent_path.parts.pop();
    while !parent_path.parts.is_empty() {
        if let Some(parent) = by_logical_path.get(&parent_path) {
            return Some(parent);
        }
        parent_path.parts.pop();
    }
    None
}

/// Associate every recognized source artifact with its nearest logical owner.
fn assign_source_paths(
    project_type: super::ProjectType,
    files: &[SourceFile],
    by_logical_path: &BTreeMap<metadata::MetadataPath, ObjectId>,
    by_source_path: &mut BTreeMap<PathBuf, BTreeSet<ObjectId>>,
    objects: &mut BTreeMap<ObjectId, LogicalObject>,
) {
    for file in files {
        let Some(mut logical) = logical_path_for_source(project_type, &file.relative) else {
            continue;
        };
        let owner = loop {
            if let Some(id) = by_logical_path.get(&logical) {
                break Some(id.clone());
            }
            if logical.parts.pop().is_none() {
                break None;
            }
        };
        let Some(owner) = owner else {
            continue;
        };
        by_source_path
            .entry(file.relative.clone())
            .or_default()
            .insert(owner.clone());
        if let Some(object) = objects.get_mut(&owner) {
            object.paths.insert(file.relative.clone());
            if is_module_path(&file.relative) {
                object.module_paths.insert(file.relative.clone());
            }
        }
    }
}

/// Resolve a path while normalizing the single root object of external projects.
fn logical_path_for_source(
    project_type: super::ProjectType,
    path: &Path,
) -> Option<metadata::MetadataPath> {
    let path = path.to_str()?;
    let mut logical = match project_type {
        super::ProjectType::Configuration | super::ProjectType::Extension => {
            metadata::from_path(project_type, path.as_bytes().as_bstr())?
        }
        super::ProjectType::Processing | super::ProjectType::Report => {
            let first = path.split('/').next()?;
            let normalized = if matches!(first, "Ext" | "Forms" | "Templates" | "Commands") {
                format!("Root/{path}")
            } else {
                path.to_owned()
            };
            metadata::from_path(project_type, normalized.as_bytes().as_bstr())?
        }
    };
    if matches!(
        project_type,
        super::ProjectType::Processing | super::ProjectType::Report
    ) && let Some(root) = logical.parts.first_mut()
    {
        root.name = None;
    }
    Some(logical)
}

/// Add each separately exported form descriptor to its direct parent's form paths.
fn assign_form_paths(objects: &mut BTreeMap<ObjectId, LogicalObject>) {
    let forms: Vec<_> = objects
        .values()
        .filter(|object| object.metadata_type == "form")
        .filter_map(|object| {
            object
                .parent
                .clone()
                .map(|parent| (parent, object.descriptor_path.clone()))
        })
        .collect();
    for (parent, path) in forms {
        if let Some(object) = objects.get_mut(&parent) {
            object.form_paths.insert(path);
        }
    }
}

/// Recognize standard Designer implementation module files.
fn is_module_path(path: &Path) -> bool {
    let Some(file) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    matches!(
        file,
        "Module.bsl"
            | "Module.bin"
            | "ObjectModule.bsl"
            | "ObjectModule.bin"
            | "ManagerModule.bsl"
            | "ManagerModule.bin"
            | "RecordSetModule.bsl"
            | "RecordSetModule.bin"
            | "ValueManagerModule.bsl"
            | "ValueManagerModule.bin"
            | "ManagedApplicationModule.bsl"
            | "OrdinaryApplicationModule.bsl"
            | "SessionModule.bsl"
            | "ExternalConnectionModule.bsl"
            | "CommandModule.bsl"
    )
}
