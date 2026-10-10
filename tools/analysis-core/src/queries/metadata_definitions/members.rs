//! Typed field origins select scoped current metadata children; platform properties have no YAML target.

use bsl_metadata::MdoType;
use bsl_types::kind::{ConfigId, MetadataKind as KernelKind};
use eska::{
    analysis_core::InputState,
    project_core::{
        MetadataKind,
        source_format::{NativeUuid, SourcePath},
    },
};
use hir::{HirFieldOrigin, Name, Type, TypeId, TypeKind};

use crate::{AnalysisSession, QueryError, SessionRevision};

use super::MetadataSymbol;

/// An enum manager's value or a real metadata field is distinct from platform/structural members.
pub(super) fn symbol(
    analysis: &ide::Analysis,
    file: vfs::FileId,
    receiver: TypeId,
    name: &str,
) -> Option<MetadataSymbol> {
    let receiver =
        hir::coerce_to_metadata_ref_id(analysis.database(), receiver).unwrap_or(receiver);
    let ty = Type::from_id(analysis.database(), file, receiver);
    let needle = Name::new(name);
    if let TypeKind::ObjectManager(facet) = ty.kind() {
        if facet.mdo != MdoType::Enum || facet.config_id != ConfigId::Root || !ty.has_field(&needle)
        {
            return None;
        }
        return Some(MetadataSymbol {
            kind: facet.mdo,
            name: facet.name.clone(),
            children: vec![(MetadataKind::EnumValue, name.to_owned())],
        });
    }
    let mut owner = owner(&ty)?;
    let field = ty.fields().into_iter().find(|field| {
        field.name.eq_ignore_case(&needle) || field.english_name.eq_ignore_case(&needle)
    })?;
    let kind = match field.origin {
        HirFieldOrigin::UserAttribute | HirFieldOrigin::TabularSectionRowColumn => {
            MetadataKind::Attribute
        }
        HirFieldOrigin::TabularSection => MetadataKind::TabularSection,
        _ => return None,
    };
    owner.children.push((kind, field.name.to_string()));
    Some(owner)
}

/// Hierarchy comes from the kernel's metadata facet; row names qualify their real tabular section.
fn owner(ty: &Type<'_, ide_db::RootDatabaseImpl>) -> Option<MetadataSymbol> {
    let (kind, name, config_id) = match ty.kind() {
        TypeKind::MetadataRef(facet) => (facet.kind, &facet.name, &facet.config_id),
        TypeKind::MetadataObject(facet) => (facet.kind, &facet.name, &facet.config_id),
        _ => return None,
    };
    if config_id != &ConfigId::Root {
        return None;
    }
    if let KernelKind::TabularSectionRow { parent } = kind {
        let (object, table) = name.split_once('.')?;
        if table.contains('.') {
            return None;
        }
        return Some(MetadataSymbol {
            kind: parent,
            name: object.into(),
            children: vec![(MetadataKind::TabularSection, table.into())],
        });
    }
    let manager = ty.manager()?;
    let TypeKind::ObjectManager(facet) = manager.kind() else {
        return None;
    };
    Some(MetadataSymbol {
        kind: facet.mdo,
        name: facet.name.clone(),
        children: Vec::new(),
    })
}

/// Follow exact parent IDs inside the main current document; ambiguity never chooses a first match.
pub(super) fn selected_uuid(
    session: &AnalysisSession,
    path: &SourcePath,
    expected: SessionRevision,
    root: &NativeUuid,
    children: &[(MetadataKind, String)],
) -> Result<Option<NativeUuid>, QueryError> {
    if children.is_empty() {
        return Ok(Some(root.clone()));
    }
    let view = session
        .metadata(path, expected)
        .map_err(QueryError::Request)?;
    let InputState::Ready(snapshot) = view.input else {
        return Ok(None);
    };
    let native = session
        .inputs
        .source_reader()
        .declared_owner(path)
        .map_err(|_| QueryError::UnsupportedNavigationTarget)?
        .native_path;
    let document = snapshot
        .documents
        .iter()
        .find(|document| &document.path == native)
        .ok_or(QueryError::UnsupportedNavigationTarget)?;
    let root_object = document
        .descriptor
        .objects
        .iter()
        .find(|object| object.metadata.uuid().eq_ignore_ascii_case(root.as_str()))
        .ok_or(QueryError::UnsupportedNavigationTarget)?;
    let mut selected = &root_object.metadata;
    for (kind, name) in children {
        let mut candidates = document.descriptor.objects.iter().filter(|object| {
            object.metadata.parent() == Some(selected.id())
                && object.metadata.kind() == *kind
                && folded_eq(object.metadata.name(), name)
        });
        let Some(next) = candidates.next() else {
            return Ok(None);
        };
        if candidates.next().is_some() {
            return Err(QueryError::UnsupportedNavigationTarget);
        }
        selected = &next.metadata;
    }
    NativeUuid::parse(selected.uuid())
        .map(Some)
        .map_err(|_| QueryError::UnsupportedNavigationTarget)
}

/// Match Unicode metadata names with the kernel's per-character lowercase convention.
fn folded_eq(left: &str, right: &str) -> bool {
    left.chars()
        .flat_map(char::to_lowercase)
        .eq(right.chars().flat_map(char::to_lowercase))
}
