//! Display the configured native candidate types without re-resolving generic platform placeholders.

use hir::{CandidateCallBinding, Semantics, TypeId};
use ide_db::{base_db::Locale, RootDatabase};
use symbol_info::{SymbolSignature, TypeRef};
use syntax::{SyntaxKind, SyntaxToken};
use vfs::FileId;

/// Keep native candidate identity, docs and arity; only configured criteria have context-dependent display types.
pub(crate) fn refine<DB: RootDatabase>(
    db: &DB,
    binding: &CandidateCallBinding,
    signatures: &mut [SymbolSignature],
) {
    let methods = hir_ty::filter_criteria::prototype_methods();
    for signature in signatures {
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
    let arguments = call.children().find(|node| node.kind() == SyntaxKind::ARG_LIST)?;
    let offset = arguments.text_range().start() + syntax::TextSize::from(1);
    let binding = Semantics::new(db).call_binding_at(file, offset)?;
    let methods = hir_ty::filter_criteria::prototype_methods();
    if !binding.candidates.as_slice().iter().any(|candidate| {
        matches!(candidate.id, hir::CandidateId::Platform { method_id, .. }
            if methods.iter().any(|method| method.id == method_id))
    }) {
        return None;
    }
    let mut signatures = symbol_info::build_signature_from_resolution(db, &binding)?;
    refine(db, &binding, &mut signatures);
    Some((binding, signatures))
}
