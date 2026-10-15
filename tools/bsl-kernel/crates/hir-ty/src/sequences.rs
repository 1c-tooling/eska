//! Current sequence declarations refine their real generated platform types without a register surrogate.

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

/// The three generated sequence roles are distinct platform objects.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SequenceType {
    /// Exposes sequence operations and creates a record set.
    Manager,
    /// Owns recorder-scoped reads/writes and creates records.
    RecordSet,
    /// Owns Period, Recorder and declared sequence dimensions.
    Record,
}

impl SequenceType {
    /// The pinned catalog stores the actual generic owner with its escaped placeholder.
    pub fn prototype(self) -> &'static str {
        match self {
            Self::Manager => "SequenceManager.&lt;Sequence name&gt;",
            Self::RecordSet => "SequenceRecordSet.&lt;Sequence name&gt;",
            Self::Record => "SequenceRecord.&lt;Sequence name&gt;",
        }
    }

    /// A generated type uses the configured name while retaining its platform role.
    pub fn type_name(self, name: &str) -> String {
        let prefix = match self {
            Self::Manager => "SequenceManager",
            Self::RecordSet => "SequenceRecordSet",
            Self::Record => "SequenceRecord",
        };
        format!("{prefix}.{name}")
    }
}

/// Global-name resolution and shadowing precede recognition of this real platform container.
pub fn is_container_name(name: &str) -> bool {
    crate::platform_type_name::name_eq_ci(name, "ПоследовательностиМенеджер", "SequencesManager")
}

/// Placeholders, nested paths and non-identifiers cannot name a configured sequence.
pub fn generated_name(type_name: &str) -> Option<(SequenceType, &str)> {
    let (prefix, name) = type_name.split_once('.')?;
    if name.is_empty() || name.contains(['.', '<', '>', '&', ';', ' ']) {
        return None;
    }
    for (kind, ru, en) in [
        (SequenceType::Manager, "ПоследовательностьМенеджер", "SequenceManager"),
        (SequenceType::RecordSet, "ПоследовательностьНаборЗаписей", "SequenceRecordSet"),
        (SequenceType::Record, "ПоследовательностьЗапись", "SequenceRecord"),
    ] {
        if crate::platform_type_name::name_eq_ci(prefix, ru, en) {
            return Some((kind, name));
        }
    }
    None
}

/// Container fields retain their native thick-client/server/external-connection environments.
pub fn environment() -> hir_def::execution_env::EnvFlags {
    hir_def::execution_env::EnvFlags::from_platform_context(
        bsl_platform::PlatformData::instance()
            .get_type("SequencesManager")
            .and_then(|value| value.context.as_ref()),
    )
}

/// A declared current sequence grants its manager; filenames and old native XML do not.
fn manager_field(
    db: &dyn TypeKernelDb,
    resolver: &dyn MetadataResolution,
    name: &str,
) -> Option<FieldInfo> {
    let sequence = resolver.resolve_sequence(name)?;
    Some(FieldInfo {
        name: Name::new(sequence.name()),
        name_en: None,
        ty: db.platform_object(SequenceType::Manager.type_name(sequence.name())),
        value_ty: None,
        is_readonly: true,
        origin: FieldOrigin::PlatformProperty { env: environment() },
    })
}

