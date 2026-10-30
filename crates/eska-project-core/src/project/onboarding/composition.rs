//! Append a newly created or initialized project to exactly one explicit application role.

use std::path::Path;

use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, Value};

use super::{ProjectName, ProjectType, WorkspaceEnrollmentError};

/// Preserve user policy, comments and dependencies while changing only the selected role.
pub(super) fn append(
    document: &mut DocumentMut,
    path: &Path,
    name: &ProjectName,
    kind: ProjectType,
    requested: Option<&str>,
) -> Result<(), WorkspaceEnrollmentError> {
    let member = path
        .to_str()
        .ok_or_else(|| WorkspaceEnrollmentError::NonUtf8MemberPath { path: path.into() })?;
    #[cfg(windows)]
    let portable = member.replace('\\', "/");
    #[cfg(windows)]
    let member = portable.as_str();
    let workspace = document
        .get_mut("workspace")
        .and_then(Item::as_table_mut)
        .ok_or(WorkspaceEnrollmentError::InvalidComposition)?;
    let members = table(
        workspace
            .entry("members")
            .or_insert_with(|| Item::Table(Table::new())),
    )?;
    let selected = select(members, name, kind, requested)?;
    let application = table(
        members
            .entry(&selected)
            .or_insert_with(|| Item::Table(Table::new())),
    )?;
    match kind {
        ProjectType::Configuration => {
            if application.contains_key("configuration") {
                return Err(WorkspaceEnrollmentError::ConfigurationAlreadySet {
                    application: selected,
                });
            }
            application.insert("configuration", toml_edit::value(member));
        }
        ProjectType::Extension | ProjectType::Processing | ProjectType::Report => {
            let key = if kind == ProjectType::Extension {
                "extensions"
            } else {
                "external_objects"
            };
            let item = application
                .entry(key)
                .or_insert_with(|| Item::Value(Value::Array(Array::new())));
            if kind == ProjectType::Extension {
                append_extension(item, member)?;
            } else {
                item.as_array_mut()
                    .ok_or(WorkspaceEnrollmentError::InvalidComposition)?
                    .push(member);
            }
        }
    }
    Ok(())
}

/// Both TOML spellings retain the same structured member and preserve existing settings.
fn append_extension(item: &mut Item, path: &str) -> Result<(), WorkspaceEnrollmentError> {
    if let Some(tables) = item.as_array_of_tables_mut() {
        let mut extension = Table::new();
        extension.insert("path", toml_edit::value(path));
        tables.push(extension);
    } else {
        let values = item
            .as_array_mut()
            .ok_or(WorkspaceEnrollmentError::InvalidComposition)?;
        let mut extension = InlineTable::new();
        extension.insert("path", path.into());
        values.push(extension);
    }
    Ok(())
}

/// Inline and expanded TOML tables express the same schema; preserve either accepted input.
fn table(item: &mut Item) -> Result<&mut Table, WorkspaceEnrollmentError> {
    if let Some(inline) = item.as_inline_table() {
        let comment = inline
            .decor()
            .suffix()
            .and_then(toml_edit::RawString::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned();
        let mut expanded = inline.clone().into_table();
        if !comment.is_empty() {
            expanded.decor_mut().set_prefix(format!("\n{comment}\n"));
        }
        *item = Item::Table(expanded);
    }
    item.as_table_mut()
        .ok_or(WorkspaceEnrollmentError::InvalidComposition)
}

/// An explicit name may create a new application; implicit selection never picks among several.
fn select(
    members: &Table,
    name: &ProjectName,
    kind: ProjectType,
    requested: Option<&str>,
) -> Result<String, WorkspaceEnrollmentError> {
    if let Some(requested) = requested {
        ProjectName::parse(requested.into()).map_err(|_| {
            WorkspaceEnrollmentError::InvalidApplication {
                name: requested.into(),
            }
        })?;
        if !members.is_empty()
            && !members.contains_key(requested)
            && kind != ProjectType::Configuration
        {
            return Err(WorkspaceEnrollmentError::InvalidApplication {
                name: requested.into(),
            });
        }
        return Ok(requested.into());
    }
    if members.is_empty() {
        return Ok(name.as_str().into());
    }
    if members.len() == 1
        && let Some((name, application)) = members.iter().next()
        && (kind != ProjectType::Configuration || application.get("configuration").is_none())
    {
        return Ok(name.into());
    }
    if kind == ProjectType::Configuration && !members.contains_key(name.as_str()) {
        return Ok(name.as_str().into());
    }
    Err(WorkspaceEnrollmentError::ApplicationRequired {
        available: members.iter().map(|(name, _)| name.into()).collect(),
    })
}
