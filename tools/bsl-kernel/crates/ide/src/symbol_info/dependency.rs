//! A current member-name token selects the same direct dependency as native completion without resolving its body.

use ide_db::{base_db::RootQueryDb, RootDatabaseImpl};
use syntax::{SyntaxKind, TextSize};
use vfs::FileId;

/// Restrict preparation to the hovered member, accepting either identifier boundary.
pub(crate) fn hover_offset(
    db: &RootDatabaseImpl,
    file: FileId,
    offset: TextSize,
) -> Option<TextSize> {
    let parse = db.parse(file);
    let token = parse
        .syntax_node()
        .token_at_offset(offset)
        .find(|token| matches!(hir::classify_token(token), hir::NameClass::FieldName { .. }))?;
    token
        .parent()?
        .children_with_tokens()
        .find(|element| element.kind() == SyntaxKind::DOT)
        .map(|dot| dot.text_range().end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ide_db::base_db::SourceDatabase;

    /// Native classification keeps a selected method or property's own receiver at each identifier boundary.
    #[test]
    fn hovered_member_dependency_anchor_accepts_method_boundaries_and_nested_calls() {
        for (body, receiver) in [
            ("Procedure Run()\n Utility.Method();\nEndProcedure\n", "Utility."),
            ("Procedure Run()\n Catalogs.Items.Method();\nEndProcedure\n", "Catalogs.Items."),
            ("Procedure Run()\n Utility.Outer(Neighbor.Method());\nEndProcedure\n", "Neighbor."),
            ("Procedure Run()\n Value=Item.Method;\nEndProcedure\n", "Item."),
        ] {
            let mut db = RootDatabaseImpl::new();
            let file = FileId(0);
            db.set_file_text(file, body);
            let start = body.find("Method").unwrap();
            let anchor = TextSize::from(
                u32::try_from(body.find(receiver).unwrap() + receiver.len()).unwrap(),
            );
            for position in [start, start + 2, start + "Method".len()] {
                assert_eq!(
                    hover_offset(&db, file, TextSize::from(u32::try_from(position).unwrap())),
                    Some(anchor),
                    "{body}"
                );
            }
        }
    }

    /// Bare receivers, constructors and non-code tokens cannot select a different member for preparation.
    #[test]
    fn hovered_member_dependency_anchor_rejects_non_callee_tokens() {
        for body in [
            "Procedure Run()\n $0Utility.Method();\nEndProcedure\n",
            "Procedure Run()\n $0Local(Utility.Method());\nEndProcedure\n",
            "Procedure Run()\n Value=New $0Structure();\nEndProcedure\n",
            "Procedure Run()\n Value=\"Utility.$0Method()\";\nEndProcedure\n",
            "Procedure Run()\n // Utility.$0Method()\nEndProcedure\n",
        ] {
            let mut db = RootDatabaseImpl::new();
            let file = FileId(0);
            let offset = TextSize::from(u32::try_from(body.find("$0").unwrap()).unwrap());
            db.set_file_text(file, &body.replace("$0", ""));
            assert_eq!(hover_offset(&db, file, offset), None, "{body}");
        }
    }
}
