//! Integration channels retain their declared order and receive-message handler spelling.

#[cfg(test)]
mod tests;

use bsl_metadata::{
    IntegrationService, IntegrationServiceBuilder, IntegrationServiceChannelBuilder,
};
use eska::project_core::{MetadataKind, metadata_parser::ParsedDescriptor};

use crate::{AdapterError, AdapterIssue, identity, object::children, properties::Properties};

/// Project channel bindings without guessing transport settings or reading their module.
///
/// # Errors
/// Returns an incomplete descriptor, wrong kind, invalid identity or malformed channel property.
pub fn integration_service(
    descriptor: &ParsedDescriptor,
) -> Result<IntegrationService, AdapterError> {
    let root = identity::descriptor_root(descriptor)?;
    if root.metadata.kind() != MetadataKind::IntegrationService {
        return Err(AdapterError {
            issue: AdapterIssue::WrongKind {
                actual: root.metadata.kind(),
            },
            range: root.range.clone(),
        });
    }
    let _uuid = identity::uuid(root)?;
    let _fields = Properties::new(root)?;
    let mut builder = IntegrationServiceBuilder::new().name(root.metadata.name());
    for channel in children(descriptor, root)
        .filter(|child| child.metadata.kind() == MetadataKind::IntegrationServiceChannel)
    {
        let _uuid = identity::uuid(channel)?;
        let fields = Properties::new(channel)?;
        builder = builder.add_channel(
            IntegrationServiceChannelBuilder::new()
                .name(channel.metadata.name())
                .receive_message_processing(
                    fields.text("ReceiveMessageProcessing")?.unwrap_or_default(),
                )
                .build(),
        );
    }
    Ok(builder.build())
}
