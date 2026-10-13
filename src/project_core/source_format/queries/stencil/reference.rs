//! Versioned recipes carry only lexical style; native headers and sole query values remain separate.

use roxmltree::Node;

use super::{Carriage, Coding, Ending, Error, Frame};
use crate::project_core::source_format::{
    SourcePath, lexical, native_writer,
    queries::{QUERY_REFERENCE_NAMESPACE, references::invalid},
};

impl Frame {
    /// The protocol namespace is local to the marker so it disappears entirely during materialization.
    pub(in crate::project_core::source_format::queries) fn reference(
        &self,
        path: &SourcePath,
    ) -> String {
        let (coding, quotes, apostrophes, carriage) = match &self.coding {
            Coding::Text {
                quotes,
                apostrophes,
                carriage,
            } => ("text", *quotes, *apostrophes, carriage),
            Coding::Cdata => ("cdata", false, false, &Carriage::Decimal),
        };
        let eol = match self.ending {
            Ending::Lf => "lf",
            Ending::CrLf => "crlf",
            Ending::Cr => "cr",
        };
        let cr = match carriage {
            Carriage::Decimal => "13",
            Carriage::UpperHex => "xD",
            Carriage::LowerHex => "xd",
        };
        let source = native_writer::escape(path.as_str());
        let empty = self.empty.is_some();
        format!(
            "{}<eska:query xmlns:eska=\"{QUERY_REFERENCE_NAMESPACE}\" source=\"{source}\" coding=\"{coding}\" eol=\"{eol}\" cr=\"{cr}\" quotes=\"{quotes}\" apostrophes=\"{apostrophes}\" empty=\"{empty}\"/>{}",
            self.open, self.close
        )
    }

    /// Parent headers preserve native spelling; the exact recipe is decoded independently of source values.
    pub(in crate::project_core::source_format::queries) fn from_reference(
        parent: Node<'_, '_>,
        marker: Node<'_, '_>,
        input: &str,
    ) -> Result<Self, Error> {
        if parent.attributes().len() != 0 {
            return Err(invalid());
        }
        let range = parent.range();
        let original = input.get(range.clone()).ok_or_else(invalid)?;
        let start = lexical::opening_end(input, range.clone()).ok_or_else(invalid)?;
        let end = range.start + original.rfind("</").ok_or_else(invalid)?;
        let open: String = input.get(range.start..start).ok_or_else(invalid)?.into();
        let close = input.get(end..range.end).ok_or_else(invalid)?.into();
        let empty = if boolean(marker, "empty")? {
            Some(format!("{}/>", open.strip_suffix('>').ok_or_else(invalid)?))
        } else {
            None
        };
        let ending = match marker.attribute("eol") {
            Some("lf") => Ending::Lf,
            Some("crlf") => Ending::CrLf,
            Some("cr") => Ending::Cr,
            _ => return Err(invalid()),
        };
        let carriage = match marker.attribute("cr") {
            Some("13") => Carriage::Decimal,
            Some("xD") => Carriage::UpperHex,
            Some("xd") => Carriage::LowerHex,
            _ => return Err(invalid()),
        };
        let quotes = boolean(marker, "quotes")?;
        let apostrophes = boolean(marker, "apostrophes")?;
        let coding = match marker.attribute("coding") {
            Some("text") => Coding::Text {
                quotes,
                apostrophes,
                carriage,
            },
            Some("cdata") if !quotes && !apostrophes && carriage == Carriage::Decimal => {
                Coding::Cdata
            }
            _ => return Err(invalid()),
        };
        Ok(Self {
            range,
            open,
            close,
            empty,
            ending,
            coding,
        })
    }
}

/// Source recipe flags have one locale-independent spelling and never use truthy coercion.
fn boolean(node: Node<'_, '_>, name: &str) -> Result<bool, Error> {
    match node.attribute(name) {
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        _ => Err(invalid()),
    }
}
