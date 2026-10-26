//! Explicit preview-bound migration preserves the original and updates both configs under one lock.

use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

use eska_project_core::{application::AnalyzerPolicy, config::ManifestConfig};
use serde::Serialize;
use toml_edit::{DocumentMut, Item};

use crate::{
    Error,
    configuration::{self, Plan, Status},
    process::Analyzer,
    storage::{self, ConfigLock, Temporary},
};

/// Machine-readable preview names every managed decision and retains the complete imported policy.
#[derive(Serialize)]
pub struct Preview {
    pub schema_version: u32,
    pub token: String,
    pub conflicts: Vec<String>,
    pub policy: toml::Table,
    pub managed: toml::Table,
    pub proposed_manifest: String,
    pub generated_config: String,
    #[serde(skip)]
    base: Plan,
    #[serde(skip)]
    proposed: Plan,
}

/// Read the foreign config and construct a reviewable migration without changing either original.
///
/// # Errors
/// Rejects malformed TOML, managed files, unreadable inputs or invalid root manifests.
pub fn preview(start: &Path, application: Option<&str>) -> Result<Preview, Error> {
    let base = configuration::inspect(start, application)?;
    if base.status != Status::MigrationRequired {
        return Err(Error::MigrationNotRequired);
    }
    let original = base.previous.as_deref().ok_or(Error::StalePreview)?;
    let input = std::str::from_utf8(original).map_err(|error| Error::InvalidToml {
        path: base.path(),
        detail: error.to_string(),
    })?;
    let mut foreign: toml::Table = toml::from_str(input).map_err(|error| Error::InvalidToml {
        path: base.path(),
        detail: error.to_string(),
    })?;
    let generated = configuration::generate(&base.context, base.context.analyzer_policy())?;
    let expected: toml::Table = toml::from_str(&generated).map_err(|error| Error::InvalidToml {
        path: base.path(),
        detail: error.to_string(),
    })?;
    let (mut conflicts, managed) = extract_managed(base.context.root(), &mut foreign, &expected)?;
    let mut policy: toml::Table = toml::from_str(base.context.analyzer_policy().as_toml())
        .map_err(|error| Error::InvalidToml {
            path: base.context.root().join("eska.toml"),
            detail: error.to_string(),
        })?;
    merge(&mut policy, foreign, "tools.bsl-analyzer", &mut conflicts);
    conflicts.sort();
    conflicts.dedup();
    let validated = AnalyzerPolicy::parse(&policy).map_err(|_| Error::InvalidPolicy {
        field: "tools.bsl-analyzer".into(),
    })?;
    let manifest_path = base.context.root().join("eska.toml");
    let input = std::str::from_utf8(&base.manifest).map_err(|error| Error::InvalidToml {
        path: manifest_path.clone(),
        detail: error.to_string(),
    })?;
    let mut document = input
        .parse::<DocumentMut>()
        .map_err(|error| Error::InvalidToml {
            path: manifest_path.clone(),
            detail: error.to_string(),
        })?;
    let policy_document = validated
        .as_toml()
        .parse::<DocumentMut>()
        .map_err(|error| Error::InvalidToml {
            path: manifest_path.clone(),
            detail: error.to_string(),
        })?;
    let tools = document.entry("tools").or_insert_with(|| {
        let mut tools = toml_edit::Table::new();
        tools.set_implicit(true);
        Item::Table(tools)
    });
    if let Item::Value(toml_edit::Value::InlineTable(inline)) = tools {
        *tools = Item::Table(std::mem::take(inline).into_table());
    }
    let tools = tools.as_table_mut().ok_or_else(|| Error::InvalidPolicy {
        field: "tools".into(),
    })?;
    let mut policy_table = policy_document.into_table();
    policy_table.set_implicit(true);
    tools.insert("bsl-analyzer", Item::Table(policy_table));
    let proposed_manifest = document.to_string();
    ManifestConfig::from_toml(&proposed_manifest).map_err(|_| Error::InvalidPolicy {
        field: "eska.toml".into(),
    })?;
    let proposed = configuration::build(
        base.context.clone(),
        proposed_manifest.as_bytes().into(),
        Some(&validated),
    )?;
    let token = configuration::sha256(
        format!("{}:{}:{}", base.token, proposed.token, proposed_manifest).as_bytes(),
    );
    Ok(Preview {
        schema_version: 1,
        token,
        conflicts,
        policy,
        managed,
        proposed_manifest,
        generated_config: String::from_utf8_lossy(&proposed.contents).into_owned(),
        base,
        proposed,
    })
}

/// Validate all preserved upstream policy fields during preview, before either original is changed.
///
/// # Errors
/// Returns the selected executable's schema diagnostics, timeout or cancellation.
pub fn validate_preview(preview: &Preview, analyzer: &Analyzer<'_>) -> Result<(), Error> {
    let root = preview.base.context.root();
    analyzer.validate(root)?;
    analyzer.check_config(root, &preview.base.path())?;
    let candidate = Temporary::new(root, &preview.proposed.contents)?;
    analyzer.check_config(root, candidate.path())?;
    configuration::ensure_snapshot(&preview.base)
}

