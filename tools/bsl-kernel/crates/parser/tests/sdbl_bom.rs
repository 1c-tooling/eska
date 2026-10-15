//! Standalone BOM handling preserves the source tree and both parser entry points.

use syntax::{ast::AstNode, ast::SdblSelectedField, SyntaxKind};

/// File encoding trivia must not make a valid query incomplete or shift later byte ranges.
#[test]
fn leading_bom_keeps_complete_original_tree_and_alias_ranges() {
    for parse in [parser::parse_sdbl, parser::parse_sdbl_with_shared_cache] {
        for text in [
            "\u{feff}SELECT 1 AS Alias",
            "\u{feff}// Я😀\r\nSELECT 1 AS Alias\r\n",
            "\u{feff}// comment\r\nВЫБРАТЬ 1 КАК Поле",
        ] {
            let parsed = parse(text);
            assert!(syntax::sdbl_query::collect_query_parse_errors(&parsed).is_empty());
            let root = parsed.syntax_node();
            assert_eq!(root.text().to_string(), text);
            let bom = root.first_token().unwrap();
            assert_eq!(bom.kind(), SyntaxKind::BOM);
            assert_eq!(&text[bom.text_range()], "\u{feff}");
            let field = root.descendants().find_map(SdblSelectedField::cast).unwrap();
            let alias = field.alias().unwrap().identifier().unwrap();
            assert_eq!(&text[alias.text_range()], alias.text());
            assert_eq!(usize::from(alias.text_range().start()), text.rfind(alias.text()).unwrap());
        }
    }
}

/// Only a byte-zero BOM is encoding trivia; repeated/interior BOMs cannot hide invalid syntax.
#[test]
fn interior_and_repeated_bom_remain_errors() {
    for parse in [parser::parse_sdbl, parser::parse_sdbl_with_shared_cache] {
        for text in
            [" \u{feff}SELECT 1 AS A", "\u{feff}\u{feff}SELECT 1 AS A", "SELECT \u{feff}1 AS A"]
        {
            let parsed = parse(text);
            assert_eq!(parsed.syntax_node().text().to_string(), text);
            assert!(!syntax::sdbl_query::collect_query_parse_errors(&parsed).is_empty());
        }
        let text = "SELECT \"\u{feff}\" AS A // \u{feff}";
        assert!(syntax::sdbl_query::collect_query_parse_errors(&parse(text)).is_empty());
    }
}
