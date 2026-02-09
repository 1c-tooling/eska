use crate::error::Result;
use crate::formatter::{Formatter, FormatterConfig};
use tree_sitter::Parser;

#[allow(clippy::unused_async)]
pub async fn run() -> Result<String> {
    let mut parser = Parser::new();
    let language = tree_sitter::Language::from(tree_sitter_bsl::LANGUAGE);
    parser
        .set_language(&language)
        .expect("Ошибка загрузки BSL грамматики");

    let code = "Процедура Привет() КонецПроцедуры";
    let tree = parser.parse(code, None).unwrap();

    let config = FormatterConfig {
        indent_style: "\t".to_string(),
    };

    let formatter = Formatter::new(code, config);
    let formatted_code = formatter.format(tree.root_node());

    Ok(formatted_code)
}
