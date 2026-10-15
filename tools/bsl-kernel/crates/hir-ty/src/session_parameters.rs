//! Configured members extend the real platform container through the same typed resolver.

use crate::{field_enum::FieldInfo, object_resolver::MetadataResolution};
use bsl_types::intern::TypeKernelDb;
use hir_def::Name;

/// Recognize only the platform's bilingual container name; user symbols are resolved before this.
pub fn is_session_parameters_type_name(name: &str) -> bool {
    crate::platform_type_name::name_eq_ci(name, "ПараметрыСеанса", "SessionParameters")
}

/// Configured fields retain the actual platform container's execution contexts.
pub fn environment() -> hir_def::execution_env::EnvFlags {
    let context = bsl_platform::PlatformData::instance()
        .get_type("SessionParameters")
        .and_then(|value| value.context.as_ref());
    hir_def::execution_env::EnvFlags::from_platform_context(context)
}

/// Resolve one current configured field; a refused slot must not leak an older base value.
pub(crate) fn field(
    db: &dyn TypeKernelDb,
    resolver: &dyn MetadataResolution,
    name: &str,
) -> Option<FieldInfo> {
    let parameter = resolver.resolve_session_parameter(name)?;
    Some(FieldInfo {
        name: Name::new(parameter.name()),
        name_en: None,
        ty: crate::field_enum::attribute_type_to_typeid(db, parameter.attribute_type(), resolver),
        value_ty: None,
        is_readonly: false,
        origin: crate::field_enum::FieldOrigin::PlatformProperty { env: environment() },
    })
}

/// Enumerating completion members reads only this declared family, not every metadata model.
pub(crate) fn fields(db: &dyn TypeKernelDb, resolver: &dyn MetadataResolution) -> Vec<FieldInfo> {
    resolver
        .session_parameter_names()
        .into_iter()
        .filter_map(|name| field(db, resolver, &name))
        .collect()
}
