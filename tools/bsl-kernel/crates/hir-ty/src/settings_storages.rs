//! Configured settings storages retain the native manager type and callable identities.

use crate::{
    field_enum::{FieldInfo, FieldOrigin},
    object_resolver::MetadataResolution,
};
use bsl_types::{
    builders::Builders,
    intern::TypeKernelDb,
    kind::{TypeId, TypeKind},
};
use hir_def::Name;

/// The pinned platform catalog stores this generated owner with its escaped placeholder.
pub const PROTOTYPE: &str = "SettingsStorageManager.&lt;Storage name&gt;";

/// Native interning canonicalizes the generic owner to the catalog's Russian spelling.
pub fn is_prototype_name(name: &str) -> bool {
    bsl_platform::PlatformData::instance().get_type(PROTOTYPE).is_some_and(|prototype| {
        crate::platform_type_name::name_eq_ci(name, &prototype.name, &prototype.english_name)
    })
}

/// Global lookup and user shadowing decide whether a spelling denotes this real container.
pub fn is_container_name(name: &str) -> bool {
    crate::platform_type_name::name_eq_ci(
        name,
        "ХранилищаНастроекМенеджер",
        "SettingsStoragesManager",
    )
}

/// A configured manager keeps its canonical storage name without inventing a metadata-object kind.
pub fn type_name(name: &str) -> String {
    format!("SettingsStorageManager.{name}")
}

/// Generic placeholders and nested paths cannot identify a declared configured storage.
pub fn manager_name(name: &str) -> Option<&str> {
    let (prefix, name) = name.split_once('.')?;
    if name.is_empty() || name.contains(['.', '<', '>', '&', ';', ' ']) {
        return None;
    }
    crate::platform_type_name::name_eq_ci(
        prefix,
        "ХранилищеНастроекМенеджер",
        "SettingsStorageManager",
    )
    .then_some(name)
}

/// Configured namespace fields retain the actual native execution environments.
pub fn environment() -> hir_def::execution_env::EnvFlags {
    hir_def::execution_env::EnvFlags::from_platform_context(
        bsl_platform::PlatformData::instance()
            .get_type("SettingsStoragesManager")
            .and_then(|value| value.context.as_ref()),
    )
}

/// Only a current resolved descriptor grants a configured manager field.
fn manager_field(
    db: &dyn TypeKernelDb,
    resolver: &dyn MetadataResolution,
    name: &str,
) -> Option<FieldInfo> {
    let storage = resolver.resolve_settings_storage(name)?;
    Some(FieldInfo {
        name: Name::new(storage.name()),
        name_en: None,
        ty: db.platform_object(type_name(storage.name())),
        value_ty: None,
        is_readonly: true,
        origin: FieldOrigin::PlatformProperty { env: environment() },
    })
}

/// The manager has no native properties; configuration descriptors are not runtime attributes.
pub(crate) fn fields(
    db: &dyn TypeKernelDb,
    resolver: &dyn MetadataResolution,
    name: &str,
) -> Option<Vec<FieldInfo>> {
    if is_container_name(name) {
        return Some(
            resolver
                .settings_storage_names()
                .into_iter()
                .filter_map(|name| manager_field(db, resolver, &name))
                .collect(),
        );
    }
    manager_name(name).map(|_| Vec::new())
}

/// Named lookup and completion use the same current namespace fields.
pub(crate) fn field(
    db: &dyn TypeKernelDb,
    resolver: &dyn MetadataResolution,
    receiver: &str,
    name: &Name,
) -> Option<FieldInfo> {
    fields(db, resolver, receiver)?
        .into_iter()
        .find(|field| stdx::case::eq_ignore_case(field.name.as_str(), name.as_str()))
}

/// Native ids, parameters, arity and execution contexts survive the catalog's malformed placeholder names.
pub fn prototype_methods() -> Vec<bsl_platform::PlatformMethod> {
    crate::generated_platform::methods(PROTOTYPE)
}

/// The native arbitrary return contract stays intact; settings keys never imply a configured value schema.
pub(crate) fn method(
    db: &dyn TypeKernelDb,
    receiver: TypeId,
    name: &Name,
) -> Option<crate::method_lookup::MethodInfo> {
    let TypeKind::PlatformObject(facet) = db.lookup_type(receiver) else {
        return None;
    };
    if !is_prototype_name(&facet.name) {
        manager_name(&facet.name)?;
    }
    prototype_methods()
        .into_iter()
        .find(|method| {
            stdx::case::eq_ignore_case(&method.name, name.as_str())
                || stdx::case::eq_ignore_case(&method.english_name, name.as_str())
        })
        .map(|method| crate::method_lookup::to_method_info(db, &method))
}

/// Only a current configured owner closes a manager's native and enrolled user-method surface.
pub(crate) fn current_receiver_name(
    db: &dyn crate::db::HirDatabase,
    file: vfs::FileId,
    receiver: TypeId,
) -> Option<Name> {
    let TypeKind::PlatformObject(facet) = db.lookup_type(receiver) else {
        return None;
    };
    let storage = db.resolve_settings_storage(file, manager_name(&facet.name)?)?;
    Some(Name::new(&type_name(storage.name())))
}
