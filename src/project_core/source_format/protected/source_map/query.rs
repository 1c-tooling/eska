//! Query origins join selected plain bytes with their actual native scopes, without granting permissions.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    COMPOSITION_SOURCE_MAP_VERSION, CREATED_SOURCE_MAP_VERSION, EXTERNAL_SOURCE_MAP_VERSION,
    FileOrigin, InputAuthority, MappedFile, OriginTexts, QUERY_SOURCE_MAP_VERSION, QueryOrigin,
    SourceLocation, SourceMap, location::verify_bytes, payload_origin, require_digest,
};
use crate::project_core::{
    MetadataKind,
    source_format::{
        AcceptedBase, Error, FileEntry, NativeQueryDocument, QueryReferenceDocument, QuerySlot,
        SourcePath, export::Package, files::Inventory, layout::queries::native_path,
        protected::Origin,
    },
};

/// The generated lexical ranges and bindings are checked against the exact selected plain inputs.
pub(super) fn origins(
    working: &Inventory,
    base: &AcceptedBase,
    package: &Package,
    entry: &FileEntry,
    xml: &BTreeMap<SourcePath, String>,
    authorities: &BTreeMap<SourcePath, Origin>,
) -> Result<Vec<QueryOrigin>, Error> {
    let Some(resource) = package
        .manifest
        .query_resources
        .iter()
        .find(|resource| resource.path == entry.path)
    else {
        return Ok(Vec::new());
    };
    if resource.sources.is_empty() {
        return Ok(Vec::new());
    }
    let native = NativeQueryDocument::from_xml(xml.get(&entry.path).ok_or_else(invalid)?)?;
    let form_xml;
    let references = if let Some(document) = package.query_inputs.resources.get(&entry.path) {
        document
    } else {
        form_xml = QueryReferenceDocument::from_xml(
            &package
                .forms
                .get(&entry.path)
                .ok_or_else(invalid)?
                .to_xml()?,
        )?;
        &form_xml
    };
    if native.queries().len() != references.references().len() {
        return Err(invalid());
    }
    native
        .queries()
        .iter()
        .zip(references.references())
        .map(|(query, reference)| {
            if query.slot != reference.slot {
                return Err(invalid());
            }
            let (authority, snapshot) = selected(working, base, authorities, &reference.source)?;
            let input = snapshot.files.get(&reference.source).ok_or_else(invalid)?;
            verify_bytes(query.text.as_bytes(), input.bytes, &input.sha256)?;
            Ok(QueryOrigin {
                slot: query.slot.clone(),
                native: query.native_range(),
                authority,
                input: input.into(),
            })
        })
        .collect()
}

/// A composition XML input supplies the gaps, while each query uses its own selected authority.
pub(super) fn resource_origin(
    working: &Inventory,
    base: &AcceptedBase,
    package: &Package,
    entry: &FileEntry,
    xml: &BTreeMap<SourcePath, String>,
    authorities: &BTreeMap<SourcePath, Origin>,
) -> Result<FileOrigin, Error> {
    let FileOrigin::Payload {
        authority, input, ..
    } = payload_origin(base, working, authorities, entry)?
    else {
        return Err(invalid());
    };
    let resource = package
        .manifest
        .query_resources
        .iter()
        .find(|resource| resource.path == entry.path)
        .ok_or_else(invalid)?;
    let uuid = base
        .package
        .documents
        .get(&resource.descriptor)
        .ok_or_else(invalid)?
        .descriptor
        .object
        .uuid
        .clone();
    Ok(FileOrigin::QueryResource {
        uuid,
        descriptor: resource.descriptor.clone(),
        authority,
        input,
        queries: origins(working, base, package, entry, xml, authorities)?,
    })
}

/// The captured physical selection, rather than filenames or current bytes, supplies authority.
fn selected<'a>(
    working: &'a Inventory,
    base: &'a AcceptedBase,
    authorities: &BTreeMap<SourcePath, Origin>,
    path: &SourcePath,
) -> Result<(InputAuthority, &'a Inventory), Error> {
    match authorities.get(path) {
        Some(Origin::Working) => Ok((InputAuthority::Working, working)),
        Some(Origin::Accepted) => Ok((InputAuthority::Accepted, &base.package.snapshot)),
        None => Err(invalid()),
    }
}

