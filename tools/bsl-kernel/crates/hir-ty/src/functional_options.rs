//! Native functional-option calls refine existing platform candidates using current storage types.

use bsl_metadata::{FunctionalOptionLocation, MdoType};
use bsl_types::{builders::Builders, kind::TypeId};
use vfs::FileId;

/// Only the real platform global receives this refinement; call ownership is decided by native inference.
pub fn is_get_name(name: &str) -> bool {
    crate::platform_type_name::name_eq_ci(
        name,
        "ПолучитьФункциональнуюОпцию",
        "GetFunctionalOption",
    )
}

/// A declared option's result follows its current storage, while unprovided values stay unknown.
pub(crate) fn result_type(
    db: &dyn crate::db::HirDatabase,
    file: FileId,
    name: &str,
) -> Option<TypeId> {
    let option = db.resolve_functional_option(file, name)?;
    let resolver = crate::object_resolver::DbObjectResolver::new(db, file);
    let location = option.location()?;
    let storage_type = match location {
        FunctionalOptionLocation::Empty => None,
        FunctionalOptionLocation::Constant { name } => db
            .resolve_metadata_object(file, MdoType::Constant, name)
            .and_then(|object| object.constant_type.clone()),
        FunctionalOptionLocation::CatalogAttribute { catalog, attribute } => {
            db.resolve_metadata_object(file, MdoType::Catalog, catalog).and_then(|object| {
                object.find_attribute(attribute).map(|field| field.attr_type.clone())
            })
        }
        FunctionalOptionLocation::InformationRegisterResource { register, resource } => {
            db.resolve_register(file, MdoType::InformationRegister, register).and_then(|register| {
                register
                    .resources()
                    .iter()
                    .find(|field| stdx::case::eq_ignore_case(field.name(), resource))
                    .and_then(|field| field.attr_type().cloned())
            })
        }
    };
    Some(storage_type.as_ref().map_or_else(
        || db.unknown(),
        |ty| {
            let value = crate::field_enum::attribute_type_to_typeid(db, ty, &resolver);
            // A group parameter may address an attribute unavailable for groups; its value is Null.
            // This profile does not prove attribute usage or the runtime parameter's element kind.
            if matches!(location, FunctionalOptionLocation::CatalogAttribute { .. }) {
                db.union(vec![value, db.null()])
            } else {
                value
            }
        },
    ))
}

/// Existing platform IDs identify discovery and presentation without relying on a same-spelled user function.
pub fn platform_id() -> Option<u32> {
    bsl_platform::PlatformData::instance()
        .get_global_function("GetFunctionalOption")
        .map(|function| function.id)
}

/// Native literal values already have BSL quoting and line-continuation semantics applied by lowering.
pub(crate) fn literal_name<'a>(
    body: &'a hir_def::body::Body,
    args: &[hir_def::ExprId],
) -> Option<&'a str> {
    let hir_def::hir::Expr::Literal(hir_def::hir::Literal::String(value)) =
        body.expr(*args.first()?)
    else {
        return None;
    };
    (!value.is_empty()).then_some(value.as_str())
}
