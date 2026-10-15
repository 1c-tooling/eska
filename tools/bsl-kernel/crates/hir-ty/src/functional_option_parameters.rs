//! The real interface getter exposes possible configured parameter keys without claiming runtime presence.

use bsl_metadata::{FunctionalOptionParameterUse, MdoType};
use bsl_types::{
    builders::Builders,
    kind::{
        Projection, ProjectionField, ProjectionFieldSource, ProjectionOrigin, TypeId, TypeKind,
    },
};
use std::sync::Arc;
use vfs::FileId;

/// Only this native global owns the configured interface-parameter result.
pub fn is_get_name(name: &str) -> bool {
    crate::platform_type_name::name_eq_ci(
        name,
        "ПолучитьПараметрыФункциональныхОпцийИнтерфейса",
        "GetInterfaceFunctionalOptionParameters",
    )
}

/// Discovery and presentation use the real platform callable's identity.
pub fn platform_id() -> Option<u32> {
    bsl_platform::PlatformData::instance()
        .get_global_function("GetInterfaceFunctionalOptionParameters")
        .map(|function| function.id)
}

/// The getter's structure remains open: configured parameters need not be set in the current interface.
pub(crate) fn result_type(db: &dyn crate::db::HirDatabase, file: FileId) -> TypeId {
    let parameters = db.resolve_functional_option_parameters(file);
    let fields = parameters
        .iter()
        .map(|parameter| {
            let types = parameter
                .uses()
                .unwrap_or_default()
                .iter()
                .map(|target| target_type(db, file, target))
                .collect::<Vec<_>>();
            let ty = if types.is_empty() { db.unknown() } else { db.union(types) };
            ProjectionField::new(parameter.name().into(), ty, ProjectionFieldSource::Unknown)
        })
        .collect::<Vec<_>>();
    let keys = fields.iter().map(|field| field.name.clone()).collect::<Vec<_>>();
    // Start from the public native builder so future facet fields keep their native defaults.
    let TypeKind::Structure(mut facet) = db.lookup_type(db.structure(None)).clone() else {
        return db.structure(None);
    };
    facet.keys = Some(Arc::from(keys));
    facet.fields =
        Some(Arc::new(Projection::new(Arc::from(fields), ProjectionOrigin::Unknown, None)));
    db.intern_type(TypeKind::Structure(facet))
}

/// One Use target supplies a possible value type from its current canonical owner.
fn target_type(
    db: &dyn crate::db::HirDatabase,
    file: FileId,
    target: &FunctionalOptionParameterUse,
) -> TypeId {
    match target {
        FunctionalOptionParameterUse::Catalog { name } => {
            db.resolve_metadata_object(file, MdoType::Catalog, name).map_or_else(
                || db.unknown(),
                |_| {
                    db.metadata_ref(
                        hir_def::ty::MetadataKind::CatalogRef,
                        name.clone(),
                        &bsl_types::testing::RootConfigCtx,
                    )
                },
            )
        }
        FunctionalOptionParameterUse::InformationRegisterDimension { register, dimension } => {
            let ty = db.resolve_register(file, MdoType::InformationRegister, register).and_then(
                |register| {
                    register
                        .dimensions()
                        .iter()
                        .find(|field| stdx::case::eq_ignore_case(field.name(), dimension))
                        .and_then(|field| field.attr_type().cloned())
                },
            );
            let resolver = crate::object_resolver::DbObjectResolver::new(db, file);
            ty.as_ref().map_or_else(
                || db.unknown(),
                |ty| crate::field_enum::attribute_type_to_typeid(db, ty, &resolver),
            )
        }
    }
}
