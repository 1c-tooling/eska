//! Walk large payloads in document order; parse only candidate fragments with their real ancestors.

mod validation;

use std::ops::Range;

use quick_xml::{
    Reader,
    events::{BytesStart, Event},
    name::NamespaceResolver,
};

use super::{ReferenceRename, XmlAnalysis};
use crate::project::metadata_edit::EditError;

/// Only the current ancestor chain and relevant direct text survive each streaming event.
struct Frame {
    opening: Range<usize>,
    name: String,
    children: bool,
    pending: Option<Range<usize>>,
    text: Vec<Range<usize>>,
}

impl Frame {
    /// Join adjacent text/entity/CDATA events without normalizing their original byte spans.
    const fn text(&mut self, range: Range<usize>) {
        match &mut self.pending {
            Some(pending) => pending.end = range.end,
            None => self.pending = Some(range),
        }
    }

    /// Whitespace and unrelated text never accumulate in the ancestor stack of a large table.
    fn flush(&mut self, input: &str, rename: &ReferenceRename) {
        if let Some(range) = self.pending.take()
            && rename.may_mention(&input[range.clone()])
        {
            self.text.push(range);
        }
    }
}

/// Per-document state bounds namespace and element tracking by the current ancestor chain.
struct Walker<'a> {
    input: &'a str,
    rename: &'a ReferenceRename,
    stack: Vec<Frame>,
    roots: usize,
    result: XmlAnalysis,
    validator: validation::Validator,
    namespaces: NamespaceResolver,
}

impl Walker<'_> {
    /// Validate bindings before analyzing attributes, then enter only nonempty elements.
    fn open(&mut self, start: &BytesStart<'_>, empty: bool, end: usize) -> Result<(), EditError> {
        let range = end - start.len() - if empty { 3 } else { 2 }..end;
        let candidate = self
            .validator
            .start(&mut self.namespaces, start, self.rename)?;
        if let Some(parent) = self.stack.last_mut() {
            parent.flush(self.input, self.rename);
            parent.children = true;
        } else {
            self.roots += 1;
        }
        if self.roots > 1 || self.stack.len() >= 64 {
            return Err(EditError::InvalidXml);
        }
        if candidate {
            let closing = if empty {
                String::new()
            } else {
                format!("</{}>", start.name().into_inner())
            };
            self.rename.fragment(
                self.input,
                &self.stack,
                range.clone(),
                &closing,
                range.clone(),
                &mut self.result,
            )?;
        }
        if empty {
            self.namespaces.pop();
        } else {
            self.stack.push(Frame {
                opening: range,
                name: start.name().into_inner().to_owned(),
                children: false,
                pending: None,
                text: Vec::new(),
            });
        }
        Ok(())
    }

    /// A leaf keeps its real XML context; mixed content never becomes a synthetic scalar reference.
    fn close(&mut self, end: usize, closing_size: usize) -> Result<(), EditError> {
        let mut frame = self.stack.pop().ok_or(EditError::InvalidXml)?;
        frame.flush(self.input, self.rename);
        if frame.children {
            self.rename
                .mixed_text(self.input, &frame.text, &mut self.result)?;
        } else if !frame.text.is_empty() {
            self.rename.fragment(
                self.input,
                &self.stack,
                frame.opening.start..end,
                "",
                frame.opening.end..end - closing_size,
                &mut self.result,
            )?;
        }
        self.namespaces.pop();
        Ok(())
    }

    /// Character references and CDATA are legal only inside the document element.
    fn text(&mut self, range: Range<usize>) -> Result<(), EditError> {
        self.stack
            .last_mut()
            .ok_or(EditError::InvalidXml)?
            .text(range);
        Ok(())
    }
}

