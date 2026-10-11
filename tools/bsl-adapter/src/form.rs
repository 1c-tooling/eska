//! A form's declaration identity stays independent of its explicitly supplied dialog data.

#[cfg(test)]
mod tests;

use bsl_metadata::{Form, FormType};
use eska::project_core::{MetadataKind, metadata_parser::ParsedDescriptor};

use crate::{AdapterError, AdapterIssue, identity, properties::Properties};

/// Combine a captured form declaration with current dialog data supplied by the input owner.
///
/// The caller must distinguish an absent ordinary dialog from a pending, unreadable or invalid
/// declared resource. This function neither reads files nor manufactures elements or handlers.
/// The declaration overrides dialog identity; every supplied payload field retains its order.
///
/// # Errors
/// Returns an incomplete descriptor, wrong kind, missing captures, invalid UUID or form type.
pub fn form(descriptor: &ParsedDescriptor, mut data: Form) -> Result<Form, AdapterError> {
    let root = identity::descriptor_root(descriptor)?;
    if !matches!(
        root.metadata.kind(),
        MetadataKind::Form | MetadataKind::CommonForm
    ) {
        return Err(AdapterError {
            issue: AdapterIssue::WrongKind {
                actual: root.metadata.kind(),
            },
            range: root.range.clone(),
        });
    }
    let fields = Properties::new(root)?;
    let form_type = if let Some(field) = fields.field("FormType")? {
        match fields.text("FormType")? {
            Some("") => FormType::Managed,
            Some(value) => match FormType::from_name(value) {
                FormType::Unknown => return Err(Properties::invalid(field)),
                known => known,
            },
            None => return Err(Properties::invalid(field)),
        }
    } else {
        FormType::Managed
    };
    data.uuid = identity::uuid(root)?;
    root.metadata.name().clone_into(&mut data.name);
    data.form_type = form_type;
    Ok(data)
}
