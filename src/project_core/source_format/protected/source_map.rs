//! Native-file origins describe accepted bindings with selected values and supported inline creation.

mod form;
mod location;

pub use location::{OriginTexts, SourceLocation};

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{Origin, native_file_owner};
use crate::project_core::{
    native,
    source_format::{
        AcceptedBase, Error, FileEntry, FileRole, NativeUuid, SourcePath,
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
    ManagedForm {
        uuid: NativeUuid,
        descriptor: SourcePath,
        accepted: InputFile,
        #[serde(skip_serializing_if = "Option::is_none")]
        working: Option<InputFile>,
        uses_working_values: bool,
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
        if !matches!(
            self.schema_version,
            SOURCE_MAP_VERSION | RELOCATED_SOURCE_MAP_VERSION | MANAGED_FORM_SOURCE_MAP_VERSION
        ) || !matches!(
            self.source_schema_version,
            super::super::SCHEMA_VERSION | super::super::READABLE_SCHEMA_VERSION
        ) || (self.schema_version == SOURCE_MAP_VERSION
            && self.source_schema_version != super::super::SCHEMA_VERSION)
            || self.rules_version != super::super::RULES_VERSION
            || self.profile != super::super::PROFILE
        {
            return Err(Error::root("source_map_schema_unsupported"));
        }
        if self.schema_version == MANAGED_FORM_SOURCE_MAP_VERSION
            && self.source_schema_version != super::super::READABLE_SCHEMA_VERSION
        {
            return Err(Error::root("source_map_schema_unsupported"));
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
        if files::revision(&entries, &self.directories) != self.native_revision {
            return Err(Error::root("source_map_inventory_invalid"));
        }
        let root = self.file(&SourcePath::parse("Configuration.xml")?);
        if !matches!(root.map(|entry| &entry.origin), Some(FileOrigin::Metadata { accepted, .. }) if accepted.path.as_str() == "configuration.yaml")
        {
            return Err(Error::root("source_map_origin_invalid"));
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
                && (self.schema_version != MANAGED_FORM_SOURCE_MAP_VERSION
                    || !descriptors.insert(descriptor)
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
                let accepted = base
                    .package
                    .snapshot
                    .files
                    .get(*source)
                    .ok_or_else(|| Error::root("source_baseline_corrupt"))?;
                let uses_working_values =
                    package.documents.get(&entry.path) != base.package.documents.get(&entry.path);
                FileOrigin::Metadata {
                    uuid: (*uuid).clone(),
                    accepted: accepted.into(),
                    working: working.files.get(*source).map(InputFile::from),
                    uses_working_values,
                }
            } else if let Some((source, descriptor)) = forms.get(&entry.path) {
                let (_, uuid) = descriptors
                    .get(*descriptor)
                    .ok_or_else(|| Error::root("source_baseline_corrupt"))?;
                let accepted = base
                    .package
                    .snapshot
                    .files
                    .get(*source)
                    .ok_or_else(|| Error::root("source_baseline_corrupt"))?;
                FileOrigin::ManagedForm {
                    uuid: (*uuid).clone(),
                    descriptor: (*descriptor).clone(),
                    accepted: accepted.into(),
                    working: working.files.get(*source).map(InputFile::from),
                    uses_working_values: package.forms.get(&entry.path)
                        != base.package.forms.get(&entry.path),
                }
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
    let source_schema_version = base
        .package
        .documents
        .values()
        .chain(package.documents.values())
        .map(|document| document.descriptor.schema_version)
        .max()
        .ok_or_else(|| Error::root("source_map_origin_invalid"))?;
    let map = SourceMap {
        schema_version: map_version(base.package.manifest.schema_version, source_schema_version),
        source_schema_version,
        rules_version: super::super::RULES_VERSION,
        profile: super::super::PROFILE.into(),
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

/// Origin schema follows actual physical mappings while descriptor envelope versions stay independent.
const fn map_version(layout: u32, descriptor: u32) -> u32 {
    if layout == super::super::MANAGED_FORM_LAYOUT_VERSION {
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
fn payload_origin(
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
    let (authority, snapshot) = match origins.get(source) {
        Some(Origin::Working) => (InputAuthority::Working, working),
        Some(Origin::Accepted) => (InputAuthority::Accepted, &base.package.snapshot),
        None => return Err(Error::root("source_map_origin_missing").in_field(entry.path.as_str())),
    };
    let input = snapshot
        .files
        .get(source)
        .ok_or_else(|| Error::root("source_map_origin_missing"))?;
    Ok(FileOrigin::Payload {
        authority,
        input: input.into(),
        module: native::is_module_path(entry.path.as_path()),
        owner: native_file_owner(base, &entry.path),
    })
}

/// A payload's origin must identify the same exact native bytes; metadata records remain composite.
fn validate_origin(entry: &MappedFile) -> Result<(), Error> {
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
                if working.path != accepted.path {
                    return Err(Error::root("source_map_origin_invalid"));
                }
            }
            if let FileOrigin::ManagedForm { descriptor, .. } = &entry.origin
                && !super::super::layout::forms::native_path(descriptor)?
                    .as_str()
                    .eq_ignore_ascii_case(entry.path.as_str())
            {
                return Err(Error::root("source_map_origin_invalid"));
            }
        }
    }
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
