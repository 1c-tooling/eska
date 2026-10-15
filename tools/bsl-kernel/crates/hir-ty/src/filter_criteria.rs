//! Configured criterion managers use their real generated platform names and current native contents.

use crate::{
    field_enum::{FieldInfo, FieldOrigin},
    object_resolver::MetadataResolution,
};
use bsl_types::{builders::Builders, intern::TypeKernelDb, kind::TypeKind};
use hir_def::Name;

pub const PROTOTYPE: &str = "FilterCriterionManager.&lt;Criterion name&gt;";

/// The actual platform container is resolved after user-name shadowing, just like other configured globals.
pub fn is_container_name(name: &str) -> bool {
    crate::platform_type_name::name_eq_ci(name, "КритерииОтбораМенеджер", "FilterCriteriaManager")
}

/// A generated type retains its exact criterion identity; the generic help placeholder is not an object name.
pub fn manager_name(name: &str) -> Option<&str> {
    let (prefix, name) = name.split_once('.')?;
    (crate::platform_type_name::name_eq_ci(
        prefix,
        "КритерийОтбораМенеджер",
        "FilterCriterionManager",
    ) && !name.is_empty()
        && !name.contains(['.', '<', '>', '&', ';', ' ']))
    .then_some(name)
}

/// Configured global fields retain the container's documented environments.
pub fn environment() -> hir_def::execution_env::EnvFlags {
    hir_def::execution_env::EnvFlags::from_platform_context(
        bsl_platform::PlatformData::instance()
            .get_type("FilterCriteriaManager")
            .and_then(|value| value.context.as_ref()),
    )
}

/// Only the current named criterion can grant a generated manager; unavailable declarations stay unavailable.
pub(crate) fn field(
    db: &dyn TypeKernelDb,
    resolver: &dyn MetadataResolution,
    name: &str,
) -> Option<FieldInfo> {
    let criterion = resolver.resolve_filter_criterion(name)?;
    Some(FieldInfo {
        name: Name::new(criterion.name()),
        name_en: None,
        ty: db.platform_object(format!("FilterCriterionManager.{}", criterion.name())),
        value_ty: None,
        is_readonly: true,
        origin: FieldOrigin::PlatformProperty { env: environment() },
    })
}

/// Completion enumerates the same resolved current fields, without inferring existence from a source filename.
pub(crate) fn fields(db: &dyn TypeKernelDb, resolver: &dyn MetadataResolution) -> Vec<FieldInfo> {
    resolver
        .filter_criterion_names()
        .into_iter()
        .filter_map(|name| field(db, resolver, &name))
        .collect()
}

/// The pinned help embeds the generic placeholder in method names; native documentation supplies callable spelling.
pub fn prototype_methods() -> Vec<bsl_platform::PlatformMethod> {
    crate::generated_platform::methods(PROTOTYPE)
        .into_iter()
        .map(|mut method| {
            if method.english_name.as_str() == "Find" {
                // The help's search-value type is configured rather than a context-free Array.
                if let Some(parameter) = method.parameters.first_mut() {
                    parameter.param_type = None;
                }
            }
            method
        })
        .collect()
}

/// Reuse native method identity, arity, environments and candidate construction; refine only configured Find types.
pub(crate) fn method(
    db: &dyn TypeKernelDb,
    receiver: bsl_types::kind::TypeId,
    name: &Name,
    context: Option<&crate::method_lookup::RefineCtx<'_>>,
) -> Option<crate::method_lookup::MethodInfo> {
    let TypeKind::PlatformObject(facet) = db.lookup_type(receiver) else {
        return None;
    };
    let criterion_name = manager_name(&facet.name)?;
    let native = prototype_methods().into_iter().find(|method| {
        stdx::case::eq_ignore_case(&method.name, name.as_str())
            || stdx::case::eq_ignore_case(&method.english_name, name.as_str())
    })?;
    let mut info = crate::method_lookup::to_method_info(db, &native);
    if native.english_name.as_str() != "Find" {
        return Some(info);
    }
    let criterion = context
        .and_then(|context| context.db.resolve_filter_criterion(context.file_id, criterion_name));
    let resolver = context
        .map(|context| crate::object_resolver::DbObjectResolver::new(context.db, context.file_id));
    let input = criterion
        .as_ref()
        .zip(resolver.as_ref())
        .and_then(|(criterion, resolver)| {
            criterion
                .search_type()
                .map(|ty| crate::field_enum::attribute_type_to_typeid(db, ty, resolver))
        })
        .unwrap_or_else(|| db.unknown());
    let references = criterion
        .as_ref()
        .and_then(|criterion| criterion.content())
        .map(|content| {
            content
                .iter()
                .filter_map(|target| {
                    hir_def::ty::MetadataKind::ref_kind_for(target.kind).map(|kind| {
                        db.metadata_ref(
                            kind,
                            target.name.clone(),
                            &bsl_types::testing::RootConfigCtx,
                        )
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let element = if references.is_empty() { db.unknown() } else { db.union(references) };
    info.return_ty = db.array(Some(element));
    for candidate in info.candidates.signatures_mut() {
        candidate.return_ty = info.return_ty;
        if let Some(parameter) = candidate.params.first_mut() {
            parameter.ty = input;
        }
    }
    Some(info)
}