/// Plain coordinates derive from decoded native values whose checksums match the captured selected inputs.
pub(super) fn plain_location(
    file: &MappedFile,
    native: &[u8],
    range: &std::ops::Range<usize>,
) -> Result<Option<SourceLocation>, Error> {
    let (FileOrigin::ManagedForm { uuid, queries, .. }
    | FileOrigin::CreatedManagedForm { uuid, queries, .. }
    | FileOrigin::QueryResource { uuid, queries, .. }) = &file.origin
    else {
        return Ok(None);
    };
    if queries.is_empty() {
        return Ok(None);
    }
    let text = std::str::from_utf8(native).map_err(|_| invalid())?;
    let document = NativeQueryDocument::from_xml(text)?;
    if queries.len() != document.queries().len() {
        return Err(invalid());
    }
    let mut location = None;
    for (query, origin) in document.queries().iter().zip(queries) {
        if query.slot != origin.slot || query.native_range() != origin.native {
            return Err(invalid());
        }
        verify_bytes(
            query.text.as_bytes(),
            origin.input.bytes,
            &origin.input.sha256,
        )?;
        if origin.native.contains(&range.start) {
            location = Some(SourceLocation {
                authority: origin.authority,
                input: origin.input.clone(),
                range: query.decoded_range(text, range)?,
                uuid: Some(uuid.clone()),
            });
        }
    }
    Ok(location)
}

/// Composition gaps retain exact source XML offsets after the query replacements are accounted for.
pub(super) fn resource_location(
    file: &MappedFile,
    native: &[u8],
    range: &std::ops::Range<usize>,
    texts: Option<OriginTexts<'_>>,
) -> Result<SourceLocation, Error> {
    let FileOrigin::QueryResource {
        uuid,
        authority,
        input,
        queries,
        ..
    } = &file.origin
    else {
        return Err(invalid());
    };
    let mut location = SourceLocation {
        uuid: Some(uuid.clone()),
        authority: *authority,
        input: input.clone(),
        range: None,
    };
    if let Some(texts) = texts {
        let source = match authority {
            InputAuthority::Accepted => texts.accepted,
            InputAuthority::Working => texts.working.ok_or_else(invalid)?,
        };
        verify_bytes(source.as_bytes(), input.bytes, &input.sha256)?;
        let native = std::str::from_utf8(native).map_err(|_| invalid())?;
        let reference = QueryReferenceDocument::from_xml(source)?;
        let parsed = NativeQueryDocument::from_xml(native)?;
        if reference.references().len() != queries.len() || parsed.queries().len() != queries.len()
        {
            return Err(invalid());
        }
        let values = reference
            .references()
            .iter()
            .zip(parsed.queries())
            .map(|(reference, query)| {
                if reference.slot != query.slot {
                    return Err(invalid());
                }
                Ok((reference.source.clone(), query.text.clone()))
            })
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        if reference.materialize(&values)? != native {
            return Err(invalid());
        }
        let (_, mapped) = crate::project_core::source_format::queries::restore_native_queries(
            source, native, range,
        )?;
        location.range = Some(mapped);
    }
    Ok(location)
}

