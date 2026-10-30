//! Project formatting remains the authority for style parameters supported by the analyzer.

use eska_project_core::{
    application::AnalysisContext,
    config::ManifestConfig,
    formatting::{Config, IndentStyle, Language, Layer, Options, Patch, resolve},
};
use std::path::Path;

use super::{Error, read};

/// Derive only documented analyzer parameters; formatter-specific switches never leak into TOML.
pub(super) fn apply(
    context: &AnalysisContext,
    manifest: &[u8],
    table: &mut toml::Table,
) -> Result<(), Error> {
    let Some(options) = options(context, manifest)? else {
        return Ok(());
    };
    let formatting = child(table, "formatting", "formatting")?;
    formatting.insert(
        "use_tabs".into(),
        (options.indent_style == IndentStyle::Tab).into(),
    );
    formatting.insert(
        "indent_size".into(),
        i64::from(if options.indent_style == IndentStyle::Tab {
            options.indent_width / options.tab_width
        } else {
            options.indent_width
        })
        .into(),
    );
    let parameters = child(
        child(table, "diagnostics", "diagnostics")?,
        "parameters",
        "diagnostics.parameters",
    )?;
    if options.column_limit == 0 {
        parameters.insert("LineLength".into(), false.into());
    } else {
        diagnostic(
            parameters,
            "LineLength",
            "maxLineLength",
            options.column_limit,
        )?;
    }
    let blank_lines = [
        options.max_empty_lines,
        options.blank_lines_between_methods,
        options.blank_lines_inside_methods,
        options.blank_lines_around_blocks,
        options.blank_lines_inside_multistatement_blocks,
        options.blank_lines_before_return,
        options.blank_lines_around_multiline_statements,
        options.blank_lines_between_object_groups,
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    diagnostic(
        parameters,
        "ConsecutiveEmptyLines",
        "allowedEmptyLinesCount",
        blank_lines,
    )
}

/// One analyzer context uses its main writable project's style, with the same workspace inheritance.
fn options(context: &AnalysisContext, manifest: &[u8]) -> Result<Option<Options>, Error> {
    let root = context.root();
    let primary = context
        .sources()
        .iter()
        .find(|source| !source.is_reference())
        .ok_or_else(|| Error::InvalidPolicy {
            field: "format.context".into(),
        })?;
    let mut manifests = vec![(root, text(manifest, root)?)];
    if primary.project().root() != root {
        let member = primary.project().root();
        manifests.push((
            member,
            text(&read(root, &member.join("eska.toml"))?, member)?,
        ));
    }
    let mut configs = manifests
        .iter()
        .map(|(_, manifest)| config(manifest))
        .collect::<Result<Vec<_>, _>>()?;
    if configs.iter().all(Config::is_empty) {
        return Ok(None);
    }
    // Upstream has one global style; a source-path rule cannot be represented as a global limit.
    for config in &mut configs {
        config.overrides.clear();
    }
    let layers = manifests
        .iter()
        .zip(&configs)
        .map(|((root, manifest), config)| Layer::new(root, "manifest", config, manifest))
        .collect::<Result<Vec<_>, _>>()
        .map_err(format_error)?;
    resolve(
        &layers,
        primary.project().source(),
        Language::Bsl,
        &Patch::default(),
    )
    .map(|policy| Some(policy.options))
    .map_err(format_error)
}

/// Proposed migration bytes must be read from their snapshot, not from the still-unmodified file.
fn text(bytes: &[u8], root: &Path) -> Result<String, Error> {
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|error| Error::InvalidToml {
            path: root.join("eska.toml"),
            detail: error.to_string(),
        })
}

/// Strict project/workspace parsing retains the same profile and language scopes as the formatter.
fn config(manifest: &str) -> Result<Config, Error> {
    let manifest = ManifestConfig::from_toml(manifest).map_err(|_| Error::InvalidPolicy {
        field: "format".into(),
    })?;
    Ok(match manifest {
        ManifestConfig::Project(project) => project.configuration().formatting_settings().clone(),
        ManifestConfig::Workspace(workspace) => workspace.formatting_settings().clone(),
    })
}

/// Preserve explicit rule disables and unrelated parameters while deriving the shared style key.
fn diagnostic(
    parameters: &mut toml::Table,
    name: &str,
    key: &str,
    value: u16,
) -> Result<(), Error> {
    let field = format!("diagnostics.parameters.{name}");
    let diagnostic = parameters
        .entry(name)
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if diagnostic.as_bool() == Some(false) {
        return Ok(());
    }
    if diagnostic.as_bool() == Some(true) {
        *diagnostic = toml::Value::Table(toml::Table::new());
    }
    let diagnostic = diagnostic
        .as_table_mut()
        .ok_or(Error::InvalidPolicy { field })?;
    diagnostic.insert(key.into(), i64::from(value).into());
    Ok(())
}

/// Malformed policy tables remain visible errors instead of being silently discarded.
fn child<'a>(
    table: &'a mut toml::Table,
    name: &str,
    field: &str,
) -> Result<&'a mut toml::Table, Error> {
    table
        .entry(name)
        .or_insert_with(|| toml::Value::Table(toml::Table::new()))
        .as_table_mut()
        .ok_or_else(|| Error::InvalidPolicy {
            field: field.into(),
        })
}

/// Shared formatting failures keep their exact field without introducing localized backend strings.
fn format_error(error: eska_project_core::formatting::Error) -> Error {
    let mut field = error.field;
    field.insert_str(0, "format.");
    Error::InvalidPolicy { field }
}
