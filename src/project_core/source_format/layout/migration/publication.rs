//! A receipt owns incomplete output; staging files are linked without replacing authoring files.

use super::{
    DescriptorLayoutPlan, Error, Inventory, LayoutPublication, Path, PathBuf, SourcePath, files, fs,
};
use serde::{Deserialize, Serialize};

pub(super) const RECEIPT: &str = super::super::PENDING_RECEIPT;
pub(super) const TEMPORARY: &str = super::super::PENDING_WRITE;

/// Recovery is tied to complete before/after revisions, never to a caller-supplied directory name.
#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema_version: u32,
    input_revision: String,
    output_revision: String,
}

/// Recheck before every destination decision; completed output is never rewritten.
pub(super) fn publish(
    plan: &DescriptorLayoutPlan,
    destination: &Path,
) -> Result<LayoutPublication, Error> {
    plan.recheck()?;
    let destination = destination_path(plan, destination)?;
    let expected = &plan.projection.snapshot;
    let receipt = receipt(plan);
    let status = match fs::symlink_metadata(&destination) {
        Ok(metadata) if metadata.file_type().is_dir() => {
            let mut current = Inventory::read(&destination)?;
            if current == *expected {
                plan.recheck()?;
                return Ok(LayoutPublication::Unchanged);
            }
            if !current.files.contains_key(&SourcePath::parse(RECEIPT)?) {
                return Err(Error::root("source_destination_exists"));
            }
            let stored: Receipt =
                serde_json::from_str(&files::read_text(&destination.join(RECEIPT))?)
                    .map_err(|_| Error::root("source_layout_receipt_invalid"))?;
            if stored != receipt {
                return Err(Error::root("source_layout_receipt_invalid"));
            }
            current.files.remove(&SourcePath::parse(RECEIPT)?);
            current.files.remove(&SourcePath::parse(TEMPORARY)?);
            validate_partial(&current, expected)?;
            if destination.join(TEMPORARY).exists() {
                // This reserved operation-owned file was never published as an authoring input.
                fs::remove_file(destination.join(TEMPORARY)).map_err(|error| files::io(&error))?;
            }
            LayoutPublication::Resumed
        }
        Ok(_) => return Err(Error::root("source_destination_exists")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&destination).map_err(|error| files::io(&error))?;
            let text =
                serde_json::to_vec(&receipt).map_err(|_| Error::root("source_schema_invalid"))?;
            files::write(&destination.join(RECEIPT), &text)?;
            LayoutPublication::Created
        }
        Err(error) => return Err(files::io(&error)),
    };
    for path in &expected.directories {
        fs::create_dir_all(destination.join(path.as_path())).map_err(|error| files::io(&error))?;
    }
    let manifest = SourcePath::parse(files::MANIFEST)?;
    for (path, entry) in expected.files.iter().filter(|(path, _)| **path != manifest) {
        stage_file(plan, &destination, path, entry)?;
    }
    plan.recheck()?;
    let entry = expected
        .files
        .get(&manifest)
        .ok_or_else(|| Error::root("source_descriptor_missing"))?;
    stage_file(plan, &destination, &manifest, entry)?;
    let mut current = Inventory::read(&destination)?;
    current.files.remove(&SourcePath::parse(RECEIPT)?);
    if current.files != expected.files || current.directories != expected.directories {
        return Err(Error::root("source_layout_destination_mismatch"));
    }
    plan.recheck()?;
    fs::remove_file(destination.join(RECEIPT)).map_err(|error| files::io(&error))?;
    if Inventory::read(&destination)? != *expected {
        return Err(Error::root("source_layout_destination_mismatch"));
    }
    Ok(status)
}

/// Each final name is installed atomically without replacing a file; only private staging may fail.
fn stage_file(
    plan: &DescriptorLayoutPlan,
    destination: &Path,
    path: &SourcePath,
    entry: &super::FileEntry,
) -> Result<(), Error> {
    if fs::symlink_metadata(destination.join(path.as_path())).is_ok() {
        return Ok(());
    }
    let temporary = destination.join(TEMPORARY);
    if let Some(text) = plan.projection.text.get(path) {
        files::write(&temporary, text.as_bytes())?;
    } else {
        files::copy_to(&plan.root, &temporary, entry)?;
    }
    fs::hard_link(&temporary, destination.join(path.as_path()))
        .map_err(|error| files::io(&error))?;
    fs::remove_file(&temporary).map_err(|error| files::io(&error))?;
    Ok(())
}

/// An edited committed file, extra directory or foreign file cannot be mistaken for interruption.
fn validate_partial(current: &Inventory, expected: &Inventory) -> Result<(), Error> {
    if current
        .files
        .iter()
        .any(|(path, file)| expected.files.get(path) != Some(file))
        || current
            .directories
            .iter()
            .any(|path| expected.directories.binary_search(path).is_err())
    {
        return Err(Error::root("source_layout_destination_mismatch"));
    }
    Ok(())
}

/// Resolve an existing parent while refusing aliases and writes into either side of the input tree.
fn destination_path(plan: &DescriptorLayoutPlan, destination: &Path) -> Result<PathBuf, Error> {
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::root("source_path_invalid"))?;
    SourcePath::parse(name)?;
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let destination = fs::canonicalize(parent)
        .map_err(|error| files::io(&error))?
        .join(name);
    if destination == plan.root && plan.is_empty() {
        return Ok(destination);
    }
    if destination.starts_with(&plan.root) || plan.root.starts_with(&destination) {
        return Err(Error::root("source_tree_overlap"));
    }
    Ok(destination)
}

/// A bounded operation receipt contains no machine-specific input/output paths.
fn receipt(plan: &DescriptorLayoutPlan) -> Receipt {
    Receipt {
        schema_version: 1,
        input_revision: plan.input_revision().into(),
        output_revision: plan.output_revision().into(),
    }
}

#[cfg(test)]
/// Simulate a process stop before ready publication without adding a production fault-injection API.
pub(in crate::project_core::source_format::layout) fn interrupt(
    plan: &DescriptorLayoutPlan,
    destination: &Path,
) {
    fs::create_dir(destination).unwrap();
    files::write(
        &destination.join(RECEIPT),
        &serde_json::to_vec(&receipt(plan)).unwrap(),
    )
    .unwrap();
}