/// Serialized maps retain one resource owner and globally unique portable plain input paths.
pub(super) fn validate_map(map: &SourceMap) -> Result<(), Error> {
    let mut paths = BTreeSet::new();
    for file in &map.files {
        let (uuid, descriptor, queries, kind) = match &file.origin {
            FileOrigin::ManagedForm {
                uuid,
                descriptor,
                queries,
                ..
            } => (uuid, descriptor, queries, MetadataKind::Form),
            FileOrigin::QueryResource {
                uuid,
                descriptor,
                queries,
                ..
            } => {
                if !matches!(
                    map.schema_version,
                    QUERY_SOURCE_MAP_VERSION
                        | EXTERNAL_SOURCE_MAP_VERSION
                        | COMPOSITION_SOURCE_MAP_VERSION
                        | CREATED_SOURCE_MAP_VERSION
                ) || !native_path(descriptor, MetadataKind::Template)?
                    .as_str()
                    .eq_ignore_ascii_case(file.path.as_str())
                    || !matches!(map.file(descriptor).map(|file| &file.origin),
                        Some(FileOrigin::Metadata { uuid: owner, .. }) if uuid == owner)
                {
                    return Err(invalid());
                }
                (uuid, descriptor, queries, MetadataKind::Template)
            }
            _ => continue,
        };
        if !queries.is_empty()
            && !matches!(
                map.schema_version,
                QUERY_SOURCE_MAP_VERSION
                    | EXTERNAL_SOURCE_MAP_VERSION
                    | COMPOSITION_SOURCE_MAP_VERSION
                    | CREATED_SOURCE_MAP_VERSION
            )
        {
            return Err(invalid());
        }
        let native = descriptor.as_str();
        let source = map
            .external_root
            .as_ref()
            .map_or(native, |root| root.relative(native));
        let authoring = crate::project_core::source_format::root::descriptor_path(
            map.external_root.as_ref(),
            kind,
            descriptor,
        )?;
        if !matches!(map.file(descriptor).map(|file| &file.origin), Some(FileOrigin::Metadata { accepted, working, .. })
            if (map.schema_version >= COMPOSITION_SOURCE_MAP_VERSION && working.as_ref().is_some_and(|input| input.path == authoring))
                || accepted.path == authoring)
        {
            return Err(invalid());
        }
        if let FileOrigin::QueryResource { input, .. } = &file.origin {
            let (_, tail) = file
                .path
                .as_str()
                .rsplit_once("/Ext/")
                .ok_or_else(invalid)?;
            let expected = format!(
                "{}/{tail}",
                source.strip_suffix(".xml").ok_or_else(invalid)?
            );
            if input.path.as_str() != expected {
                return Err(invalid());
            }
        }
        let prefix = format!(
            "{}/Queries/",
            source.strip_suffix(".xml").ok_or_else(invalid)?
        );
        for query in queries {
            if !query.input.path.as_str().starts_with(&prefix)
                || !paths.insert(query.input.path.as_str().to_lowercase())
                || !matches!(map.file(descriptor).map(|file| &file.origin),
                    Some(FileOrigin::Metadata { uuid: owner, .. }) if uuid == owner)
            {
                return Err(invalid());
            }
        }
    }
    Ok(())
}

/// File-level validation also runs for addressed reads that do not have a complete map DTO.
pub(super) fn validate_origin(file: &MappedFile) -> Result<(), Error> {
    let (queries, prefix, form) = match &file.origin {
        FileOrigin::ManagedForm {
            queries,
            accepted,
            working,
            ..
        } => (
            queries,
            working
                .as_ref()
                .unwrap_or(accepted)
                .path
                .as_str()
                .strip_suffix("/form.yaml")
                .ok_or_else(invalid)?,
            true,
        ),
        FileOrigin::QueryResource {
            queries,
            descriptor,
            input,
            ..
        } => {
            let prefix = input
                .path
                .as_str()
                .rsplit_once('/')
                .map(|(parent, _)| parent)
                .ok_or_else(invalid)?;
            if !native_path(descriptor, MetadataKind::Template)?
                .as_str()
                .eq_ignore_ascii_case(file.path.as_str())
            {
                return Err(invalid());
            }
            (queries, prefix, false)
        }
        _ => return Ok(()),
    };
    let prefix = format!("{prefix}/Queries/");
    let mut end = 0;
    let mut slots = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for query in queries {
        require_digest(&query.input.sha256)?;
        if query.native.start < end
            || query.native.start >= query.native.end
            || query.native.end as u64 > file.bytes
            || !slots.insert(&query.slot)
            || !paths.insert(query.input.path.as_str().to_lowercase())
            || !query.input.path.as_str().starts_with(&prefix)
            || !query
                .input
                .path
                .as_path()
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("sdbl"))
            || form != matches!(query.slot, QuerySlot::DynamicList { .. })
        {
            return Err(invalid());
        }
        if let QuerySlot::Composition { datasets } = &query.slot
            && (datasets.is_empty() || datasets.iter().any(String::is_empty))
        {
            return Err(invalid());
        }
        end = query.native.end;
    }
    Ok(())
}

/// Source map data cannot substitute a different input, native slot or owner.
fn invalid() -> Error {
    Error::root("source_map_origin_invalid")
}