/// Validate the exact proposed config, retain the original, and publish both files or roll back our edit.
///
/// # Errors
/// Rejects stale/mismatched preview tokens, unresolved decisions, process failures and active clients.
pub fn apply(
    preview: &Preview,
    token: &str,
    analyzer: &Analyzer<'_>,
) -> Result<(Plan, PathBuf), Error> {
    if token != preview.token {
        return Err(Error::StalePreview);
    }
    if !preview.conflicts.is_empty() {
        return Err(Error::MigrationConflicts {
            fields: preview.conflicts.clone(),
        });
    }
    let root = preview.base.context.root();
    let _lock = ConfigLock::exclusive(root)?;
    configuration::ensure_snapshot(&preview.base)?;
    analyzer.validate(root)?;
    analyzer.check_config(root, &preview.base.path())?;
    let candidate = Temporary::new(root, &preview.proposed.contents)?;
    analyzer.check_config(root, candidate.path())?;
    configuration::ensure_snapshot(&preview.base)?;
    if analyzer.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let original = preview
        .base
        .previous
        .as_deref()
        .ok_or(Error::StalePreview)?;
    let backup = root.join(format!(
        "bsl-analyzer.toml.eska-original-{}",
        &configuration::sha256(original)[..16]
    ));
    save_original(&backup, original)?;
    publish_pair(preview, candidate)?;
    Ok((
        configuration::inspect(root, preview.base.ownership.context.as_deref())?,
        backup,
    ))
}

/// Never replace an existing backup; identical originals can be reused after a failed attempt.
fn save_original(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    if let Some(previous) = storage::read_optional(path)? {
        return if previous == bytes {
            Ok(())
        } else {
            Err(Error::ManagedConflict { path: path.into() })
        };
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| Error::Io {
            path: path.into(),
            source,
        })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| Error::Io {
            path: path.into(),
            source,
        })
}

/// Merge every policy leaf, retaining conflicting existing ESKA values for explicit user choice.
fn merge(target: &mut toml::Table, source: toml::Table, prefix: &str, conflicts: &mut Vec<String>) {
    for (key, value) in source {
        let path = format!("{prefix}.{key}");
        match target.get_mut(&key) {
            None => {
                target.insert(key, value);
            }
            Some(toml::Value::Table(existing)) if value.is_table() => {
                if let toml::Value::Table(value) = value {
                    merge(existing, value, &path, conflicts);
                }
            }
            Some(existing) if *existing == value => {}
            Some(_) => conflicts.push(path),
        }
    }
}

/// Canonicalize equivalent local spellings while preserving declared dependency semantics.
fn source_matches(
    root: &Path,
    field: &str,
    old: &toml::Value,
    expected: Option<&toml::Value>,
) -> bool {
    match field {
        "root" => old
            .as_str()
            .zip(expected.and_then(toml::Value::as_str))
            .is_some_and(|(old, new)| same_path(root, old, new)),
        "extensions" | "externals" => {
            match (old.as_array(), expected.and_then(toml::Value::as_array)) {
                (Some(old), Some(expected)) => entries_match(root, old, expected),
                _ => false,
            }
        }
        _ => false,
    }
}

/// Match by canonical path rather than introducing a second set of member aliases.
fn entries_match(root: &Path, old: &[toml::Value], expected: &[toml::Value]) -> bool {
    if old.len() != expected.len() {
        return false;
    }
    let mut aliases = BTreeMap::new();
    let mut matched = Vec::new();
    for entry in old {
        let path = entry
            .as_str()
            .or_else(|| entry.get("path").and_then(toml::Value::as_str));
        let Some(path) = path else {
            return false;
        };
        let Some((index, expected)) = expected.iter().enumerate().find(|(_, expected)| {
            expected
                .get("path")
                .and_then(toml::Value::as_str)
                .is_some_and(|new| same_path(root, path, new))
        }) else {
            return false;
        };
        if matched.contains(&index) {
            return false;
        }
        matched.push(index);
        if let Some(old_name) = entry
            .get("name")
            .and_then(toml::Value::as_str)
            .or_else(|| Path::new(path).file_name().and_then(|name| name.to_str()))
        {
            aliases.insert(
                old_name,
                expected
                    .get("name")
                    .and_then(toml::Value::as_str)
                    .unwrap_or_default(),
            );
        }
    }
    for (entry, index) in old.iter().zip(matched) {
        if entry.as_table().is_some_and(|table| {
            table.keys().any(|key| {
                !matches!(
                    key.as_str(),
                    "name" | "path" | "scope" | "dependsOn" | "depends_on"
                )
            })
        }) {
            return false;
        }
        if entry
            .get("scope")
            .is_some_and(|scope| Some(scope) != expected[index].get("scope"))
        {
            return false;
        }
        let old_depends = entry.get("dependsOn").or_else(|| entry.get("depends_on"));
        let expected_depends = expected[index]
            .get("dependsOn")
            .and_then(toml::Value::as_array);
        let mut actual = Vec::new();
        if let Some(dependencies) = old_depends {
            let Some(dependencies) = dependencies.as_array() else {
                return false;
            };
            for dependency in dependencies {
                let Some(name) = dependency.as_str().and_then(|name| aliases.get(name)) else {
                    return false;
                };
                actual.push(*name);
            }
        }
        let required: Vec<_> = expected_depends
            .into_iter()
            .flatten()
            .filter_map(toml::Value::as_str)
            .collect();
        if actual != required {
            return false;
        }
    }
    true
}

