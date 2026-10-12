//! One YAML per main owner, with an explicit reversible map for every native input file.

use std::{collections::BTreeSet, path::Path};

use serde::{Deserialize, Deserializer, Serialize};

use super::{Descriptor, Error, NativeUuid, PROFILE, RULES_VERSION, SCHEMA_VERSION, yaml};

/// A portable, nonempty relative path; deserialization cannot introduce traversal or aliases.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SourcePath(String);

impl SourcePath {
    /// Check path components before using a native or YAML path in filesystem operations.
    ///
    /// # Errors
    /// Returns `source_path_invalid` for absolute, ambiguous or nonportable paths.
    pub fn parse(value: &str) -> Result<Self, Error> {
        if value.is_empty()
            || value.contains(['\\', ':', '\0', '*', '?', '"', '<', '>', '|'])
            || value.chars().any(char::is_control)
            || value.split('/').any(|part| {
                part.is_empty()
                    || matches!(part, "." | "..")
                    || part.ends_with(['.', ' '])
                    || device_name(part)
            })
        {
            return Err(Error::root("source_path_invalid"));
        }
        Ok(Self(value.into()))
    }

    /// Return the portable relative spelling, retaining Unicode and dotted names.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Convert to a host path only after the relative-path invariant has been established.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }
}

impl<'de> Deserialize<'de> for SourcePath {
    /// Apply the same boundary checks to a persisted file map and to native discovery.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// Native descriptor identity remains independent of its physical authoring file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeDocument {
    pub path: SourcePath,
    pub descriptor: Descriptor,
}

/// One authoring file contains a primary descriptor and optional legacy family members.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectSource {
    pub document: NativeDocument,
    pub nested: Vec<NativeDocument>,
}

impl ObjectSource {
    /// Read strict YAML and validate every native descriptor and its file identity.
    ///
    /// # Errors
    /// Returns schema, XML, path and duplicate-identity failures without normalization.
    pub fn from_yaml(input: &str) -> Result<Self, Error> {
        Self::from_value(yaml::decode(input)?)
    }

    /// Use the same schema boundary when a reader also retains the source CST.
    pub(super) fn from_value(value: serde_json::Value) -> Result<Self, Error> {
        let value: Self =
            serde_json::from_value(value).map_err(|_| Error::root("source_schema_invalid"))?;
        value.validate()?;
        Ok(value)
    }

    /// Serialize one complete owner deterministically for repeatable imports.
    ///
    /// # Errors
    /// Returns a validation failure before creating a source file.
    pub fn to_yaml(&self) -> Result<String, Error> {
        self.validate()?;
        yaml::encode(&serde_json::to_value(self).map_err(|_| Error::root("source_schema_invalid"))?)
    }

    /// Reject a repeated physical descriptor inside an owner's document.
    ///
    /// # Errors
    /// Returns invalid descriptor content or `source_descriptor_duplicate`.
    pub fn validate(&self) -> Result<(), Error> {
        let mut paths = BTreeSet::new();
        for part in std::iter::once(&self.document).chain(&self.nested) {
            if !paths.insert(&part.path) {
                return Err(Error::root("source_descriptor_duplicate"));
            }
            part.descriptor
                .validate()
                .map_err(|error| error.in_field(part.path.as_str()))?;
        }
        Ok(())
    }
}

/// The native file map is separate from the authoritative declared object composition.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceManifest {
    pub schema_version: u32,
    pub rules_version: u32,
    pub profile: String,
    pub input_revision: String,
    pub directories: Vec<SourcePath>,
    pub files: Vec<FileEntry>,
    /// Reverse context is required for separated descriptors, never inferred by scanning files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub descriptor_contexts: Vec<super::DescriptorContext>,
}

/// Original byte fingerprints record the input, rather than claiming edited YAML is unchanged.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileEntry {
    pub path: SourcePath,
    pub bytes: u64,
    pub sha256: String,
    pub role: FileRole,
}

/// Declared descriptors are rebuilt; every other file remains an exact separate payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FileRole {
    Descriptor {
        source: SourcePath,
        uuid: NativeUuid,
    },
    Preserved,
    /// Exact payload bytes live at an explicit authoring location instead of the native path.
    Relocated {
        source: SourcePath,
    },
}

impl FileEntry {
    /// Resolve the physical authoring file without confusing it with native output identity.
    #[must_use]
    pub const fn source_path(&self) -> &SourcePath {
        match &self.role {
            FileRole::Descriptor { source, .. } | FileRole::Relocated { source } => source,
            FileRole::Preserved => &self.path,
        }
    }

    /// Opaque payloads retain their bytes regardless of their physical authoring location.
    #[must_use]
    pub const fn is_payload(&self) -> bool {
        !matches!(self.role, FileRole::Descriptor { .. })
    }
}

impl SourceManifest {
    /// Validate the persisted native map without inferring active metadata from directory names.
    ///
    /// # Errors
    /// Returns unsupported versions or malformed, colliding file-map entries.
    pub fn validate(&self) -> Result<(), Error> {
        if self.schema_version != SCHEMA_VERSION
            && self.schema_version != super::DESCRIPTOR_LAYOUT_VERSION
            && self.schema_version != super::RESOURCE_LAYOUT_VERSION
        {
            return Err(Error::root("source_schema_unsupported"));
        }
        if self.rules_version != RULES_VERSION {
            return Err(Error::root("source_rules_unsupported"));
        }
        if self.profile != PROFILE {
            return Err(Error::root("source_profile_unsupported"));
        }
        if !hash(&self.input_revision) {
            return Err(Error::root("source_hash_invalid"));
        }
        let mut paths = BTreeSet::new();
        for path in self
            .directories
            .iter()
            .chain(self.files.iter().map(|file| &file.path))
        {
            if !paths.insert(path.as_str().to_lowercase()) {
                return Err(Error::root("source_path_collision"));
            }
        }
        for file in &self.files {
            if !hash(&file.sha256) {
                return Err(Error::root("source_hash_invalid"));
            }
        }
        super::layout::validate_map(self)?;
        Ok(())
    }
}

/// Provenance is machine data; compact v2 avoids exceeding the reader quota on large configurations.
pub(super) fn manifest_text(manifest: &SourceManifest) -> Result<String, Error> {
    let text = if manifest.schema_version >= super::DESCRIPTOR_LAYOUT_VERSION {
        serde_json::to_string(manifest)
    } else {
        serde_json::to_string_pretty(manifest)
    }
    .map_err(|_| Error::root("source_schema_invalid"))?;
    if text.len() as u64 > super::files::MAX_TEXT_BYTES {
        return Err(Error::root("source_document_too_large"));
    }
    Ok(text)
}

/// SHA-256 fingerprints use one stable lowercase hexadecimal spelling.
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

/// Windows device basenames remain reserved even with an extension or different letter case.
fn device_name(part: &str) -> bool {
    let name = part
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(name.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((name.starts_with("COM") || name.starts_with("LPT"))
            && name.len() == 4
            && matches!(name.as_bytes()[3], b'1'..=b'9'))
}
