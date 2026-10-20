//! Conservative, lossless BSL method extraction for the initial patch allowlist.

use std::{collections::BTreeMap, ops::Range};

#[derive(Debug)]
struct Token<'a> {
    text: &'a str,
    range: Range<usize>,
    word: bool,
}

#[derive(Debug)]
struct Routine<'a> {
    name: &'a str,
    name_range: Range<usize>,
    range: Range<usize>,
    signature: Vec<String>,
    body: Vec<Token<'a>>,
}

/// Generate replacements only when every declaration and non-method token is accounted for.
pub(super) fn replacements(
    before: &str,
    after: &str,
    prefix: &str,
) -> Option<(String, Vec<String>)> {
    let old = routines(before)?;
    let new = routines(after)?;
    if old.keys().ne(new.keys()) {
        return None;
    }
    let mut output = String::new();
    let mut names = Vec::new();
    for (key, routine) in new {
        let previous = old.get(&key)?;
        if previous.signature != routine.signature {
            return None;
        }
        if same_tokens(&previous.body, &routine.body) {
            continue;
        }
        if !self_contained(&routine.body) {
            return None;
        }
        let generated_name = format!("{prefix}{}", routine.name);
        if old.contains_key(&generated_name.to_lowercase()) {
            return None;
        }
        output.push_str("&Around(\"");
        output.push_str(routine.name);
        output.push_str("\")\n");
        output.push_str(&after[routine.range.start..routine.name_range.start]);
        output.push_str(&generated_name);
        output.push_str(&after[routine.name_range.end..routine.range.end]);
        output.push_str("\n\n");
        names.push(routine.name.to_owned());
    }
    Some((output, names))
}

/// Treat whitespace and comments as presentation so they cannot create an empty patch method.
fn same_tokens(left: &[Token<'_>], right: &[Token<'_>]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.text == right.text)
}

/// Reject calls, member access and dynamic execution until their binding is separately proven.
fn self_contained(tokens: &[Token<'_>]) -> bool {
    for (index, token) in tokens.iter().enumerate() {
        if matches!(token.text, "." | "[" | "]")
            || matches!(
                token.text.to_lowercase().as_str(),
                "new" | "новый" | "execute" | "выполнить"
            )
        {
            return false;
        }
        if token.word
            && tokens.get(index + 1).is_some_and(|next| next.text == "(")
            && !matches!(
                token.text.to_lowercase().as_str(),
                "if" | "если"
                    | "elsif"
                    | "иначеесли"
                    | "while"
                    | "пока"
                    | "return"
                    | "возврат"
                    | "not"
                    | "не"
                    | "and"
                    | "и"
                    | "or"
                    | "или"
            )
        {
            return false;
        }
    }
    true
}

/// Parse complete synchronous top-level methods, never skipping executable module text.
fn routines(source: &str) -> Option<BTreeMap<String, Routine<'_>>> {
    let mut tokens = tokenize(source)?.into_iter().peekable();
    let mut routines = BTreeMap::new();
    while let Some(start) = tokens.next() {
        let ending = match start.text.to_lowercase().as_str() {
            "function" | "функция" => ["endfunction", "конецфункции"],
            "procedure" | "процедура" => ["endprocedure", "конецпроцедуры"],
            _ => return None,
        };
        let name = tokens.next()?;
        if !name.word || tokens.next()?.text != "(" {
            return None;
        }
        let mut signature = vec![start.text.to_lowercase(), name.text.to_lowercase()];
        loop {
            let token = tokens.next()?;
            if token.text == ")" {
                break;
            }
            if matches!(token.text, "(" | ";") {
                return None;
            }
            signature.push(if token.word {
                token.text.to_lowercase()
            } else {
                token.text.to_owned()
            });
        }
        if tokens
            .peek()
            .is_some_and(|token| matches!(token.text.to_lowercase().as_str(), "export" | "экспорт"))
        {
            signature.push(tokens.next()?.text.to_lowercase());
        }
        let mut body = Vec::new();
        let end = loop {
            let token = tokens.next()?;
            if token.word && ending.contains(&token.text.to_lowercase().as_str()) {
                break token.range.end;
            }
            if token.word
                && matches!(
                    token.text.to_lowercase().as_str(),
                    "function"
                        | "функция"
                        | "procedure"
                        | "процедура"
                        | "endfunction"
                        | "конецфункции"
                        | "endprocedure"
                        | "конецпроцедуры"
                )
            {
                return None;
            }
            body.push(token);
        };
        let key = name.text.to_lowercase();
        if routines
            .insert(
                key,
                Routine {
                    name: name.text,
                    name_range: name.range,
                    range: start.range.start..end,
                    signature,
                    body,
                },
            )
            .is_some()
        {
            return None;
        }
    }
    Some(routines)
}

