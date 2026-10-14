//! Recreate query source links through stable owner slots, without changing their native values.

use crate::project_core::source_format::{
    AcceptedBase, Error, NativeQueryDocument, QueryReferenceDocument, SourcePath,
};
use std::collections::BTreeMap;

/// Query frames and values stay pinned while each explicit link follows its verified path transition.
pub(super) fn rebind(
    base: &AcceptedBase,
    input: &str,
    mappings: &BTreeMap<&SourcePath, &SourcePath>,
) -> Result<String, Error> {
    let document = QueryReferenceDocument::from_xml(input)?;
    if document.references().is_empty() {
        return Ok(input.to_owned());
    }
    let paths = document
        .references()
        .iter()
        .map(|reference| {
            let path = mappings
                .get(&reference.source)
                .ok_or_else(|| Error::root("source_file_unmapped"))?;
            Ok((reference.slot.clone(), (**path).clone()))
        })
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
    let native = document.render(|reference| {
        base.package
            .query_inputs
            .values
            .get(&reference.source)
            .map(String::as_str)
    })?;
    Ok(NativeQueryDocument::from_xml(&native)?
        .extract(&paths)?
        .into_parts()
        .0)
}
