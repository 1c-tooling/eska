//! Current command parameters are projected directly from the shared property tree.

#[cfg(test)]
mod tests;

use bsl_metadata::{CommandParameterUseMode, CommonCommand, ObjectBelonging};
use eska::project_core::{MetadataKind, metadata_parser::ParsedObject};

use crate::{AdapterError, AdapterIssue, identity, properties::Properties};

/// Project a top-level command without reading a sidecar or treating it as an MDO manager.
///
/// # Errors
/// Rejects missing own parameters, unsupported types, malformed flags, duplicate keys and ownership.
pub fn common_command(object: &ParsedObject) -> Result<CommonCommand, AdapterError> {
    if object.metadata.kind() != MetadataKind::CommonCommand {
        return Err(AdapterError {
            issue: AdapterIssue::WrongKind {
                actual: object.metadata.kind(),
            },
            range: object.range.clone(),
        });
    }
    if !object.children.is_empty() || object.metadata.parent().is_some() {
        return Err(AdapterError {
            issue: AdapterIssue::IncompleteDescriptor,
            range: object.range.clone(),
        });
    }
    let fields = Properties::new(object)?;
    let (belonging, extends) = identity::ownership(&fields)?;
    if (belonging == ObjectBelonging::Adopted) != extends.is_some() {
        return Err(AdapterError {
            issue: AdapterIssue::InvalidProperty {
                name: "ExtendedConfigurationObject".into(),
            },
            range: object.range.clone(),
        });
    }
    let parameter_type = fields.field("CommandParameterType")?;
    let parameter_use = fields.field("ParameterUseMode")?;
    if belonging != ObjectBelonging::Adopted {
        for (name, value) in [
            ("CommandParameterType", parameter_type),
            ("ParameterUseMode", parameter_use),
        ] {
            if value.is_none() {
                return Err(AdapterError {
                    issue: AdapterIssue::MissingProperty { name: name.into() },
                    range: object.range.clone(),
                });
            }
        }
    }
    let parameter_type = parameter_type.map(crate::attribute_type).transpose()?;
    let parameter_use = match fields.text("ParameterUseMode")? {
        None => None,
        Some("Single") => Some(CommandParameterUseMode::Single),
        Some("Multiple") => Some(CommandParameterUseMode::Multiple),
        Some(_) => {
            return Err(AdapterError {
                issue: AdapterIssue::InvalidProperty {
                    name: "ParameterUseMode".into(),
                },
                range: parameter_use
                    .map_or_else(|| object.range.clone(), |field| field.range.clone()),
            });
        }
    };
    // These flags do not affect type lowering, but malformed explicit booleans cannot be admitted.
    fields.boolean("ModifiesData")?;
    fields.boolean("IncludeHelpInContents")?;
    Ok(CommonCommand::from_parts(
        identity::uuid(object)?,
        object.metadata.name().into(),
        parameter_type,
        parameter_use,
        belonging,
        extends,
    ))
}