impl ReferenceRename {
    /// Validate the full token stream, preserving namespaces when inspecting small candidate subtrees.
    pub(super) fn analyze_streaming(&self, input: &str) -> Result<XmlAnalysis, EditError> {
        validation::characters(input)?;
        // quick-xml removes the BOM without counting its bytes in buffer_position().
        let body = input.strip_prefix('\u{feff}').unwrap_or(input);
        let bom = input.len() - body.len();
        if body.starts_with('\u{feff}') {
            return Err(EditError::InvalidXml);
        }
        let mut reader = Reader::from_str(body);
        reader.config_mut().check_comments = true;
        let mut walk = Walker {
            input,
            rename: self,
            stack: Vec::new(),
            roots: 0,
            result: XmlAnalysis::default(),
            validator: validation::Validator::default(),
            namespaces: NamespaceResolver::default(),
        };
        loop {
            let event = reader.read_event().map_err(|_| EditError::InvalidXml)?;
            let end =
                usize::try_from(reader.buffer_position()).map_err(|_| EditError::InvalidXml)? + bom;
            match event {
                Event::Start(ref start) | Event::Empty(ref start) => {
                    walk.open(start, matches!(event, Event::Empty(_)), end)?;
                }
                Event::End(close) => walk.close(end, close.len() + 3)?,
                Event::Text(text) => {
                    if text.contains("]]>") {
                        return Err(EditError::InvalidXml);
                    }
                    if let Some(frame) = walk.stack.last_mut() {
                        frame.text(end - text.len()..end);
                    } else if !text
                        .chars()
                        .all(|character| matches!(character, ' ' | '\t' | '\r' | '\n'))
                    {
                        return Err(EditError::InvalidXml);
                    }
                }
                Event::CData(text) => walk.text(end - text.len() - 12..end)?,
                Event::GeneralRef(reference) => {
                    validation::entity(&reference)?;
                    walk.text(end - reference.len() - 2..end)?;
                }
                Event::Comment(_) | Event::PI(_) => {
                    if let Event::PI(instruction) = &event {
                        validation::Validator::instruction(instruction.target())?;
                    }
                    if let Some(frame) = walk.stack.last_mut() {
                        frame.flush(input, self);
                    }
                }
                Event::Decl(declaration) => {
                    if end - declaration.len() - 4 != bom {
                        return Err(EditError::InvalidXml);
                    }
                    if let Some(value) = declaration.standalone()
                        && !matches!(value.as_deref(), Ok("yes" | "no"))
                    {
                        return Err(EditError::InvalidXml);
                    }
                    roxmltree::Document::parse(&format!("{}<r/>", &input[..end]))
                        .map_err(|_| EditError::InvalidXml)?;
                }
                Event::Eof if walk.stack.is_empty() && walk.roots == 1 => break,
                Event::Eof | Event::DocType(_) => return Err(EditError::InvalidXml),
            }
        }
        walk.result
            .replacements
            .sort_by_key(|change| change.range.start);
        walk.result
            .uncertain
            .sort_by_key(|candidate| candidate.range.start);
        Ok(walk.result)
    }

    /// Retain real ancestor tags so namespace rebinding and metadata class context remain identical.
    fn fragment(
        &self,
        input: &str,
        ancestors: &[Frame],
        source: Range<usize>,
        closing: &str,
        allowed: Range<usize>,
        result: &mut XmlAnalysis,
    ) -> Result<(), EditError> {
        let prefix_size: usize = ancestors.iter().map(|frame| frame.opening.len()).sum();
        if prefix_size.saturating_add(source.len()) > 64 * 1024 * 1024 {
            return Err(EditError::InvalidXml);
        }
        let mut fragment = String::with_capacity(prefix_size + source.len() + closing.len());
        for frame in ancestors {
            fragment.push_str(&input[frame.opening.clone()]);
        }
        fragment.push_str(&input[source.clone()]);
        fragment.push_str(closing);
        for frame in ancestors.iter().rev() {
            fragment.push_str("</");
            fragment.push_str(&frame.name);
            fragment.push('>');
        }
        let analysis = self.analyze_dom(&fragment, &[])?;
        for mut change in analysis.replacements {
            if let Some(range) =
                source_range(change.range.clone(), prefix_size, source.start, &allowed)
            {
                change.range = range;
                result.replacements.push(change);
            }
        }
        for mut candidate in analysis.uncertain {
            if let Some(range) =
                source_range(candidate.range.clone(), prefix_size, source.start, &allowed)
            {
                candidate.range = range;
                result.uncertain.push(candidate);
            }
        }
        Ok(())
    }

    /// Mixed-content elements cannot establish a scalar binding by omitting their child elements.
    fn mixed_text(
        &self,
        input: &str,
        ranges: &[Range<usize>],
        result: &mut XmlAnalysis,
    ) -> Result<(), EditError> {
        for range in ranges {
            let fragment = format!("<r>{}</r>", &input[range.clone()]);
            let parsed =
                roxmltree::Document::parse(&fragment).map_err(|_| EditError::InvalidXml)?;
            let value: String = parsed
                .root_element()
                .children()
                .filter_map(|node| node.text())
                .collect();
            self.collect(input, range.clone(), &value, None, result);
        }
        Ok(())
    }
}

/// Synthetic ancestor coordinates never escape into a file replacement or duplicate an attribute hit.
fn source_range(
    range: Range<usize>,
    prefix: usize,
    start: usize,
    allowed: &Range<usize>,
) -> Option<Range<usize>> {
    let mapped = range.start.checked_sub(prefix)?.checked_add(start)?
        ..range.end.checked_sub(prefix)?.checked_add(start)?;
    (mapped.start >= allowed.start && mapped.end <= allowed.end).then_some(mapped)
}
