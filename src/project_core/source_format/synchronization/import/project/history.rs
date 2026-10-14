//! Migration copies raw history as data; independently supplied pins still select import authority.

use super::{
    AcceptedBase, Error, ImportAncestor, Inventory, Path, SERVICE, STATE, SourcePath,
    checked_directory, read_optional, read_state,
};

/// Inspect existing raw state for explicit project attachment without selecting an ancestry pin.
///
/// # Errors
/// Returns malformed/changed storage or a raw snapshot with a different independently pinned identity.
pub fn verify_project_import_history(
    root: &Path,
    source: &SourcePath,
    approved: &AcceptedBase,
) -> Result<(), Error> {
    verified_project_import_history(root, source, approved).map(|_| ())
}

/// Validate the active snapshot and capture every raw byte without approving ancestry or recovery.
///
/// Current published and initial pointers keep their own formats. The returned inventory grants
/// copying permission only; later source import still requires the caller's independent digest.
pub(in crate::project_core::source_format) fn verified_project_import_history(
    root: &Path,
    source: &SourcePath,
    approved: &AcceptedBase,
) -> Result<Inventory, Error> {
    let directory = root.join(SERVICE);
    checked_directory(&directory)?;
    let original = Inventory::read(&directory)?;
    let text = read_optional(&root.join(STATE))?
        .ok_or_else(|| Error::root("source_import_state_invalid"))?;
    let state = read_state(root, source, &text)?;
    let ancestor = ImportAncestor::open(&state.store(root), state.ancestor())?;
    super::super::compatible(&approved.package, &ancestor.package)?;
    if Inventory::read(&directory)? != original
        || read_optional(&root.join(STATE))?.as_deref() != Some(&text)
    {
        return Err(Error::root("source_input_changed"));
    }
    Ok(original)
}
