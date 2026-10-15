//! Display the configured native candidate types without re-resolving generic platform placeholders.

use hir::{CandidateCallBinding, Semantics, TypeId};
use ide_db::{base_db::Locale, RootDatabase};
use symbol_info::{SignatureSource, SymbolSignature, TypeRef};
use syntax::{SyntaxKind, SyntaxToken};
use vfs::FileId;

/// Keep native candidate identity, docs and arity; configured calls retain their context-dependent display types.
pub(crate) fn refine<DB: RootDatabase>(
    db: &DB,
    binding: &CandidateCallBinding,
    signatures: &mut [SymbolSignature],
) {
    let methods = configured_methods();
    for signature in signatures {
        if signature.source == SignatureSource::GlobalFunction {
            if let Some(id) = signature.platform_id.filter(|id| configured_global(*id)) {
                if signature.platform_id == Some(id) {
                    if let Some(candidate) = binding.candidates.as_slice().iter().find(|candidate| {
                        matches!(candidate.id, hir::CandidateId::Builtin { callable: hir::BuiltinCallableId::PlatformGlobal(value), .. } if value == id)
                    }) {
                        signature.returns = vec![type_ref(db, candidate.return_ty)];
                    }
                }
            }
            continue;
        }
        if signature.source != SignatureSource::Platform {
            continue;
        }
        let Some(method) = methods.iter().find(|method| Some(method.id) == signature.platform_id)
        else {
            continue;
        };
        let Some(candidate) = binding.candidates.as_slice().iter().find(|candidate| {
            matches!(candidate.id, hir::CandidateId::Platform { method_id, .. } if method_id == method.id)
        }) else { continue; };
        signature.name_russian = method.name.clone();
        signature.name_english = Some(method.english_name.clone());
        signature.qualifier = None;
        for (display, parameter) in signature.params.iter_mut().zip(&candidate.params) {
            display.types = vec![type_ref(db, parameter.ty)];
        }
        signature.returns = vec![type_ref(db, candidate.return_ty)];
    }
}

/// Type labels come from the same native interned candidates as inference and argument diagnostics.
fn type_ref<DB: RootDatabase>(db: &DB, ty: TypeId) -> TypeRef {
    TypeRef {
        russian: hir::kernel_type_label(db, ty, Locale::Ru, false).into(),
        english: Some(hir::kernel_type_label(db, ty, Locale::En, false).into()),
        description: None,
        is_hyperlink: false,
    }
}

/// A hovered callee reuses its own argument-list binding, never another textual same-name manager.
pub(crate) fn at_token<DB: RootDatabase>(
    db: &DB,
    file: FileId,
    token: &SyntaxToken,
) -> Option<(CandidateCallBinding, Vec<SymbolSignature>)> {
    let call = token.parent_ancestors().find(|node| node.kind() == SyntaxKind::CALL_EXPR)?;
    let callee = call.children().next()?;
    if !callee.text_range().contains_range(token.text_range()) {
        return None;
    }
    let arguments = call.children().find(|node| node.kind() == SyntaxKind::ARG_LIST)?;
    let offset = arguments.text_range().start() + syntax::TextSize::from(1);
    let binding = Semantics::new(db).call_binding_at(file, offset)?;
    let methods = configured_methods();
    if !binding.candidates.as_slice().iter().any(|candidate| match candidate.id {
        hir::CandidateId::Platform { method_id, .. } => {
            methods.iter().any(|method| method.id == method_id)
        }
        hir::CandidateId::Builtin {
            callable: hir::BuiltinCallableId::PlatformGlobal(id), ..
        } => configured_global(id),
        _ => false,
    }) {
        return None;
    }
    let mut signatures = symbol_info::build_signature_from_resolution(db, &binding)?;
    refine(db, &binding, &mut signatures);
    Some((binding, signatures))
}

/// Both configured global consumers retain their actual platform callable IDs.
fn configured_global(id: u32) -> bool {
    hir_ty::functional_options::platform_id() == Some(id)
        || hir_ty::functional_option_parameters::platform_id() == Some(id)
}

/// Each configured generated family keeps its native platform method IDs and current candidate types.
fn configured_methods() -> Vec<bsl_platform::PlatformMethod> {
    let mut methods = hir_ty::filter_criteria::prototype_methods();
    methods.extend(hir_ty::settings_storages::prototype_methods());
    for kind in [
        hir_ty::sequences::SequenceType::Manager,
        hir_ty::sequences::SequenceType::RecordSet,
        hir_ty::sequences::SequenceType::Record,
    ] {
        methods.extend(hir_ty::generated_platform::methods(kind.prototype()));
    }
    methods
}
