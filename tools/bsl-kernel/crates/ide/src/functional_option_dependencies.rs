//! Cold discovery admits only literal calls bound to the real platform global in the current native body.

use hir::{BuiltinCallableId, CandidateId, Semantics};
use ide_db::RootDatabase;
use syntax::{SyntaxKind, TextSize};
use vfs::FileId;

/// A same-spelled user method or a dynamic argument cannot authorize metadata preparation.
pub(crate) fn names<DB: RootDatabase>(db: &DB, file: FileId, offset: TextSize) -> Vec<String> {
    let Some(platform_id) = hir_ty::functional_options::platform_id() else {
        return Vec::new();
    };
    let root = db.parse(file).syntax_node();
    let Some(token) = root.token_at_offset(offset).left_biased() else {
        return Vec::new();
    };
    let owner = token
        .parent_ancestors()
        .find(|node| matches!(node.kind(), SyntaxKind::PROCEDURE_DEF | SyntaxKind::FUNCTION_DEF));
    let scope = owner.clone().unwrap_or(root);
    let mut result: Vec<String> = Vec::new();
    for call in scope.descendants().filter(|node| node.kind() == SyntaxKind::CALL_EXPR) {
        let call_owner = call.ancestors().find(|node| {
            matches!(node.kind(), SyntaxKind::PROCEDURE_DEF | SyntaxKind::FUNCTION_DEF)
        });
        if call_owner != owner {
            continue;
        }
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
        let Some(binding) = Semantics::new(db)
            .call_binding_at(file, arguments.text_range().start() + TextSize::from(1))
        else {
            continue;
        };
        if !binding.candidates.as_slice().iter().any(|candidate| matches!(candidate.id, CandidateId::Builtin { callable: BuiltinCallableId::PlatformGlobal(id), .. } if id == platform_id)) { continue; }
        let name = hir::unquote_bsl_string(token.text());
        if !name.is_empty() && !result.iter().any(|value| stdx::case::eq_ignore_case(value, &name))
        {
            result.push(name);
        }
    }
    result
}
