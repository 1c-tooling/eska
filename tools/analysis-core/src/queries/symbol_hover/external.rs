//! Own external field cards use current native type facets without inventing metadata definitions.

use bsl_metadata::MdoType;
use bsl_types::kind::{ConfigId, MetadataKind};
use hir::{HirFieldOrigin, Name, Semantics, Type, TypeKind};

use crate::{AnalysisScope, AnalysisSession, HoverKind, Locale, QueryError, SymbolHover};

/// The pinned positional card accepts definitions; current external attributes instead expose typed fields.
pub(super) fn current(
    session: &AnalysisSession,
    analysis: &ide::Analysis,
    file: vfs::FileId,
    class: hir::NameClass,
    locale: Locale,
) -> Result<Option<SymbolHover>, QueryError> {
    if session.scope() != AnalysisScope::CanonicalExternal {
        return Ok(None);
    }
    let hir::NameClass::FieldName {
        receiver,
        token,
        is_call: false,
    } = class
    else {
        return Ok(None);
    };
    let receiver = Semantics::new(analysis.database()).type_of_expr(file, &receiver);
    let receiver =
        hir::coerce_to_metadata_ref_id(analysis.database(), receiver).unwrap_or(receiver);
    let receiver = Type::from_id(analysis.database(), file, receiver);
    let Some((kind, name)) = owner(&receiver) else {
        return Ok(None);
    };
    session.require_own_external_target(kind, name)?;
    let needle = Name::new(token.text());
    let Some(field) = receiver.fields().into_iter().find(|field| {
        matches!(
            field.origin,
            HirFieldOrigin::UserAttribute
                | HirFieldOrigin::TabularSection
                | HirFieldOrigin::TabularSectionRowColumn
        ) && (field.name.eq_ignore_case(&needle) || field.english_name.eq_ignore_case(&needle))
    }) else {
        return Ok(None);
    };
    Ok(Some(SymbolHover {
        symbol: token.text().into(),
        kind: HoverKind::Field,
        declaration: None,
        documentation: None,
        return_type: Some(hir::kernel_type_label(
            analysis.database(),
            field.ty,
            locale,
            false,
        )),
        start_byte: token.text_range().start().into(),
        end_byte: token.text_range().end().into(),
    }))
}

/// A managed main attribute preserves its real native underlying owner through the form-data wrapper.
fn owner<'a>(receiver: &'a Type<'_, ide_db::RootDatabaseImpl>) -> Option<(MdoType, &'a str)> {
    let (kind, name, config) = match receiver.kind() {
        TypeKind::FormData {
            underlying: Some(owner),
            ..
        } => return Some((owner.mdo_type, owner.name.as_str())),
        TypeKind::MetadataObject(facet) => (facet.kind, facet.name.as_str(), &facet.config_id),
        TypeKind::MetadataRef(facet) => (facet.kind, facet.name.as_str(), &facet.config_id),
        _ => return None,
    };
    if config != &ConfigId::Root {
        return None;
    }
    match kind {
        MetadataKind::ExternalDataProcessorObject => Some((MdoType::ExternalDataProcessor, name)),
        MetadataKind::ExternalReportObject => Some((MdoType::ExternalReport, name)),
        MetadataKind::TabularSection { parent } | MetadataKind::TabularSectionRow { parent } => {
            Some((parent, name.split_once('.')?.0))
        }
        _ => None,
    }
}
