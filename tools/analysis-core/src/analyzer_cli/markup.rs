//! Hover and signature documentation share standard format negotiation and literal text escaping.

use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    Plaintext,
    Markdown,
}

impl Format {
    /// Use the first supported client preference; absent preferences receive plain text.
    pub fn preferred(formats: Option<&Value>) -> Self {
        formats
            .and_then(Value::as_array)
            .and_then(|formats| {
                formats.iter().find_map(|format| match format.as_str() {
                    Some("plaintext") => Some(Self::Plaintext),
                    Some("markdown") => Some(Self::Markdown),
                    _ => None,
                })
            })
            .unwrap_or(Self::Plaintext)
    }

    /// Markup kinds are wire vocabulary and never depend on presentation locale.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Plaintext => "plaintext",
            Self::Markdown => "markdown",
        }
    }

    /// Documentation data is rendered as literal text rather than executable Markdown content.
    pub fn document(self, text: &str) -> Value {
        json!({"kind":self.as_str(),"value":match self {
            Self::Plaintext=>text.to_owned(),Self::Markdown=>escape(text),
        }})
    }
}

/// Escape `CommonMark` punctuation so source comments cannot create headings, links, images or HTML.
pub fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_ascii_punctuation() {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}
