//! Non-BSL leaf descriptors retain current source structure without inventing a kernel value.

#[cfg(test)]
mod tests;

use eska::project_core::{MetadataKind, metadata_parser::ParsedDescriptor};

use crate::{AdapterError, AdapterIssue, identity, properties::Properties};

/// Validate current language/template metadata independently of template payloads and query context.
///
/// This checks the descriptor's actual fields, including explicit absence, without supplying platform
/// defaults. Success permits structural coverage only; no BSL canonical object or DCS model exists.
///
/// # Errors
/// Returns wrong kind, incomplete leaf structure, invalid UUID, unloaded/duplicate/invalid properties
/// or an explicit unsupported template type, with ranges in the caller's exact source snapshot.
pub fn structural_descriptor(descriptor: &ParsedDescriptor) -> Result<(), AdapterError> {
    let root = identity::descriptor_root(descriptor)?;
    let kind = root.metadata.kind();
    if !matches!(
        kind,
        MetadataKind::Language | MetadataKind::Template | MetadataKind::CommonTemplate
    ) {
        return Err(AdapterError {
            issue: AdapterIssue::WrongKind { actual: kind },
            range: root.range.clone(),
        });
    }
    if let Some(child) = descriptor.objects.get(1) {
        return Err(AdapterError {
            issue: AdapterIssue::IncompleteDescriptor,
            range: child.range.clone(),
        });
    }
    if let Some(reference) = descriptor.references.first() {
        return Err(AdapterError {
            issue: AdapterIssue::IncompleteDescriptor,
            range: reference.range.clone(),
        });
    }
    let _uuid = identity::uuid(root)?;
    let fields = Properties::new(root)?;
    fields.text("Name")?;
    fields.text("Comment")?;
    // The shared parser validates localized values; uniqueness still belongs to this projection.
    fields.field("Synonym")?;
    identity::ownership(&fields)?;
    if kind == MetadataKind::Language {
        fields.text("LanguageCode")?;
    } else if let Some(value) = fields.text("TemplateType")?
        && !matches!(
            value,
            "ActiveDocument"
                | "AddIn"
                | "BinaryData"
                | "DataCompositionAppearanceTemplate"
                | "DataCompositionSchema"
                | "GeographicalSchema"
                | "GraphicalSchema"
                | "HTMLDocument"
                | "SpreadsheetDocument"
                | "TextDocument"
        )
    {
        let range = fields
            .field("TemplateType")?
            .map_or_else(|| root.range.clone(), |field| field.range.clone());
        return Err(AdapterError {
            issue: AdapterIssue::UnsupportedPropertyValue {
                name: "TemplateType".into(),
                value: value.into(),
            },
            range,
        });
    }
    Ok(())
}
