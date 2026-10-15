//! Metadata resolution for standalone text with explicit owner or configurator scope.
//!
//! SDBL lowering asks for metadata through [`QueryMetadataResolver`], and every existing
//! implementation is anchored to a `FileId` — which root a query may see follows from which
//! file it sits in. A standalone declared query borrows its existing owner's anchor.
//! A query handed to a configurator tool as bare text has no such anchor, so it gets the
//! configurator's view instead: the base configuration plus every extension. External
//! objects stay outside it — SDBL names no external data processor or report, and the
//! roots a tool reports as consulted are exactly these.

use std::sync::Arc;

use bsl_metadata::{
    AttributeType, MdoType, MetadataObject, MetadataResolver, QueryMetadataResolver, Register,
};

use crate::RootDatabaseImpl;

#[cfg(test)]
mod tests;

/// Resolve standalone query metadata through the registered file's current visibility chain.
/// The anchor is an existing owner descriptor; query text needs no synthetic BSL file.
pub struct FileQueryResolver<'a> {
    db: &'a dyn hir::ConfigsDatabase,
    file_id: vfs::FileId,
}

impl std::fmt::Debug for FileQueryResolver<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileQueryResolver").field("file_id", &self.file_id).finish_non_exhaustive()
    }
}

impl<'a> FileQueryResolver<'a> {
    /// The caller supplies a registered current descriptor and admits metadata readiness separately.
    pub fn new(db: &'a dyn hir::ConfigsDatabase, file_id: vfs::FileId) -> Self {
        Self { db, file_id }
    }
}

impl MetadataResolver for FileQueryResolver<'_> {
    fn resolve_defined_type(&self, name: &str) -> Option<AttributeType> {
        self.db.resolve_defined_type(self.file_id, name)
    }
}

impl QueryMetadataResolver for FileQueryResolver<'_> {
    fn resolve_metadata_object(
        &self,
        mdo_type: MdoType,
        name: &str,
    ) -> Option<Arc<MetadataObject>> {
        self.db.resolve_metadata_object(self.file_id, mdo_type, name)
    }

    fn resolve_register(&self, mdo_type: MdoType, name: &str) -> Option<Arc<Register>> {
        self.db.resolve_register(self.file_id, mdo_type, name)
    }
}

/// The whole-configuration view of metadata, for a consumer with no file to anchor
/// visibility to.
///
/// Deliberately wider than any single file's view: an object defined only in an extension is
/// found. That matches the MCP `metadata object` tool, which answers the same
/// "what does this configuration contain" question.
pub struct AcrossRootsQueryResolver<'a> {
    db: &'a RootDatabaseImpl,
}

impl std::fmt::Debug for AcrossRootsQueryResolver<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcrossRootsQueryResolver").finish_non_exhaustive()
    }
}

impl<'a> AcrossRootsQueryResolver<'a> {
    pub fn new(db: &'a RootDatabaseImpl) -> Self {
        Self { db }
    }
}

impl MetadataResolver for AcrossRootsQueryResolver<'_> {
    fn resolve_defined_type(&self, name: &str) -> Option<AttributeType> {
        self.db.resolve_defined_type_across_designer_roots(name)
    }
}

impl QueryMetadataResolver for AcrossRootsQueryResolver<'_> {
    fn resolve_metadata_object(
        &self,
        mdo_type: MdoType,
        name: &str,
    ) -> Option<Arc<MetadataObject>> {
        self.db.resolve_metadata_object_across_designer_roots(mdo_type, name)
    }

    fn resolve_register(&self, mdo_type: MdoType, name: &str) -> Option<Arc<Register>> {
        self.db.resolve_register_across_designer_roots(mdo_type, name)
    }
}