/// Generated record properties and dimensions share one current resolved owner and native field flags.
pub(crate) fn fields(
    db: &dyn TypeKernelDb,
    resolver: &dyn MetadataResolution,
    type_name: &str,
) -> Option<Vec<FieldInfo>> {
    if is_container_name(type_name) {
        return Some(
            resolver
                .sequence_names()
                .into_iter()
                .filter_map(|name| manager_field(db, resolver, &name))
                .collect(),
        );
    }
    let (kind, name) = generated_name(type_name)?;
    let Some(sequence) = resolver.resolve_sequence(name) else {
        return Some(Vec::new());
    };
    let mut fields = bsl_platform::PlatformData::instance()
        .get_type_properties(kind.prototype())
        .into_iter()
        .map(|property| {
            let native = crate::platform_property_lookup::to_resolution(db, property);
            let ty = match property.english_name.as_str() {
                "ThisObject" => db.platform_object(kind.type_name(sequence.name())),
                "Recorder" => recorder_type(db, &sequence),
                _ => native.return_ty,
            };
            FieldInfo {
                name: Name::new(&property.name),
                name_en: Some(Name::new(&property.english_name)),
                ty,
                value_ty: None,
                is_readonly: native.is_readonly,
                origin: FieldOrigin::PlatformProperty { env: native.env },
            }
        })
        .collect::<Vec<_>>();
    if kind == SequenceType::Record {
        fields.extend(sequence.dimensions().iter().map(|dimension| FieldInfo {
            name: Name::new(&dimension.name),
            name_en: None,
            ty: dimension.attribute_type.as_ref().map_or_else(
                || db.unknown(),
                |ty| crate::field_enum::attribute_type_to_typeid(db, ty, resolver),
            ),
            value_ty: None,
            is_readonly: false,
            origin: FieldOrigin::PlatformProperty { env: environment() },
        }));
    }
    Some(fields)
}

/// Recorder candidates come from the current sequence's declared document set.
fn recorder_type(db: &dyn TypeKernelDb, sequence: &bsl_metadata::Sequence) -> TypeId {
    let types = sequence
        .documents()
        .unwrap_or_default()
        .iter()
        .map(|name| {
            db.metadata_ref(
                hir_def::ty::MetadataKind::DocumentRef,
                name.clone(),
                &bsl_types::testing::RootConfigCtx,
            )
        })
        .collect::<Vec<_>>();
    if types.is_empty() {
        db.unknown()
    } else {
        db.union(types)
    }
}

/// Named lookup and completion enumerate identical current generated fields.
pub(crate) fn field(
    db: &dyn TypeKernelDb,
    resolver: &dyn MetadataResolution,
    type_name: &str,
    name: &Name,
) -> Option<FieldInfo> {
    fields(db, resolver, type_name)?.into_iter().find(|field| {
        stdx::case::eq_ignore_case(field.name.as_str(), name.as_str())
            || field
                .name_en
                .as_ref()
                .is_some_and(|other| stdx::case::eq_ignore_case(other.as_str(), name.as_str()))
    })
}

/// Native method ids, documented parameters, contexts and arity survive placeholder normalization.
pub fn prototype_methods(prototype: &str) -> Option<Vec<bsl_platform::PlatformMethod>> {
    [SequenceType::Manager, SequenceType::RecordSet, SequenceType::Record]
        .into_iter()
        .any(|kind| kind.prototype() == prototype)
        .then(|| crate::generated_platform::methods(prototype))
}

/// Method inference refines only generated return identities whose current owner has been proven.
pub(crate) fn method(
    db: &dyn TypeKernelDb,
    receiver: TypeId,
    name: &Name,
    context: Option<&crate::method_lookup::RefineCtx<'_>>,
) -> Option<crate::method_lookup::MethodInfo> {
    let TypeKind::PlatformObject(facet) = db.lookup_type(receiver) else {
        return None;
    };
    let (kind, sequence_name) = generated_name(&facet.name)?;
    let native =
        crate::generated_platform::methods(kind.prototype()).into_iter().find(|method| {
            stdx::case::eq_ignore_case(&method.name, name.as_str())
                || stdx::case::eq_ignore_case(&method.english_name, name.as_str())
        })?;
    let mut info = crate::method_lookup::to_method_info(db, &native);
    let result = match (kind, native.english_name.as_str()) {
        (SequenceType::Manager, "CreateRecordSet") => Some(SequenceType::RecordSet),
        (SequenceType::RecordSet, "Add" | "Get") => Some(SequenceType::Record),
        _ => None,
    };
    if let Some(result) = result {
        info.return_ty = context
            .and_then(|context| context.db.resolve_sequence(context.file_id, sequence_name))
            .map_or_else(
                || db.unknown(),
                |sequence| db.platform_object(result.type_name(sequence.name())),
            );
        for candidate in info.candidates.signatures_mut() {
            candidate.return_ty = info.return_ty;
        }
    }
    Some(info)
}
