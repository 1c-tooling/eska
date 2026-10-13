//! Query text is removed from native gaps; closed lexical recipes retain escaping and XML transport.

use std::ops::Range;

use roxmltree::Node;

use super::{Error, binding};
use crate::project_core::source_format::{lexical, native_writer, xml};

#[derive(Clone, Debug, Eq, PartialEq)]
enum Ending {
    Lf,
    CrLf,
    Cr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Carriage {
    Decimal,
    UpperHex,
    LowerHex,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Coding {
    Text {
        quotes: bool,
        apostrophes: bool,
        carriage: Carriage,
    },
    Cdata,
}

/// Frames contain no old query value, including when the native element is self-closing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Frame {
    pub(super) range: Range<usize>,
    open: String,
    close: String,
    empty: Option<String>,
    ending: Ending,
    coding: Coding,
}

impl Frame {
    /// Promotion requires a byte-equal inverse; unsupported lexical forms never become hidden fallbacks.
    pub(super) fn from_node(node: Node<'_, '_>, input: &str) -> Result<Self, Error> {
        if node.attributes().len() != 0 || node.children().any(|child| !child.is_text()) {
            return Err(unverified());
        }
        let range = node.range();
        let original = input.get(range.clone()).ok_or_else(binding)?;
        let (qname, _) = lexical::opening_name(input, range.start).ok_or_else(binding)?;
        let (open, close, empty, content) = if let Some(prefix) = original.strip_suffix("/>") {
            (
                format!("{prefix}>"),
                format!("</{qname}>"),
                Some(original.to_owned()),
                "",
            )
        } else {
            let start = lexical::opening_end(input, range.clone()).ok_or_else(binding)?;
            let end = range.start + original.rfind("</").ok_or_else(binding)?;
            (
                input.get(range.start..start).ok_or_else(binding)?.into(),
                input.get(end..range.end).ok_or_else(binding)?.into(),
                None,
                input.get(start..end).ok_or_else(binding)?,
            )
        };
        let value = Self {
            range,
            open,
            close,
            empty,
            ending: ending(content)?,
            coding: coding(content),
        };
        if value.render(&xml::text(node)) != original {
            return Err(unverified());
        }
        Ok(value)
    }

    /// Numeric CR references and split CDATA keep XML's newline normalization from changing the value.
    pub(super) fn render(&self, value: &str) -> String {
        if value.is_empty()
            && let Some(empty) = &self.empty
        {
            return empty.clone();
        }
        let mut content = match &self.coding {
            Coding::Cdata => format!(
                "<![CDATA[{}]]>",
                value
                    .replace("]]>", "]]]]><![CDATA[>")
                    .replace('\r', "]]>&#13;<![CDATA[")
            ),
            Coding::Text {
                quotes,
                apostrophes,
                carriage,
            } => {
                let mut content = native_writer::escape(value);
                if *quotes {
                    content = content.replace('"', "&quot;");
                }
                if *apostrophes {
                    content = content.replace('\'', "&apos;");
                }
                match carriage {
                    Carriage::Decimal => {}
                    Carriage::UpperHex => content = content.replace("&#13;", "&#xD;"),
                    Carriage::LowerHex => content = content.replace("&#13;", "&#xd;"),
                }
                content
            }
        };
        match self.ending {
            Ending::Lf => {}
            Ending::CrLf => content = content.replace('\n', "\r\n"),
            Ending::Cr => content = content.replace('\n', "\r"),
        }
        format!("{}{content}{}", self.open, self.close)
    }
}

/// Logical query EOL is decoded by XML; uniform native transport is retained independently.
fn ending(input: &str) -> Result<Ending, Error> {
    if input.contains("\r\n") {
        let rest = input.replace("\r\n", "");
        if rest.contains(['\r', '\n']) {
            return Err(unverified());
        }
        Ok(Ending::CrLf)
    } else if input.contains('\r') {
        if input.contains('\n') {
            return Err(unverified());
        }
        Ok(Ending::Cr)
    } else {
        Ok(Ending::Lf)
    }
}

/// Only styles whose full inverse agrees with the parsed value are accepted by the caller.
fn coding(input: &str) -> Coding {
    if input.starts_with("<![CDATA[") && input.ends_with("]]>") {
        Coding::Cdata
    } else {
        Coding::Text {
            quotes: input.contains("&quot;"),
            apostrophes: input.contains("&apos;"),
            carriage: if input.contains("&#xD;") {
                Carriage::UpperHex
            } else if input.contains("&#xd;") {
                Carriage::LowerHex
            } else {
                Carriage::Decimal
            },
        }
    }
}

/// Unknown lexical shapes remain an explicit refusal before a query becomes separately editable.
fn unverified() -> Error {
    Error::root("source_query_lexical_unverified")
}
