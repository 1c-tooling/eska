//! Lossless source values, separate from the existing semantic metadata projection.

use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

use crate::project_core::MetadataKind;

use super::{Error, ExactUnsigned};

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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Descriptor {
    pub schema_version: u32,
    pub rules_version: u32,
    pub profile: String,
    pub xml: XmlProfile,
    pub object: MetadataNode,
}

/// The original wire representation remains readable without implicitly changing its schema.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyDescriptor {
    schema_version: u32,
    rules_version: u32,
    profile: String,
    xml: XmlProfile,
    object: MetadataNode,
}

impl Serialize for Descriptor {
    /// Schema selects a wire envelope; the lossless native model remains shared by all consumers.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.schema_version == super::READABLE_SCHEMA_VERSION {
            return super::readable::serialize(self, serializer);
        }
        let mut fields = serializer.serialize_struct("Descriptor", 5)?;
        fields.serialize_field("schema_version", &self.schema_version)?;
        fields.serialize_field("rules_version", &self.rules_version)?;
        fields.serialize_field("profile", &self.profile)?;
        fields.serialize_field("xml", &self.xml)?;
        fields.serialize_field("object", &self.object)?;
        fields.end()
    }
}

impl<'de> Deserialize<'de> for Descriptor {
    /// A readable envelope cannot fall back to legacy fields or silently promote preserved XML.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        if value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            == Some(u64::from(super::READABLE_SCHEMA_VERSION))
        {
            return super::readable::deserialize(value).map_err(serde::de::Error::custom);
        }
        let legacy: LegacyDescriptor =
            serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(Self {
            schema_version: legacy.schema_version,
            rules_version: legacy.rules_version,
            profile: legacy.profile,
            xml: legacy.xml,
            object: legacy.object,
        })
    }
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

/// Verified native properties; each additional name has an owner-specific inverse rule.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum PropertyName {
    Name,
    Comment,
    Version,
    Synonym,
    Global,
    ClientManagedApplication,
    ClientOrdinaryApplication,
    Server,
    ExternalConnection,
    ServerCall,
    Privileged,
    Hierarchical,
    Autonumbering,
    CheckUnique,
    LimitLevelCount,
    MultiLine,
    PasswordMode,
    MarkNegatives,
    FillFromFillingValue,
    CodeLength,
    DescriptionLength,
    NumberLength,
    LineNumberLength,
    LevelCount,
    CodeType,
    NumberType,
    CodeAllowedLength,
    NumberAllowedLength,
    InformationRegisterPeriodicity,
    ReturnValuesReuse,
    FillChecking,
    Indexing,
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
            Self::Global => "Global",
            Self::ClientManagedApplication => "ClientManagedApplication",
            Self::ClientOrdinaryApplication => "ClientOrdinaryApplication",
            Self::Server => "Server",
            Self::ExternalConnection => "ExternalConnection",
            Self::ServerCall => "ServerCall",
            Self::Privileged => "Privileged",
            Self::Hierarchical => "Hierarchical",
            Self::Autonumbering => "Autonumbering",
            Self::CheckUnique => "CheckUnique",
            Self::LimitLevelCount => "LimitLevelCount",
            Self::MultiLine => "MultiLine",
            Self::PasswordMode => "PasswordMode",
            Self::MarkNegatives => "MarkNegatives",
            Self::FillFromFillingValue => "FillFromFillingValue",
            Self::CodeLength => "CodeLength",
            Self::DescriptionLength => "DescriptionLength",
            Self::NumberLength => "NumberLength",
            Self::LineNumberLength => "LineNumberLength",
            Self::LevelCount => "LevelCount",
            Self::CodeType => "CodeType",
            Self::NumberType => "NumberType",
            Self::CodeAllowedLength => "CodeAllowedLength",
            Self::NumberAllowedLength => "NumberAllowedLength",
            Self::InformationRegisterPeriodicity => "InformationRegisterPeriodicity",
            Self::ReturnValuesReuse => "ReturnValuesReuse",
            Self::FillChecking => "FillChecking",
            Self::Indexing => "Indexing",
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
    Boolean(bool),
    Integer(ExactUnsigned),
    Enumeration(String),
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
