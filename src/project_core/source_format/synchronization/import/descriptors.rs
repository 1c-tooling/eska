//! Disjoint descriptor values merge before the same pinned protected selector checks permissions.

mod cst;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{
    Error, FileEntry, SelectedFile, SourceImportConflictKind, SourceImportPlan, files, selection,
    storage,
};
use crate::project_core::source_format::{FileRole, ObjectSource, yaml};

/// Only verified owner slots use semantic merging; modules, queries and binary files stay atomic.
pub(super) fn resolve(plan: &mut SourceImportPlan) -> Result<(), Error> {
    if plan
        .conflicts
        .iter()
        .any(|conflict| conflict.kind == SourceImportConflictKind::MappingUnsupported)
        || !plan
            .conflicts
            .iter()
            .any(|conflict| conflict.kind == SourceImportConflictKind::BothModified)
    {
        return Ok(());
    }
    let mut merged = false;
    let mut remaining = Vec::new();
    for mut conflict in std::mem::take(&mut plan.conflicts) {
        let change = plan.changes.iter().find(|change| {
            change.identity.starts_with("owner:")
                && change
                    .selected
                    .as_ref()
                    .is_some_and(|entry| entry.path == conflict.path)
        });
        let Some(change) =
            change.filter(|_| conflict.kind == SourceImportConflictKind::BothModified)
        else {
            remaining.push(conflict);
            continue;
        };
        let (Some(before), Some(after)) = (&change.ancestor, &change.incoming) else {
            remaining.push(conflict);
            continue;
        };
        let old_text = files::read_text(
            &plan
                .ancestor
                .root
                .join(storage::blob_path(before)?.as_path()),
        )?;
        let new_text = files::read_text(
            &plan
                .incoming
                .root
                .join(storage::blob_path(after)?.as_path()),
        )?;
        let working = plan.read(&conflict.path)?;
        match merge(&old_text, &new_text, &working)? {
            Outcome::Merged(text) => {
                let entry = FileEntry {
                    path: conflict.path.clone(),
                    bytes: text.len() as u64,
                    sha256: files::hex(&Sha256::digest(text.as_bytes())),
                    role: FileRole::Preserved,
                };
                plan.selected
                    .insert(conflict.path, SelectedFile::Merged { entry, text });
                merged = true;
            }
            Outcome::Conflict(coordinate) => {
                conflict.coordinate = coordinate;
                remaining.push(conflict);
            }
            Outcome::Unsupported => remaining.push(conflict),
        }
    }
    plan.conflicts = remaining;
    if !merged {
        return Ok(());
    }
    plan.output.files = plan
        .selected
        .iter()
        .map(|(path, choice)| (path.clone(), selection::selected_entry(path, choice)))
        .collect();
    plan.output.revision = files::revision(&plan.output.files, &plan.output.directories);
    for change in &mut plan.changes {
        if let Some(entry) = &change.selected {
            change.selected = plan.output.files.get(&entry.path).cloned();
        }
    }
    Ok(())
}

/// Unsupported concrete syntax is a reviewable file conflict, never a serializer fallback.
enum Outcome {
    Merged(String),
    Conflict(Vec<String>),
    Unsupported,
}

/// Preserve working trivia and copy only incoming value subtrees that have no local value change.
fn merge(ancestor: &str, incoming: &str, working: &str) -> Result<Outcome, Error> {
    let old = yaml::decode(ancestor)?;
    let (new, incoming_tree) = yaml::decode_with_tree(incoming)?;
    let (local, working_tree) = yaml::decode_with_tree(working)?;
    let mut patches = Vec::new();
    let merged = match select(&old, &new, &local, &mut Vec::new(), &mut patches) {
        Ok(value) => value,
        Err(coordinate) => return Ok(Outcome::Conflict(coordinate)),
    };
    ObjectSource::from_value(merged.clone())?;
    let Some(output) = cst::splice(working, incoming, &working_tree, &incoming_tree, &patches)
    else {
        return Ok(Outcome::Unsupported);
    };
    if yaml::decode(&output).ok().as_ref() != Some(&merged)
        || !comments_preserved(working, incoming, &output)
    {
        return Ok(Outcome::Unsupported);
    }
    Ok(Outcome::Merged(output))
}

/// Maps expose independent properties; arrays and preserved scalar payloads remain whole values.
fn select(
    old: &Value,
    new: &Value,
    local: &Value,
    coordinate: &mut Vec<String>,
    patches: &mut Vec<Vec<String>>,
) -> Result<Value, Vec<String>> {
    if new == old || new == local {
        return Ok(local.clone());
    }
    if let (Value::Object(old), Value::Object(new), Value::Object(local)) = (old, new, local)
        && old.keys().eq(new.keys())
        && old.keys().eq(local.keys())
    {
        let mut selected = serde_json::Map::new();
        for (key, before) in old {
            coordinate.push(key.clone());
            let value = select(before, &new[key], &local[key], coordinate, patches)?;
            coordinate.pop();
            selected.insert(key.clone(), value);
        }
        return Ok(Value::Object(selected));
    }
    if local == old {
        patches.push(coordinate.clone());
        return Ok(new.clone());
    }
    Err(coordinate.clone())
}

/// Exact comment multiplicity refuses a merge that would remove or duplicate either annotation.
fn comments_preserved(working: &str, incoming: &str, output: &str) -> bool {
    let mut expected = comments(working);
    for (comment, count) in comments(incoming) {
        let current = expected.entry(comment).or_default();
        *current = (*current).max(count);
    }
    comments(output) == expected
}

/// Comments are lexical tokens, so XML strings containing a hash are not YAML annotations.
fn comments(input: &str) -> BTreeMap<&str, usize> {
    let mut result = BTreeMap::new();
    for (kind, token) in yaml_edit::lex(input) {
        if kind == yaml_edit::SyntaxKind::COMMENT {
            *result.entry(token).or_default() += 1;
        }
    }
    result
}
