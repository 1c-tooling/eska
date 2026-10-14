//! Native-file origins describe accepted bindings with selected values and supported inline creation.

mod created;
mod form;
mod location;
mod query;

pub use location::{OriginTexts, SourceLocation};

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{Origin, native_file_owner};
use crate::project_core::{
    native,
    source_format::{
        AcceptedBase, Error, FileEntry, FileRole, NativeUuid, QuerySlot, SourcePath,
        export::Package,
        files::{self, Inventory},
    },
};

/// The version of native-file origin records, independent of source and conversion schemas.
pub const SOURCE_MAP_VERSION: u32 = 1;
/// Relocated payload inputs have physical source paths distinct from their native output paths.
pub const RELOCATED_SOURCE_MAP_VERSION: u32 = 2;
/// A generated dialog shares authoring bytes and metadata UUID with its declared form descriptor.
pub const MANAGED_FORM_SOURCE_MAP_VERSION: u32 = 3;
/// Composite query resources include the independently selected plain inputs and scoped native ranges.
pub const QUERY_SOURCE_MAP_VERSION: u32 = 4;
/// External roots require an explicit reverse binding in addition to form/query origins.
pub const EXTERNAL_SOURCE_MAP_VERSION: u32 = 5;
/// UUID-preserving renames retain old pinned origins and distinct current authoring paths.
pub const COMPOSITION_SOURCE_MAP_VERSION: u32 = 6;
/// Newly declared owners have working evidence without inventing a pinned accepted origin.
pub const CREATED_SOURCE_MAP_VERSION: u32 = 7;

/// Exact authoring bytes addressed relative to a working tree or its pinned accepted snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputFile {
    pub path: SourcePath,
    pub bytes: u64,
    pub sha256: String,
}

impl From<&FileEntry> for InputFile {
    /// Keep the captured checksum rather than rereading a possibly newer working file.
    fn from(entry: &FileEntry) -> Self {
        Self {
            path: entry.path.clone(),
            bytes: entry.bytes,
            sha256: entry.sha256.clone(),
        }
    }
}

/// Byte-preserved payloads use exactly one selected input authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputAuthority {
    Working,
    Accepted,
}

/// This exact selected plain input supplies one native query property; the record is not permission.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryOrigin {
    pub slot: QuerySlot,
    pub native: std::ops::Range<usize>,
    pub authority: InputAuthority,
    pub input: InputFile,
}

/// A generated descriptor can combine accepted bindings, selected values and supported new inline nodes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FileOrigin {
    Payload {
        authority: InputAuthority,
        input: InputFile,
        module: bool,
        owner: Option<NativeUuid>,
    },
    Metadata {
        uuid: NativeUuid,
        accepted: InputFile,
        #[serde(skip_serializing_if = "Option::is_none")]
        working: Option<InputFile>,
        uses_working_values: bool,
    },
    CreatedMetadata {
        uuid: NativeUuid,
        input: InputFile,
    },
    ManagedForm {
        uuid: NativeUuid,
        descriptor: SourcePath,
        accepted: InputFile,
        #[serde(skip_serializing_if = "Option::is_none")]
        working: Option<InputFile>,
        uses_working_values: bool,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        queries: Vec<QueryOrigin>,
    },
    CreatedManagedForm {
        uuid: NativeUuid,
        descriptor: SourcePath,
        input: InputFile,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        queries: Vec<QueryOrigin>,
    },
    QueryResource {
        uuid: NativeUuid,
        descriptor: SourcePath,
        authority: InputAuthority,
        input: InputFile,
        queries: Vec<QueryOrigin>,
    },
}

/// One native file, including descriptors, modules, policy, supplier CF and orphan resources.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MappedFile {
    pub path: SourcePath,
    pub bytes: u64,
    pub sha256: String,
    pub origin: FileOrigin,
}

/// Portable file origins for a particular protected input; this is data, not an export grant.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMap {
    pub schema_version: u32,
    pub source_schema_version: u32,
    pub rules_version: u32,
    pub profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_root: Option<crate::project_core::source_format::ExternalRoot>,
    pub generation: String,
    pub input_revision: String,
    pub baseline: String,
    pub native_revision: String,
    pub directories: Vec<SourcePath>,
    pub files: Vec<MappedFile>,
}

