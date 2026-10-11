//! Textual namespace consumers share the native ownership verdict at the receiver's exact read.

use hir::{BodyShadowScope, ModuleId, Name, Resolver};
use ide_db::RootDatabase;
use syntax::{SyntaxKind, SyntaxNode, TextSize};
use vfs::FileId;

/// A simple dotted namespace cannot replace a body binding, module member or form/object member.
pub(crate) fn unclaimed_namespace_root<DB: RootDatabase>(
    db: &DB,
    file: FileId,
    offset: TextSize,
    receiver: &SyntaxNode,
) -> Option<Name> {
    let mut root = receiver.clone();
    while root.kind() == SyntaxKind::FIELD_EXPR {
        root = root.children().next()?;
    }
    let mut name = None;
    for child in root.children_with_tokens() {
        let token = child.into_token()?;
        match token.kind() {
            SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE | SyntaxKind::COMMENT => {}
            SyntaxKind::IDENT if name.is_none() => name = Some(Name::new(token.text())),
            _ => return None,
        }
    }
    let name = name?;
    let module = ModuleId::new(file);
    let bodies = db.module_bodies_ref(module);
    let tree = db.item_tree(file);
    let lower = match hir::bare_root::method_item_at(&tree, offset) {
        Some(method) => bodies.lower_result(method.key()),
        None => bodies.module_code_result(),
    };
    let scope = lower.map(|result| BodyShadowScope {
        body: result.body(),
        source_map: result.source_map(),
        read_offset: receiver.text_range().start(),
    });
    // The workspace namespace is the candidate itself; the shared predicate still
    // judges all actual user owners using the same rules as native completion.
    let resolver = Resolver::for_module(module);
    hir::bare_global_name_claim(db, &resolver, scope.as_ref(), &name).is_none().then_some(name)
}
