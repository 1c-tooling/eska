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

impl ProjectSession {
    /// Resolve a design-time value by its globally named item within one predefined payload.
    ///
    /// # Errors
    /// Rejects missing or ambiguous names and uses the usual bounded parser and containment checks.
    pub fn predefined_reference(
        &mut self,
        owner: &ObjectId,
        name: &str,
    ) -> Result<ObjectSummary, WorkspaceError> {
        use crate::project::metadata_model::CollectionKind;
        self.children(&NodeId::Object(owner.clone()), TreeOptions::default())?;
        let collection = NodeId::Collection {
            owner: owner.clone(),
            kind: CollectionKind::Metadata(MetadataKind::PredefinedItem),
        };
        self.children(&collection, TreeOptions::default())?;
        let parsed = self.load_predefined(owner, PropertiesMode::Summary)?;
        let mut found = parsed
            .objects
            .iter()
            .filter(|item| item.metadata.name() == name);
        let item = found.next().ok_or_else(|| {
            WorkspaceError::UnknownObject(ObjectId::from_parts(
                Some(owner),
                MetadataKind::PredefinedItem.as_str(),
                name,
            ))
        })?;
        if found.next().is_some() {
            return Err(WorkspaceError::BrokenAncestry(collection));
        }
        Ok(self.object(item.metadata.id())?.clone())
    }
}
