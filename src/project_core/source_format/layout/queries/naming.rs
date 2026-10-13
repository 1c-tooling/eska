//! Deterministic initial filenames do not serve as native identity after references have been created.

use std::collections::BTreeMap;

use crate::project_core::source_format::{Error, NativeQueryDocument, QuerySlot, SourcePath};

/// Native scoped names are encoded component by component; form attributes already have stable IDs.
pub(super) fn paths(
    descriptor: &SourcePath,
    document: &NativeQueryDocument,
) -> Result<BTreeMap<QuerySlot, SourcePath>, Error> {
    let prefix = descriptor
        .as_str()
        .strip_suffix(".xml")
        .ok_or_else(super::invalid)?;
    document
        .queries()
        .iter()
        .map(|query| {
            let name = match &query.slot {
                QuerySlot::DynamicList { attribute } => format!("Attribute_{}", attribute.as_str()),
                QuerySlot::Composition { datasets } => datasets
                    .iter()
                    .map(|name| format!("Query_{}", component(name)))
                    .collect::<Vec<_>>()
                    .join("/"),
            };
            Ok((
                query.slot.clone(),
                SourcePath::parse(&format!("{prefix}/Queries/{name}.sdbl"))?,
            ))
        })
        .collect()
}

/// Encoding is injective and portable; prefixes prevent Windows device basenames without truncation.
fn component(name: &str) -> String {
    let mut output = String::new();
    for character in name.chars() {
        if character.is_alphanumeric() || character == '_' {
            output.push(character);
        } else {
            let mut bytes = [0; 4];
            for byte in character.encode_utf8(&mut bytes).bytes() {
                output.push('%');
                output.push(char::from(b"0123456789ABCDEF"[usize::from(byte >> 4)]));
                output.push(char::from(b"0123456789ABCDEF"[usize::from(byte & 15)]));
            }
        }
    }
    output
}