impl SourceMap {
    /// Locate a native file without rescanning metadata or loading a source tree.
    #[must_use]
    pub fn file(&self, path: &SourcePath) -> Option<&MappedFile> {
        self.files
            .binary_search_by(|entry| entry.path.cmp(path))
            .ok()
            .map(|index| &self.files[index])
    }

    /// Validate the complete file map before accepting externally supplied origin data.
    ///
    /// # Errors
    /// Returns unsupported versions, malformed fingerprints, inconsistent origins or inventory.
    pub fn validate(&self) -> Result<(), Error> {
        self.validate_schema()?;
        if let Some(normalized) = created::normalized(self)? {
            return normalized.validate();
        }
        for digest in [
            &self.generation,
            &self.input_revision,
            &self.baseline,
            &self.native_revision,
        ] {
            require_digest(digest)?;
        }
        if self
            .files
            .windows(2)
            .any(|pair| pair[0].path >= pair[1].path)
            || self.directories.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(Error::root("source_map_inventory_invalid"));
        }
        let mut names = BTreeSet::new();
        for path in self
            .directories
            .iter()
            .chain(self.files.iter().map(|entry| &entry.path))
        {
            if !names.insert(path.as_str().to_lowercase()) {
                return Err(Error::root("source_map_inventory_invalid"));
            }
            if let Some((parent, _)) = path.as_str().rsplit_once('/')
                && self
                    .directories
                    .binary_search_by(|directory| directory.as_str().cmp(parent))
                    .is_err()
            {
                return Err(Error::root("source_map_inventory_invalid"));
            }
        }
        let mut entries = BTreeMap::new();
        let mut identities = BTreeSet::new();
        for entry in &self.files {
            require_digest(&entry.sha256)?;
            validate_origin(entry)?;
            if self.schema_version < COMPOSITION_SOURCE_MAP_VERSION
                && matches!(&entry.origin, FileOrigin::Metadata { accepted, working: Some(working), .. }
                    | FileOrigin::ManagedForm { accepted, working: Some(working), .. } if accepted.path != working.path)
            {
                return Err(Error::root("source_map_origin_invalid"));
            }
            if self.schema_version == SOURCE_MAP_VERSION
                && matches!(&entry.origin, FileOrigin::Payload { input, .. } if input.path != entry.path)
            {
                return Err(Error::root("source_map_origin_invalid"));
            }
            if let FileOrigin::Metadata { uuid, .. } = &entry.origin
                && !identities.insert(uuid.key())
            {
                return Err(Error::root("source_map_origin_invalid"));
            }
            entries.insert(
                entry.path.clone(),
                FileEntry {
                    path: entry.path.clone(),
                    bytes: entry.bytes,
                    sha256: entry.sha256.clone(),
                    role: FileRole::Preserved,
                },
            );
        }
        self.validate_forms()?;
        query::validate_map(self)?;
        if files::revision(&entries, &self.directories) != self.native_revision {
            return Err(Error::root("source_map_inventory_invalid"));
        }
        let (path, source) = self.external_root.as_ref().map_or(
            (
                SourcePath::parse("Configuration.xml")?,
                "configuration.yaml",
            ),
            |root| (root.path.clone(), root.kind.source()),
        );
        let root = self.file(&path);
        if !matches!(root.map(|entry| &entry.origin), Some(FileOrigin::Metadata { accepted, .. }) if accepted.path.as_str() == source)
        {
            return Err(Error::root("source_map_origin_invalid"));
        }
        Ok(())
    }

    /// Schema and root profile jointly select the origin envelope; neither may spoof the other.
    fn validate_schema(&self) -> Result<(), Error> {
        if !matches!(
            self.schema_version,
            SOURCE_MAP_VERSION
                | RELOCATED_SOURCE_MAP_VERSION
                | MANAGED_FORM_SOURCE_MAP_VERSION
                | QUERY_SOURCE_MAP_VERSION
                | EXTERNAL_SOURCE_MAP_VERSION
                | COMPOSITION_SOURCE_MAP_VERSION
                | CREATED_SOURCE_MAP_VERSION
        ) || !matches!(
            self.source_schema_version,
            super::super::SCHEMA_VERSION | super::super::READABLE_SCHEMA_VERSION
        ) || (self.schema_version == SOURCE_MAP_VERSION
            && self.source_schema_version != super::super::SCHEMA_VERSION)
            || self.rules_version != super::super::RULES_VERSION
        {
            return Err(Error::root("source_map_schema_unsupported"));
        }
        let profile_valid = self.external_root.as_ref().map_or_else(
            || {
                self.schema_version != EXTERNAL_SOURCE_MAP_VERSION
                    && self.profile == super::super::PROFILE
            },
            |root| {
                matches!(
                    self.schema_version,
                    EXTERNAL_SOURCE_MAP_VERSION
                        | COMPOSITION_SOURCE_MAP_VERSION
                        | CREATED_SOURCE_MAP_VERSION
                ) && self.profile == root.kind.profile()
                    && root.validate().is_ok()
            },
        );
        if !profile_valid {
            return Err(Error::root("source_map_schema_unsupported"));
        }
        if self.schema_version >= MANAGED_FORM_SOURCE_MAP_VERSION
            && self.schema_version < COMPOSITION_SOURCE_MAP_VERSION
            && self.source_schema_version != super::super::READABLE_SCHEMA_VERSION
        {
            return Err(Error::root("source_map_schema_unsupported"));
        }
        Ok(())
    }

    /// A dialog and its descriptor share one native owner and the exact same authoring snapshots.
    fn validate_forms(&self) -> Result<(), Error> {
        let mut descriptors = BTreeSet::new();
        for entry in &self.files {
            if let FileOrigin::ManagedForm {
                uuid,
                descriptor,
                accepted,
                working,
                ..
            } = &entry.origin
                && (!matches!(
                    self.schema_version,
                    MANAGED_FORM_SOURCE_MAP_VERSION
                        | QUERY_SOURCE_MAP_VERSION
                        | EXTERNAL_SOURCE_MAP_VERSION
                        | COMPOSITION_SOURCE_MAP_VERSION
                        | CREATED_SOURCE_MAP_VERSION
                ) || !descriptors.insert(descriptor)
                    || !matches!(self.file(descriptor).map(|file| &file.origin),
                        Some(FileOrigin::Metadata { uuid: owner, accepted: input, working: overlay, .. })
                        if owner == uuid && input == accepted && overlay == working))
            {
                return Err(Error::root("source_map_origin_invalid"));
            }
        }
        Ok(())
    }

    /// Hash the validated DTO in its defined field and native-path order, independent of locale.
    ///
    /// # Errors
    /// Returns invalid origin data or JSON serialization failure.
    pub fn checksum(&self) -> Result<String, Error> {
        self.validate()?;
        let bytes =
            serde_json::to_vec(self).map_err(|_| Error::root("source_map_schema_invalid"))?;
        Ok(files::hex(&Sha256::digest(bytes)))
    }
}

