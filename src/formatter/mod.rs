use tree_sitter::{Node, TreeCursor};

pub struct FormatterConfig {
    pub indent_style: String,
}

pub struct Formatter<'a> {
    source: &'a str,
    output: String,
    config: FormatterConfig,
    indent_level: usize,
}

impl<'a> Formatter<'a> {
    pub fn new(source: &'a str, config: FormatterConfig) -> Self {
        Self {
            source,
            output: String::new(),
            config,
            indent_level: 0,
        }
    }

    pub fn format(mut self, root: Node) -> String {
        let mut cursor = root.walk();
        self.visit_node(&mut cursor);
        self.output
    }

    fn visit_node(&mut self, cursor: &mut TreeCursor) {
        let node = cursor.node();

        // Если есть дети, идем внутрь
        if cursor.goto_first_child() {
            // Увеличиваем отступ, если вошли в блок
            if self.is_block_start(node.kind()) {
                self.indent_level += 1;
            }

            loop {
                self.visit_node(cursor);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }

            // Возвращаемся наверх
            if self.is_block_start(node.kind()) {
                self.indent_level -= 1;
            }
            cursor.goto_parent();
        } else {
            // Это лист (Token). Пишем его в output.
            let text = &self.source[node.byte_range()];
            // Тут будет логика: нужно ли вставить пробел перед этим словом?
            self.output.push_str(text);
        }
    }

    fn is_block_start(&self, kind: &str) -> bool {
        // Тут нужно свериться с grammar.js из tree-sitter-bsl
        // Например: "procedure_definition", "if_statement", "method_definition"
        matches!(
            kind,
            "procedure_definition" | "function_definition" | "if_statement"
        )
    }
}
