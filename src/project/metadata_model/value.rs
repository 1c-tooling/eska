//! Owned logical property values; no XML nodes, input buffers or physical locations.

/// Namespace-aware property or qualifier key; prefixes do not participate in equality.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertyKey {
    pub namespace: Option<String>,
    pub name: String,
}

/// A property and its qualified value annotations, preserving unknown property names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataProperty {
    pub key: PropertyKey,
    pub qualifiers: Vec<(PropertyKey, String)>,
    pub value: MetadataValue,
}

/// One localized value, retaining its language key and content without UI locale selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalizedText {
    pub language: String,
    pub content: String,
}

/// Structured metadata values, independent of the parser's DOM and its source representation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetadataValue {
    Text(String),
    Localized(Vec<LocalizedText>),
    /// Ordered fields retain repeated keys (lists) instead of silently deduplicating them.
    Record(Vec<MetadataProperty>),
    /// An explicitly unsupported shape remains addressable through source mapping.
    Unsupported(ValueIssue),
}

/// A shape the value layer cannot represent safely as ordinary text or record fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueIssue {
    MixedContent,
    InvalidLocalizedText,
}
