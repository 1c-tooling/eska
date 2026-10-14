//! Metadata UUIDs and native member slots disambiguate rename from unrelated path reuse.

use std::collections::{BTreeMap, BTreeSet};

use super::FileRole;
use super::{Error, NativeUuid, SourcePath, export::Package, files, protected};

/// Include declared optional modules even when their accepted physical file is absent.
pub(super) fn files(package: &Package) -> Result<BTreeMap<String, SourcePath>, Error> {
    let mut result = BTreeMap::new();
    let owners = package
        .documents
        .values()
        .map(|document| (document.descriptor.object.uuid.key(), document))
        .collect::<BTreeMap<_, _>>();
    let source_paths = package
        .manifest
        .files
        .iter()
        .map(|file| (&file.path, file.source_path()))
        .collect::<BTreeMap<_, _>>();
    for (source, paths) in &package.owners {
        let document = paths
            .first()
            .and_then(|path| package.documents.get(path))
            .ok_or_else(|| Error::root("source_descriptor_missing"))?;
        insert(
            &mut result,
            format!("owner:{}:source", document.descriptor.object.uuid.key()),
            source,
        )?;
    }
    for resource in &package.manifest.query_resources {
        let uuid = &package
            .documents
            .get(&resource.descriptor)
            .ok_or_else(|| Error::root("source_descriptor_missing"))?
            .descriptor
            .object
            .uuid;
        let owner = source_paths
            .get(&resource.descriptor)
            .ok_or_else(|| Error::root("source_descriptor_missing"))?;
        let folder = owner
            .as_str()
            .rsplit_once('/')
            .map_or("", |(parent, _)| parent);
        for source in &resource.sources {
            let slot = source
                .as_str()
                .strip_prefix(folder)
                .and_then(|tail| tail.strip_prefix('/'))
                .unwrap_or(source.as_str());
            insert(&mut result, format!("query:{}:{slot}", uuid.key()), source)?;
        }
    }
    for file in &package.manifest.files {
        if matches!(
            file.role,
            FileRole::Descriptor { .. } | FileRole::ManagedForm { .. }
        ) {
            continue;
        }
        let source = file.source_path();
        let key = if authority_path(&file.path) {
            format!("authority:{}", file.path.as_str())
        } else if let Some(uuid) = protected::native_package_file_owner(package, &file.path) {
            let owner = owners.get(&uuid.key());
            let slot = owner
                .and_then(|document| document.path.as_str().strip_suffix(".xml"))
                .and_then(|prefix| file.path.as_str().strip_prefix(prefix))
                .and_then(|tail| tail.strip_prefix('/'))
                .unwrap_or(file.path.as_str());
            format!("payload:{}:{slot}", uuid.key())
        } else {
            format!("path:{}", file.path.as_str())
        };
        insert(&mut result, key, source)?;
    }
    let declared = result.values().collect::<BTreeSet<_>>();
    for path in package.snapshot.files.keys() {
        if path.as_str() != files::MANIFEST && !declared.contains(path) {
            return Err(Error::root("source_file_unmapped").in_field(path.as_str()));
        }
    }
    Ok(result)
}

/// One identity cannot choose two physical source files, even when their bytes agree.
fn insert(
    files: &mut BTreeMap<String, SourcePath>,
    key: String,
    path: &SourcePath,
) -> Result<(), Error> {
    if files.insert(key, path.clone()).is_some() {
        return Err(Error::root("source_update_identity_ambiguous").in_field(path.as_str()));
    }
    Ok(())
}

/// Supplier rules and original CF payloads are accepted authority, not ordinary local source.
pub(super) fn authority_path(path: &SourcePath) -> bool {
    path.as_str() == "Ext/ParentConfigurations.bin"
        || path.as_str().starts_with("Ext/ParentConfigurations/")
}

/// Unowned paths stay explicit instead of acquiring a guessed metadata UUID.
pub(super) fn uuid(key: &str) -> Option<NativeUuid> {
    let (kind, rest) = key.split_once(':')?;
    matches!(kind, "owner" | "payload" | "query")
        .then(|| rest.split(':').next())
        .flatten()
        .and_then(|uuid| NativeUuid::parse(uuid).ok())
}