/// Reuse the protected selector's authorities rather than guessing from editable source flags.
pub(super) fn create(
    working: &Inventory,
    base: &AcceptedBase,
    package: &Package,
    native: &Inventory,
    xml: &BTreeMap<SourcePath, String>,
    origins: &BTreeMap<SourcePath, Origin>,
    generation: String,
) -> Result<SourceMap, Error> {
    let descriptors: BTreeMap<_, _> = base
        .package
        .manifest
        .files
        .iter()
        .filter_map(|entry| {
            if let FileRole::Descriptor { source, uuid } = &entry.role {
                Some((&entry.path, (source, uuid)))
            } else {
                None
            }
        })
        .collect();
    let forms = base
        .package
        .manifest
        .files
        .iter()
        .filter_map(|file| {
            if let FileRole::ManagedForm { source, descriptor } = &file.role {
                Some((&file.path, (source, descriptor)))
            } else {
                None
            }
        })
        .collect::<BTreeMap<_, _>>();
    let mapped = native
        .files
        .values()
        .map(|entry| {
            let origin = if let Some((source, uuid)) = descriptors.get(&entry.path) {
                let accepted = (!base.created_owner(uuid))
                    .then(|| base.origin_entry(source))
                    .flatten();
                let uses_working_values = package.documents.get(&entry.path)
                    != base.package.documents.get(&entry.path)
                    || base.composition_changed();
                authoring_origin(
                    uuid,
                    accepted,
                    working.files.get(*source),
                    uses_working_values,
                    None,
                    Vec::new(),
                )?
            } else if let Some((source, descriptor)) = forms.get(&entry.path) {
                let (_, uuid) = descriptors
                    .get(*descriptor)
                    .ok_or_else(|| Error::root("source_baseline_corrupt"))?;
                let queries = query::origins(working, base, package, entry, xml, origins)?;
                authoring_origin(
                    uuid,
                    (!base.created_owner(uuid))
                        .then(|| base.origin_entry(source))
                        .flatten(),
                    working.files.get(*source),
                    package.forms.get(&entry.path) != base.package.forms.get(&entry.path)
                        || base.composition_changed(),
                    Some(descriptor),
                    queries,
                )?
            } else if package.query_inputs.resources.contains_key(&entry.path) {
                query::resource_origin(working, base, package, entry, xml, origins)?
            } else {
                payload_origin(base, working, origins, entry)?
            };
            Ok(MappedFile {
                path: entry.path.clone(),
                bytes: entry.bytes,
                sha256: entry.sha256.clone(),
                origin,
            })
        })
        .collect::<Result<_, Error>>()?;
    let source_schema_version = descriptor_schema(base, package)?;
    let map = SourceMap {
        schema_version: if base.has_created_owners() {
            CREATED_SOURCE_MAP_VERSION
        } else if base.composition_changed() {
            COMPOSITION_SOURCE_MAP_VERSION
        } else {
            map_version(base.package.manifest.schema_version, source_schema_version)
        },
        source_schema_version,
        rules_version: super::super::RULES_VERSION,
        profile: base.package.manifest.profile.clone(),
        external_root: base.package.manifest.external_root.clone(),
        generation,
        input_revision: working.revision.clone(),
        baseline: base.reference().sha256.clone(),
        native_revision: native.revision.clone(),
        directories: native.directories.clone(),
        files: mapped,
    };
    map.validate()?;
    Ok(map)
}

