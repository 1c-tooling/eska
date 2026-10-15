//! Read only qualified properties of one logical object; sibling names and namespace prefixes differ.

use eska::project_core::{
    metadata_model::MetadataValue,
    metadata_parser::{LocatedProperty, ParsedObject},
    source_format::MD_NAMESPACE,
};

use crate::{AdapterError, AdapterIssue};

pub struct Properties<'a> {
    fields: &'a [LocatedProperty],
}

impl<'a> Properties<'a> {
    /// Summary mode cannot be interpreted as a model with all flags absent.
    pub fn new(object: &'a ParsedObject) -> Result<Self, AdapterError> {
        let fields = object.properties.as_deref().ok_or_else(|| AdapterError {
            issue: AdapterIssue::PropertiesNotLoaded,
            range: object.range.clone(),
        })?;
        Ok(Self { fields })
    }

    /// Duplicate semantic keys must fail even if the common value reader retains both entries.
    pub fn field(&self, name: &str) -> Result<Option<&'a LocatedProperty>, AdapterError> {
        let mut values = self.fields.iter().filter(|field| {
            field.property.key.namespace.as_deref() == Some(MD_NAMESPACE)
                && field.property.key.name == name
        });
        let first = values.next();
        if let Some(duplicate) = values.next() {
            return Err(AdapterError {
                issue: AdapterIssue::DuplicateProperty { name: name.into() },
                range: duplicate.range.clone(),
            });
        }
        Ok(first)
    }

    /// A scalar's spelling stays intact; qualifiers cannot turn structured values into text.
    pub fn text(&self, name: &str) -> Result<Option<&'a str>, AdapterError> {
        self.field(name)?
            .map(|field| {
                if let MetadataValue::Text(text) = &field.property.value {
                    Ok(text.as_str())
                } else {
                    Err(Self::invalid(field))
                }
            })
            .transpose()
    }

    /// Preserve missing versus explicitly false; invalid tokens never become false by default.
    pub fn boolean(&self, name: &str) -> Result<Option<bool>, AdapterError> {
        self.field(name)?
            .map(|field| {
                let MetadataValue::Text(text) = &field.property.value else {
                    return Err(Self::invalid(field));
                };
                match text.as_str() {
                    "true" => Ok(true),
                    "false" => Ok(false),
                    _ => Err(Self::invalid(field)),
                }
            })
            .transpose()
    }

    /// Lengths are unsigned source values; malformed explicit values cannot become absent defaults.
    pub fn unsigned(&self, name: &str) -> Result<Option<u32>, AdapterError> {
        self.field(name)?
            .map(|field| {
                let MetadataValue::Text(text) = &field.property.value else {
                    return Err(Self::invalid(field));
                };
                text.parse().map_err(|_| Self::invalid(field))
            })
            .transpose()
    }

    /// Ordered metadata-reference lists retain the declared entry kind and reject foreign or malformed entries.
    pub fn references(&self, name: &str, item_name: &str) -> Result<Vec<&'a str>, AdapterError> {
        const READABLE: &str = "http://v8.1c.ru/8.3/xcf/readable";
        let Some(field) = self.field(name)? else {
            return Ok(Vec::new());
        };
        let entries = match &field.property.value {
            MetadataValue::Record(entries) => entries,
            MetadataValue::Text(text) if text.is_empty() => return Ok(Vec::new()),
            _ => return Err(Self::invalid(field)),
        };
        entries
            .iter()
            .map(|entry| {
                if entry.key.name != item_name
                    || !matches!(
                        entry.key.namespace.as_deref(),
                        Some(MD_NAMESPACE | READABLE)
                    )
                {
                    return Err(Self::invalid(field));
                }
                match &entry.value {
                    MetadataValue::Text(text) => Ok(text.as_str()),
                    MetadataValue::TypedText { text, key }
                        if key.namespace.as_deref() == Some(READABLE)
                            && key.name == "MDObjectRef" =>
                    {
                        Ok(text.as_str())
                    }
                    _ => Err(Self::invalid(field)),
                }
            })
            .collect()
    }

    /// Address a field failure at its original qualified-property range.
    pub fn invalid(field: &LocatedProperty) -> AdapterError {
        AdapterError {
            issue: AdapterIssue::InvalidProperty {
                name: field.property.key.name.clone(),
            },
            range: field.range.clone(),
        }
    }
}
