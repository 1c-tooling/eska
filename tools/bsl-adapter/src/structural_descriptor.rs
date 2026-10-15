//! Non-BSL leaf descriptors retain current source structure without inventing a kernel value.

#[cfg(test)]
mod tests;

use eska::project_core::{MetadataKind, metadata_parser::ParsedDescriptor};

use crate::{AdapterError, AdapterIssue, identity, properties::Properties};

mod presentation;

/// Admit only reviewed non-BSL leaf kinds; callers must still validate their current descriptor.
#[must_use]
pub const fn is_structural_descriptor_kind(kind: MetadataKind) -> bool {
    matches!(
        kind,
        MetadataKind::Language
            | MetadataKind::Template
            | MetadataKind::CommonTemplate
            | MetadataKind::CommonPicture
            | MetadataKind::CommandGroup
    )
}

/// Validate current non-BSL leaves independently of image/template payloads and query context.
///
/// This checks the descriptor's actual fields, including explicit absence, without supplying platform
/// defaults. Success permits structural coverage only; no BSL canonical object or DCS model exists.
///
/// # Errors
/// Returns wrong kind, incomplete leaf structure, invalid UUID, unloaded/duplicate/invalid properties
/// or an explicit unsupported enum value, with ranges in the caller's exact source snapshot.
pub fn structural_descriptor(descriptor: &ParsedDescriptor) -> Result<(), AdapterError> {
    let root = identity::descriptor_root(descriptor)?;
    let kind = root.metadata.kind();
    if !is_structural_descriptor_kind(kind) {
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
    match kind {
        MetadataKind::Language => {
            fields.text("LanguageCode")?;
        }
        MetadataKind::CommonPicture | MetadataKind::CommandGroup => {
            presentation::validate(root, &fields)?;
        }
        _ => enum_value(
            &fields,
            "TemplateType",
            &[
                "ActiveDocument",
                "AddIn",
                "BinaryData",
                "DataCompositionAppearanceTemplate",
                "DataCompositionSchema",
                "GeographicalSchema",
                "GraphicalSchema",
                "HTMLDocument",
                "SpreadsheetDocument",
                "TextDocument",
            ],
        )?,
    }
    Ok(())
}

/// An absent enum is not a default, and an unrecognized explicit token cannot gain coverage.
fn enum_value(fields: &Properties<'_>, name: &str, allowed: &[&str]) -> Result<(), AdapterError> {
    if let Some(field) = fields.field(name)?
        && let Some(value) = fields.text(name)?
        && !allowed.contains(&value)
    {
        return Err(AdapterError {
            issue: AdapterIssue::UnsupportedPropertyValue {
                name: name.into(),
                value: value.into(),
            },
            range: field.range.clone(),
        });
    }
    Ok(())
}
