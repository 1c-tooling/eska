//! Cold discovery admits only real platform globals in the current native body.

use hir::{BuiltinCallableId, CandidateId, Semantics};
use ide_db::RootDatabase;
use syntax::{SyntaxKind, SyntaxNode, TextSize};
use vfs::FileId;

/// A same-spelled user method or a dynamic argument cannot authorize named-option preparation.
pub(crate) fn names<DB: RootDatabase>(db: &DB, file: FileId, offset: TextSize) -> Vec<String> {
    let Some(id) = hir_ty::functional_options::platform_id() else {
        return Vec::new();
    };
    let mut result: Vec<String> = Vec::new();
    for call in platform_calls(db, file, offset, id) {
        let Some(arguments) = call.children().find(|node| node.kind() == SyntaxKind::ARG_LIST)
        else {
            continue;
        };
        let Some(mut literal) = arguments.children().next() else {
            continue;
        };
        while matches!(literal.kind(), SyntaxKind::EXPR | SyntaxKind::PAREN_EXPR) {
            let mut children = literal.children();
            let Some(child) = children.next().filter(|_| children.next().is_none()) else {
                break;
            };
            literal = child;
        }
        if literal.kind() != SyntaxKind::LITERAL {
            continue;
        }
        let Some(token) = literal.first_token().filter(|token| token.kind() == SyntaxKind::STRING)
        else {
            continue;
        };
        let name = hir::unquote_bsl_string(token.text());
        if !name.is_empty() && !result.iter().any(|value| stdx::case::eq_ignore_case(value, &name))
        {
            result.push(name);
        }
    }
    result
}

/// The interface getter can prepare only its own declared parameter family and its current Use owners.
pub(crate) fn interface_parameters<DB: RootDatabase>(
    db: &DB,
    file: FileId,
    offset: TextSize,
) -> bool {
    hir_ty::functional_option_parameters::platform_id()
        .is_some_and(|id| !platform_calls(db, file, offset, id).is_empty())
}

/// Native call binding proves ownership; body boundaries prevent unrelated source from granting preparation.
fn platform_calls<DB: RootDatabase>(
    db: &DB,
    file: FileId,
    offset: TextSize,
    id: u32,
) -> Vec<SyntaxNode> {
    let root = db.parse(file).syntax_node();
    let Some(token) = root.token_at_offset(offset).left_biased() else {
        return Vec::new();
    };
    let owner = token
        .parent_ancestors()
        .find(|node| matches!(node.kind(), SyntaxKind::PROCEDURE_DEF | SyntaxKind::FUNCTION_DEF));
    let scope = owner.clone().unwrap_or(root);
    scope.descendants().filter(|node| node.kind() == SyntaxKind::CALL_EXPR).filter(|call| {
        let call_owner = call.ancestors().find(|node| matches!(node.kind(), SyntaxKind::PROCEDURE_DEF | SyntaxKind::FUNCTION_DEF));
        if call_owner != owner { return false; }
        let Some(arguments) = call.children().find(|node| node.kind() == SyntaxKind::ARG_LIST) else { return false; };
        Semantics::new(db).call_binding_at(file, arguments.text_range().start() + TextSize::from(1)).is_some_and(|binding| binding.candidates.as_slice().iter().any(|candidate| matches!(candidate.id, CandidateId::Builtin { callable: BuiltinCallableId::PlatformGlobal(value), .. } if value == id)))
    }).collect()
}
