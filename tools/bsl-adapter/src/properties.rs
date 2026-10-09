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
