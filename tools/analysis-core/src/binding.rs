//! Compose kernel identities from declared metadata, never from a directory scan.

use bsl_metadata::MdoType;
use eska::project_core::{MetadataKind, source_format::SourcePath};
use ide_db::metadata::{CommonModuleEntry, DefinedTypeEntry, MdoEntry, MetadataListingData};
use vfs::FileId;

/// Every owner gets a stable slot; unsupported families keep an explicit unavailable value.
#[derive(Clone, Debug)]
pub struct OwnerBinding {
    pub kind: MetadataKind,
    pub name: Option<String>,
    pub main: FileId,
    pub native_path: SourcePath,
}

impl OwnerBinding {
    /// Register supported resolver families without activating unrelated files.
    pub fn add_to_listing(&self, listing: &mut MetadataListingData) {
        let Some(name) = &self.name else {
            return;
        };
        match self.kind {
            MetadataKind::CommonModule => listing.common_modules.push(CommonModuleEntry {
                name: name.clone(),
                main: self.main,
                module_file: None,
                unread_module_file: None,
            }),
            MetadataKind::DefinedType => listing.defined_types.push(DefinedTypeEntry {
                name: name.clone(),
                main: self.main,
            }),
            _ => {
                if let Ok(kind) = self.kind.designer_tag().parse::<MdoType>()
                    && MdoType::all().contains(&kind)
                {
                    listing.entries.push(MdoEntry {
                        kind,
                        name: name.clone(),
                        main: self.main,
                        // Predefined resources are outside the current adapter's coverage.
                        predefined: None,
                    });
                }
            }
        }
    }
}