/// Resolve paths from the original config's parent, which is also the generated config's parent.
fn same_path(root: &Path, left: &str, right: &str) -> bool {
    std::fs::canonicalize(root.join(left))
        .ok()
        .zip(std::fs::canonicalize(root.join(right)).ok())
        .is_some_and(|(left, right)| left == right)
}

/// Separate generated coordinates from policy while retaining every mismatch in the preview.
fn extract_managed(
    root: &Path,
    foreign: &mut toml::Table,
    expected: &toml::Table,
) -> Result<(Vec<String>, toml::Table), Error> {
    let mut conflicts = Vec::new();
    let mut managed = toml::Table::new();
    for field in ["target_platform_version", "compatibility_mode"] {
        if let Some(value) = foreign.remove(field) {
            if expected.get(field) != Some(&value) {
                conflicts.push(field.into());
            }
            managed.insert(field.into(), value);
        }
    }
    if let Some(source) = foreign.remove("source") {
        let mut source = source
            .as_table()
            .cloned()
            .ok_or_else(|| Error::InvalidPolicy {
                field: "source".into(),
            })?;
        let expected_source = expected
            .get("source")
            .and_then(toml::Value::as_table)
            .ok_or_else(|| Error::InvalidPolicy {
                field: "source".into(),
            })?;
        let mut previous_source = toml::Table::new();
        for field in ["root", "extensions", "externals", "configuration"] {
            if let Some(value) = source.remove(field) {
                if !source_matches(root, field, &value, expected_source.get(field)) {
                    conflicts.push(format!("source.{field}"));
                }
                previous_source.insert(field.into(), value);
            }
        }
        managed.insert("source".into(), previous_source.into());
        if !source.is_empty() {
            foreign.insert("source".into(), source.into());
        }
    }
    Ok((conflicts, managed))
}

/// Consumers honor the writer lock; ordinary second-file failure restores the exact first file.
fn publish_pair(preview: &Preview, candidate: Temporary) -> Result<(), Error> {
    let root = preview.base.context.root();
    let original = preview
        .base
        .previous
        .as_deref()
        .ok_or(Error::StalePreview)?;
    let manifest = root.join("eska.toml");
    let manifest_candidate = Temporary::new(root, preview.proposed_manifest.as_bytes())?;
    configuration::ensure_snapshot(&preview.base)?;
    manifest_candidate.publish(&manifest)?;
    let result = storage::unchanged(&preview.base.path(), Some(original))
        .and_then(|()| candidate.publish(&preview.base.path()));
    if let Err(original) = result {
        if storage::unchanged(&manifest, Some(preview.proposed_manifest.as_bytes()))
            .and_then(|()| Temporary::new(root, &preview.base.manifest)?.publish(&manifest))
            .is_err()
        {
            return Err(Error::Rollback {
                paths: vec![manifest],
                original: Box::new(original),
            });
        }
        return Err(original);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A real failed second rename restores the first file; a concurrent config edit also survives.
    #[test]
    fn pair_publication_rolls_back_after_second_file_failure() {
        for manual_change in [false, true] {
            let fixture = crate::test_support::TestDir::new();
            let root = &fixture.0;
            fs::create_dir(root.join("src")).unwrap();
            let manifest = b"# preserved\r\n[project]\r\ntype='configuration'\r\n";
            fs::write(root.join("eska.toml"), manifest).unwrap();
            fs::write(root.join("src/Configuration.xml"),"<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\"><Configuration uuid=\"12345678-1234-1234-1234-123456789012\"><Properties><Name>Example</Name></Properties><ChildObjects/></Configuration></MetaDataObject>").unwrap();
            let original = b"[formatting]\nindent_size=2\n";
            fs::write(root.join("bsl-analyzer.toml"), original).unwrap();
            let preview = preview(root, None).unwrap();
            let candidate = Temporary::new(root, &preview.proposed.contents).unwrap();
            if manual_change {
                fs::write(preview.base.path(), b"# manual edit\n").unwrap();
            } else {
                fs::remove_file(candidate.path()).unwrap();
            }
            assert!(publish_pair(&preview, candidate).is_err());
            assert_eq!(fs::read(root.join("eska.toml")).unwrap(), manifest);
            assert_eq!(
                fs::read(root.join("bsl-analyzer.toml")).unwrap(),
                if manual_change {
                    b"# manual edit\n".as_slice()
                } else {
                    original.as_slice()
                }
            );
        }
    }
}