/// A composite origin envelope must support the highest descriptor schema actually selected.
fn descriptor_schema(base: &AcceptedBase, package: &Package) -> Result<u32, Error> {
    base.package
        .documents
        .values()
        .chain(package.documents.values())
        .map(|document| document.descriptor.schema_version)
        .max()
        .ok_or_else(|| Error::root("source_map_origin_invalid"))
}

/// Original owners retain actual accepted evidence; complete new owners expose only working bytes.
fn authoring_origin(
    uuid: &NativeUuid,
    accepted: Option<&FileEntry>,
    working: Option<&FileEntry>,
    uses_working_values: bool,
    descriptor: Option<&SourcePath>,
    queries: Vec<QueryOrigin>,
) -> Result<FileOrigin, Error> {
    let working = working.map(InputFile::from);
    let uuid = uuid.clone();
    if let Some(accepted) = accepted {
        let accepted = accepted.into();
        Ok(if let Some(descriptor) = descriptor {
            FileOrigin::ManagedForm {
                uuid,
                descriptor: descriptor.clone(),
                accepted,
                working,
                uses_working_values,
                queries,
            }
        } else {
            FileOrigin::Metadata {
                uuid,
                accepted,
                working,
                uses_working_values,
            }
        })
    } else {
        let input = working.ok_or_else(|| Error::root("source_map_origin_missing"))?;
        Ok(if let Some(descriptor) = descriptor {
            FileOrigin::CreatedManagedForm {
                uuid,
                descriptor: descriptor.clone(),
                input,
                queries,
            }
        } else {
            FileOrigin::CreatedMetadata { uuid, input }
        })
    }
}

