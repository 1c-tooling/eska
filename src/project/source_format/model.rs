//! Lossless source values, separate from the existing semantic metadata projection.

use serde::{Deserialize, Deserializer, Serialize};

use crate::project::metadata_model::MetadataKind;

use super::Error;

/// A checked native UUID; no path-derived replacement identity is introduced.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct NativeUuid(String);

impl NativeUuid {
    /// Validate the observed native spelling without normalizing letter case.
    ///
    /// # Errors
    /// Returns `native_uuid_invalid` for malformed UUIDs.
    pub fn parse(input: &str) -> Result<Self, Error> {
        let valid = input.len() == 36
            && input.bytes().enumerate().all(|(index, byte)| {
                if matches!(index, 8 | 13 | 18 | 23) {
                    byte == b'-'
                } else {
                    byte.is_ascii_hexdigit()
                }
            });
        if !valid {
            return Err(Error::root("native_uuid_invalid"));
        }
        Ok(Self(input.to_owned()))
    }

    /// Return the exact original spelling for native output.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Compare native identity independently of hexadecimal letter case.
    pub(crate) fn key(&self) -> String {
        self.0.to_ascii_lowercase()
    }
}

impl<'de> Deserialize<'de> for NativeUuid {
    /// Enforce the UUID invariant at every deserialization boundary.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = String::deserialize(deserializer)?;
        Self::parse(&input).map_err(serde::de::Error::custom)
    }
}

/// One main-object YAML also contains its inline and external nested descriptors.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    pub schema_version: u32,
    pub rules_version: u32,
    pub profile: String,
    pub xml: XmlProfile,
    pub object: MetadataNode,
}

/// XML transport details are distinct from the 1C project's own Version property.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XmlProfile {
    pub version: String,
    pub namespaces: Vec<Namespace>,
    pub attributes: Vec<XmlAttribute>,
    pub bom: bool,
    pub line_ending: LineEnding,
}

/// Keep the observed XML EOL policy; generated metadata uses one consistent convention.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineEnding {
    Lf,
    CrLf,
}

/// Local bindings retain prefixes; inherited context is supplied by the owning document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Namespace {
    pub prefix: Option<String>,
    pub uri: String,
}

/// Preserve extra XML attributes without treating them as editable metadata properties.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XmlAttribute {
    pub name: String,
    pub value: String,
}

/// An opaque part keeps its slot and local bindings relative to its metadata owner.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XmlFragment {
    pub xml: String,
    pub namespaces: Vec<Namespace>,
}

/// Preserve the ordered native sections and every nested identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataNode {
    pub kind: MetadataKind,
    pub uuid: NativeUuid,
    pub namespaces: Vec<Namespace>,
    pub attributes: Vec<XmlAttribute>,
    pub sections: Vec<Section>,
}

/// Semantic sections have typed contents; unsupported sections remain opaque in their slot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "items",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Section {
    Properties(Vec<Property>),
    Children(Vec<Child>),
    Preserved(XmlFragment),
}

/// Inline parts and declared external descriptors share native kinds, not filesystem guesses.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Child {
    Inline {
        object: Box<MetadataNode>,
    },
    Reference {
        metadata_kind: MetadataKind,
        name: String,
    },
    Preserved {
        fragment: XmlFragment,
    },
}

/// Only verified properties have editable typed values; foreign `QNames` cannot spoof them.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Property {
    Typed {
        name: PropertyName,
        value: PropertyValue,
    },
    Preserved {
        fragment: XmlFragment,
    },
}

/// The initial verified property vocabulary; adding a name requires its own inverse rule.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum PropertyName {
    Name,
    Comment,
    Version,
    Synonym,
}

impl PropertyName {
    /// Return the native property tag without localized aliases.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Comment => "Comment",
            Self::Version => "Version",
            Self::Synonym => "Synonym",
        }
    }
}

/// Absence is the absence of a property entry, distinct from Empty and Nil.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum PropertyValue {
    Text(String),
    Empty,
    Nil,
    Localized(Vec<LocalizedText>),
}

/// A native language key is preserved verbatim, without locale normalization.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalizedText {
    pub language: String,
    pub text: String,
}

impl MetadataNode {
    /// Find one verified property without interpreting preserved XML as editable data.
    #[must_use]
    pub fn property(&self, name: PropertyName) -> Option<&PropertyValue> {
        self.sections.iter().find_map(|section| match section {
            Section::Properties(properties) => {
                properties.iter().find_map(|property| match property {
                    Property::Typed { name: key, value } if *key == name => Some(value),
                    _ => None,
                })
            }
            _ => None,
        })
    }
}
