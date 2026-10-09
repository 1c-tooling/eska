//! Verify lossless BSL syntax reuse without an analysis database or project metadata.

use std::error::Error;

use serde_json::json;
use syntax::{
    SyntaxKind,
    ast::{AstNode, FunctionDef, ReturnStmt},
};

/// Locate a literal through the CST, accounting for a preserved UTF-8 BOM outside the parser.
fn version_case(version: &str) -> Result<serde_json::Value, Box<dyn Error>> {
    let source = format!(
        "\u{feff}// Version is private to this object module.\r\nФункция Версия()\r\n\tВозврат \"{version}\"; // Keep this comment.\r\nКонецФункции\r\n"
    );
    let text = source.strip_prefix('\u{feff}').ok_or("missing BOM")?;
    let parsed = parser::parse(text);
    if !parsed.errors().is_empty() || parsed.syntax_node().to_string() != text {
        return Err("parser failed to preserve source text".into());
    }
    let function = parsed
        .syntax_node()
        .children()
        .find_map(FunctionDef::cast)
        .ok_or("missing function")?;
    if function.export_keyword().is_some()
        || function.name().is_none_or(|token| token.text() != "Версия")
    {
        return Err("wrong version declaration".into());
    }
    let statement = function
        .syntax()
        .descendants()
        .find_map(ReturnStmt::cast)
        .ok_or("missing return")?;
    let literal = statement
        .syntax()
        .descendants_with_tokens()
        .filter_map(|item| item.into_token())
        .find(|token| token.kind() == SyntaxKind::STRING)
        .ok_or("missing literal")?;
    let start = usize::from(literal.text_range().start()) + '\u{feff}'.len_utf8();
    let end = usize::from(literal.text_range().end()) + '\u{feff}'.len_utf8();
    if source[start..end] != format!("\"{version}\"") {
        return Err("wrong byte range".into());
    }
    let mut replacement = source.clone();
    replacement.replace_range(start..end, "\"2.3.4\"");
    let parsed = parser::parse(replacement.trim_start_matches('\u{feff}'));
    if !parsed.errors().is_empty()
        || !replacement.starts_with('\u{feff}')
        || !replacement.ends_with("КонецФункции\r\n")
    {
        return Err("literal replacement damaged source".into());
    }
    Ok(
        json!({"version":version,"literal_range":[start,end],"bom_crlf_preserved":true,"non_exported":true}),
    )
}

/// Check both version shapes, malformed syntax and byte-exact trivia with the same parser.
fn main() -> Result<(), Box<dyn Error>> {
    let cases = [version_case("1.2.3")?, version_case("1.2.3.4")?];
    if parser::parse("Функция Версия(\nВозврат \"1.2.3\";")
        .errors()
        .is_empty()
    {
        return Err("malformed syntax accepted".into());
    }
    println!(
        "{}",
        json!({"cases":cases,"invalid_syntax_rejected":true,"analysis_database":false})
    );
    Ok(())
}
