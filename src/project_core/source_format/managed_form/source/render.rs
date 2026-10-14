//! Validated flat stencils render with an explicit work stack, retaining each opaque chunk once.

use super::context::{Fragment, ReferenceView};
use super::{ChildFrame, Error, FormId, ManagedFormSource, Part, binding, property};

enum Task<'a> {
    Part(&'a Part, Option<&'a FormId>, usize),
    Element(&'a FormId),
    Text(&'a str),
}

/// Single-parent validation precedes expansion, so cyclic or duplicated graphs never enter the render stack.
pub(super) fn render(source: &ManagedFormSource) -> Result<String, Error> {
    render_reference(source, u64::MAX, false).map(|view| view.xml)
}

/// Context captures only preserved fragments; generated values never acquire guessed source offsets.
pub(super) fn render_reference(
    source: &ManagedFormSource,
    max_bytes: u64,
    track: bool,
) -> Result<ReferenceView, Error> {
    source.preflight()?;
    let mut stack: Vec<_> = source
        .native
        .root
        .iter()
        .enumerate()
        .rev()
        .map(|(index, part)| Task::Part(part, None, index))
        .collect();
    let mut output = String::new();
    let mut fragments = Vec::new();
    while let Some(task) = stack.pop() {
        match task {
            Task::Text(text) => output.push_str(text),
            Task::Part(Part::Xml { value }, owner, index) => {
                let start = output.len();
                output.push_str(value.as_str());
                if track && !value.is_empty() {
                    fragments.push(Fragment {
                        native: start..output.len(),
                        owner: owner.cloned(),
                        index,
                    });
                }
            }
            Task::Element(id) | Task::Part(Part::Element { id }, _, _) => {
                let stencil = source.native.elements.get(id).ok_or_else(binding)?;
                stack.extend(
                    stencil
                        .parts
                        .iter()
                        .enumerate()
                        .rev()
                        .map(|(index, part)| Task::Part(part, Some(id), index)),
                );
            }
            Task::Part(Part::Name, owner, _) => {
                let definition = source
                    .elements
                    .get(owner.ok_or_else(binding)?)
                    .ok_or_else(binding)?;
                output.push_str(&attribute(&definition.name));
            }
            Task::Part(Part::Id, owner, _) => output.push_str(owner.ok_or_else(binding)?.as_str()),
            Task::Part(Part::Property { name, stencil }, owner, _) => {
                let value = source
                    .property_values(owner)?
                    .get(name)
                    .ok_or_else(binding)?;
                output.push_str(&property::render(stencil, value)?);
            }
            Task::Part(Part::Class { prefix }, owner, _) => {
                let definition = source
                    .elements
                    .get(owner.ok_or_else(binding)?)
                    .ok_or_else(binding)?;
                output.push_str(prefix);
                output.push_str(definition.kind.native_name());
            }
            Task::Part(Part::Children { frame }, owner, _) => {
                let children = if let Some(owner) = owner {
                    source
                        .layout
                        .children
                        .get(owner)
                        .ok_or_else(binding)?
                        .as_slice()
                } else {
                    &source.layout.roots
                };
                enqueue(frame, children, &mut stack);
            }
        }
        if output.len() as u64 > max_bytes {
            return Err(Error::root("source_document_too_large"));
        }
    }
    Ok(ReferenceView {
        xml: output,
        fragments,
    })
}

/// Original gaps stay at their owner even when a move changes its child count; no opaque comment is lost or cloned.
fn enqueue<'a>(frame: &'a ChildFrame, children: &'a [FormId], stack: &mut Vec<Task<'a>>) {
    if children.is_empty()
        && let Some(empty) = &frame.empty
    {
        stack.push(Task::Text(empty));
        return;
    }
    let mut tasks = vec![Task::Text(&frame.open)];
    for (index, id) in children.iter().enumerate() {
        if let Some(gap) = frame.gaps.get(index) {
            tasks.push(Task::Text(gap));
        }
        tasks.push(Task::Element(id));
    }
    tasks.extend(
        frame
            .gaps
            .iter()
            .skip(children.len())
            .map(|gap| Task::Text(gap)),
    );
    tasks.push(Task::Text(&frame.close));
    stack.extend(tasks.into_iter().rev());
}

/// Attribute escaping preserves XML normalization-sensitive characters regardless of the retained quote delimiter.
fn attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
        .replace('\r', "&#13;")
        .replace('\n', "&#10;")
        .replace('\t', "&#9;")
}