/// Lex strings and comments before keywords so embedded declarations cannot change boundaries.
fn tokenize(source: &str) -> Option<Vec<Token<'_>>> {
    let mut output = Vec::new();
    let mut chars = source.char_indices().peekable();
    while let Some((start, character)) = chars.next() {
        if character.is_whitespace() || (start == 0 && character == '\u{feff}') {
            continue;
        }
        if character == '/' && chars.peek().is_some_and(|(_, c)| *c == '/') {
            for (_, c) in chars.by_ref() {
                if c == '\n' {
                    break;
                }
            }
            continue;
        }
        if matches!(character, '&' | '#') {
            return None;
        }
        let word = character.is_alphabetic() || character == '_';
        if matches!(character, '"' | '\'') {
            loop {
                let (_, c) = chars.next()?;
                if c == character {
                    if chars.peek().is_some_and(|(_, next)| *next == character) {
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
        } else if word {
            while chars
                .peek()
                .is_some_and(|(_, c)| c.is_alphanumeric() || *c == '_')
            {
                chars.next();
            }
        } else if character.is_ascii_digit() {
            while chars
                .peek()
                .is_some_and(|(_, c)| c.is_ascii_digit() || *c == '.')
            {
                chars.next();
            }
        }
        let end = chars.peek().map_or(source.len(), |(index, _)| *index);
        output.push(Token {
            text: &source[start..end],
            range: start..end,
            word,
        });
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::replacements;

    /// Preserve bodies and ignore fake declarations inside strings and comments.
    #[test]
    fn replaces_complete_methods_in_both_languages() {
        for (start, end, export, ret) in [
            ("Function", "EndFunction", "Export", "Return"),
            ("Функция", "КонецФункции", "Экспорт", "Возврат"),
        ] {
            let base = format!(
                "\u{feff}// Function Fake()\r\n{start} Value(X) {export}\r\n {ret} X + 1;\r\n{end}"
            );
            let head = base.replace("X + 1", "X + 2");
            let (generated, names) = replacements(&base, &head, "Patch_").expect("supported");
            assert_eq!(names, ["Value"]);
            assert!(generated.contains("&Around(\"Value\")"));
            assert!(generated.contains("Patch_Value(X)"));
            assert!(generated.contains("X + 2"));
        }
        let base = "Function F()\n Return \"EndFunction\";\nEndFunction";
        let (generated, names) =
            replacements(base, &base.replace("Return", "// comment\nReturn"), "P_")
                .expect("comments are presentation");
        assert!(generated.is_empty());
        assert!(names.is_empty());
    }

    /// Reject every unproven construction rather than returning a partial patch.
    #[test]
    fn rejects_unsafe_or_incomplete_modules() {
        let base = "Function F(X)\n Return X;\nEndFunction";
        for head in [
            base.replace("F(X)", "F(Y)"),
            base.replace("X;", "Other(X);"),
            base.replace("X;", "X.Value;"),
            base.replace("X;", "X[0];"),
            format!("Var Global;\n{base}"),
            format!("#If Server Then\n{base}"),
            format!("&AtServer\n{base}"),
            base.replace("EndFunction", ""),
            format!("{base}\n{base}"),
            base.replace("F(X)", "G(X)"),
            base.replace("X;", "Execute \"code\";"),
            base.replace("X;", "\"unterminated;"),
        ] {
            assert!(replacements(base, &head, "P_").is_none(), "{head}");
        }
    }
}
