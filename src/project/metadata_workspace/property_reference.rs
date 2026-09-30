//! Resolve explicit metadata paths against declared objects, without a workspace scan.

use super::{ObjectSummary, ProjectSession, WorkspaceError};
use crate::project::{
    configurator::TreeOptions,
    metadata_model::{MetadataKind, NodeId, ObjectId},
    metadata_parser::{LoadError, PropertiesMode},
};

impl ProjectSession {
    /// Load only the declared ancestors and target of an explicit Designer reference.
    ///
    /// # Errors
    /// Rejects undeclared objects and missing or malformed target descriptors.
    pub fn property_reference(
        &mut self,
        parts: &[(MetadataKind, &str)],
    ) -> Result<ObjectSummary, WorkspaceError> {
        let mut parent: Option<ObjectId> = None;
        for &(kind, name) in parts {
            if let Some(id) = &parent {
                self.children(&NodeId::Object(id.clone()), TreeOptions::default())?;
            }
            let id = ObjectId::from_parts(parent.as_ref(), kind.as_str(), name);
            self.object(&id)?;
            parent = Some(id);
        }
        let id = parent.ok_or_else(|| WorkspaceError::UnknownNode(self.root().clone()))?;
        let parsed = self.load(&id, PropertiesMode::Summary)?;
        let object = parsed
            .objects
            .iter()
            .find(|object| object.metadata.id() == &id)
            .ok_or_else(|| WorkspaceError::Load(LoadError::ObjectNotFound(id.clone())))?;
        let mut summary = self.object(&id)?.clone();
        summary.synonyms.clone_from(&object.synonyms);
        summary.uuid = Some(object.metadata.uuid().to_owned());
        Ok(summary)
    }
}