/// Origin schema follows actual physical mappings while descriptor envelope versions stay independent.
const fn map_version(layout: u32, descriptor: u32) -> u32 {
    if layout == super::super::EXTERNAL_LAYOUT_VERSION {
        EXTERNAL_SOURCE_MAP_VERSION
    } else if layout == super::super::QUERY_LAYOUT_VERSION {
        QUERY_SOURCE_MAP_VERSION
    } else if layout == super::super::MANAGED_FORM_LAYOUT_VERSION {
        MANAGED_FORM_SOURCE_MAP_VERSION
    } else if layout == super::super::RESOURCE_LAYOUT_VERSION
        || descriptor == super::super::READABLE_SCHEMA_VERSION
    {
        RELOCATED_SOURCE_MAP_VERSION
    } else {
        SOURCE_MAP_VERSION
    }
}

/// Byte-preserved payloads retain the selector's exact input authority and physical source path.
pub(super) fn payload_origin(
    base: &AcceptedBase,
    working: &Inventory,
    origins: &BTreeMap<SourcePath, Origin>,
    entry: &FileEntry,
) -> Result<FileOrigin, Error> {
    let source = base
        .package
        .payload_sources
        .get(&entry.path)
        .ok_or_else(|| Error::root("source_map_origin_missing"))?;
    let (authority, input) = match origins.get(source) {
        Some(Origin::Working) => (InputAuthority::Working, working.files.get(source)),
        Some(Origin::Accepted) => (InputAuthority::Accepted, base.origin_entry(source)),
        None => return Err(Error::root("source_map_origin_missing").in_field(entry.path.as_str())),
    };
    let input = input.ok_or_else(|| Error::root("source_map_origin_missing"))?;
    Ok(FileOrigin::Payload {
        authority,
        input: input.into(),
        module: native::is_module_path(entry.path.as_path()),
        owner: native_file_owner(base, &entry.path),
    })
}

/// A payload's origin must identify the same exact native bytes; metadata records remain composite.
fn validate_origin(entry: &MappedFile) -> Result<(), Error> {
    if let Some(origin) = created::existing(&entry.origin) {
        return validate_origin(&MappedFile {
            origin,
            ..entry.clone()
        });
    }
    match &entry.origin {
        FileOrigin::Payload { input, module, .. } => {
            require_digest(&input.sha256)?;
            if input.bytes != entry.bytes
                || input.sha256 != entry.sha256
                || *module != native::is_module_path(entry.path.as_path())
            {
                return Err(Error::root("source_map_origin_invalid"));
            }
        }
        FileOrigin::Metadata {
            accepted,
            working,
            uses_working_values,
            ..
        }
        | FileOrigin::ManagedForm {
            accepted,
            working,
            uses_working_values,
            ..
        } => {
            require_digest(&accepted.sha256)?;
            if accepted.path.as_path().extension() != Some(std::ffi::OsStr::new("yaml"))
                || !entry
                    .path
                    .as_path()
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
                || accepted.bytes == 0
            {
                return Err(Error::root("source_map_origin_invalid"));
            }
            if *uses_working_values && working.is_none() {
                return Err(Error::root("source_map_origin_invalid"));
            }
            if let Some(working) = working {
                require_digest(&working.sha256)?;
            }
            if let FileOrigin::ManagedForm { descriptor, .. } = &entry.origin
                && !super::super::layout::forms::native_path(descriptor)?
                    .as_str()
                    .eq_ignore_ascii_case(entry.path.as_str())
            {
                return Err(Error::root("source_map_origin_invalid"));
            }
        }
        FileOrigin::QueryResource { input, .. } => {
            require_digest(&input.sha256)?;
            if input.bytes == 0
                || !input
                    .path
                    .as_path()
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
            {
                return Err(Error::root("source_map_origin_invalid"));
            }
        }
        FileOrigin::CreatedMetadata { .. } | FileOrigin::CreatedManagedForm { .. } => {
            return Err(Error::root("source_map_origin_invalid"));
        }
    }
    query::validate_origin(entry)?;
    Ok(())
}

/// Content identifiers use one canonical lowercase SHA-256 spelling.
fn require_digest(value: &str) -> Result<(), Error> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        Ok(())
    } else {
        Err(Error::root("source_map_fingerprint_invalid"))
    }
}
