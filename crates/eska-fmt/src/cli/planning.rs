//! Scope planning finishes for every file before core publication can replace the first one.

use super::{
    Cli,
    report::{self, File, Report},
};
use eska_project_core::{
    formatting::{
        Error, Language, Patch,
        context::Context,
        files::{self, WritePlan},
        migration,
        protection::Access,
        protocol::{Edit, Range, Request, source_sha256},
    },
    project::{metadata_edit, query},
};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

/// Native source protection remains in core; this layer only coordinates the pure engine and report.
pub fn run(cli: &Cli, cancelled: &AtomicBool, report: &mut Report) -> Result<(), Error> {
    let start =
        fs::canonicalize(&cli.project_dir).map_err(|_| Error::new("source_io", "project_dir"))?;
    let discovery = cli
        .arguments
        .scope
        .as_ref()
        .map_or_else(|| start.clone(), |scope| start.join(scope));
    let mut context = Context::open(&discovery, &cli.arguments.member, cli.arguments.workspace)?;
    if cli.arguments.migrate_config() || cli.arguments.apply_migration() {
        return migrate(cli, &context, report);
    }
    let operation: Patch = cli
        .arguments
        .style
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|_| Error::new("invalid_options", "style"))?
        .unwrap_or_default();
    let paths = context.files(
        &start,
        cli.arguments.scope.as_deref(),
        cli.arguments.query_index.is_some(),
    )?;
    if (cli.arguments.query_at.is_some() || cli.arguments.query_index.is_some()) && paths.len() != 1
    {
        return Err(Error::new("invalid_scope", "query"));
    }
    if paths.is_empty() && discovery.is_file() {
        return Err(Error::new(
            "unsupported_language",
            discovery.to_string_lossy(),
        ));
    }
    let (plans, indices) =
        prepare_sources(cli, cancelled, report, &mut context, paths, &operation)?;
    if cli.arguments.check() || cli.arguments.diff() || cli.arguments.dump_config() {
        return Ok(());
    }
    let publication = files::publish(&mut context, &plans, cancelled);
    for (index, state) in indices.into_iter().zip(publication.states) {
        report.files[index].status = match state {
            files::State::Applied => "applied",
            files::State::Failed => "failed",
            files::State::NotApplied => "not_applied",
        };
    }
    publication.error.map_or(Ok(()), Err)
}

/// Retain only a bounded complete plan; an error here cannot have published any source file.
fn prepare_sources(
    cli: &Cli,
    cancelled: &AtomicBool,
    report: &mut Report,
    context: &mut Context,
    paths: Vec<PathBuf>,
    operation: &Patch,
) -> Result<(Vec<WritePlan>, Vec<usize>), Error> {
    let mut plans = Vec::new();
    let mut indices = Vec::new();
    let mut bytes = 0_usize;
    for path in paths {
        if cancelled.load(Ordering::Acquire) {
            return Err(Error::new("cancelled", "operation"));
        }
        let language = if path
            .extension()
            .is_some_and(|extension| extension == "sdbl")
            || cli.arguments.query_at.is_some()
            || cli.arguments.query_index.is_some()
        {
            Language::Sdbl
        } else {
            Language::Bsl
        };
        let effective = context.policy(&path, language, operation)?;
        let mut file = File {
            path: path.to_string_lossy().into(),
            language,
            source_sha256: None,
            changed: false,
            status: if effective.excluded {
                "excluded"
            } else {
                "unchanged"
            },
            edits: Vec::new(),
            effective,
            diff: None,
            skip_reason: None,
        };
        if !file.effective.excluded && !cli.arguments.dump_config() {
            if !eligible(cli, context, &path, &mut file)? {
                report.files.push(file);
                continue;
            }
            let before = read(
                &path,
                cli.arguments.query_at.is_some() || cli.arguments.query_index.is_some(),
            )?;
            file.source_sha256 = Some(source_sha256(&before));
            let range = query_range(cli, &before)?;
            let result = files::preflight(context, &path, &before, range)
                .and_then(|()| format(cli, &path, &before, &file));
            let (after, edits) = match result {
                Ok(value) => value,
                Err(error) => {
                    file.status = "failed";
                    report.files.push(file);
                    return Err(error);
                }
            };
            file.changed = before != after;
            file.edits = edits;
            if file.changed {
                bytes = bytes
                    .saturating_add(before.len())
                    .saturating_add(after.len());
                if bytes > 128 * 1024 * 1024 {
                    return Err(Error::new("plan_too_large", "scope"));
                }
                file.status = "planned";
                if cli.arguments.diff() {
                    file.diff = Some(report::diff(&file.path, &before, &after));
                }
                indices.push(report.files.len());
                plans.push(WritePlan {
                    path,
                    before,
                    after,
                    range,
                });
            }
        }
        report.files.push(file);
    }
    Ok((plans, indices))
}

/// Bulk CI formatting skips supplier restrictions before reading or parsing their module text.
fn eligible(cli: &Cli, context: &mut Context, path: &Path, file: &mut File) -> Result<bool, Error> {
    let query = cli.arguments.query_at.is_some() || cli.arguments.query_index.is_some();
    let access = context.check_file(path).and_then(|access| {
        if access == Access::SelectionOnly && !query {
            Err(Error::new("selection_required", "range"))
        } else {
            Ok(())
        }
    });
    let bulk = !query
        && cli
            .arguments
            .scope
            .as_ref()
            .is_none_or(|scope| !cli.project_dir.join(scope).is_file());
    match access {
        Ok(()) => Ok(true),
        Err(error)
            if bulk
                && matches!(
                    error.code.as_str(),
                    "source_read_only" | "support_unknown" | "selection_required"
                ) =>
        {
            file.status = "skipped";
            file.skip_reason = Some(error.code);
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

/// An explicit query selector authorizes its scalar source span, never the whole supplier module.
fn query_range(cli: &Cli, input: &str) -> Result<Option<Range>, Error> {
    let selected = if let Some(offset) = cli.arguments.query_at {
        Some(
            query::read_bsl(input, offset)
                .map_err(|_| Error::new("unsupported_source", "query_at"))?,
        )
    } else if let Some(index) = cli.arguments.query_index {
        Some(
            query::read_xml(input)
                .map_err(|_| Error::new("unsupported_source", "query_index"))?
                .into_iter()
                .nth(index)
                .ok_or_else(|| Error::new("invalid_scope", "query_index"))?,
        )
    } else {
        None
    };
    Ok(selected.map(|query| Range {
        start: query.source.start,
        end: query.source.end,
    }))
}

/// Query encoding reuses source maps; ordinary BSL never traverses or regenerates its string values.
fn format(cli: &Cli, path: &Path, before: &str, file: &File) -> Result<(String, Vec<Edit>), Error> {
    let identity = if let Some(offset) = cli.arguments.query_at {
        Some(
            query::read_bsl(before, offset)
                .map_err(|_| Error::new("unsupported_source", "query_at"))?
                .identity,
        )
    } else if let Some(index) = cli.arguments.query_index {
        Some(
            query::read_xml(before)
                .map_err(|_| Error::new("unsupported_source", "query_index"))?
                .get(index)
                .ok_or_else(|| Error::new("invalid_scope", "query_index"))?
                .identity
                .clone(),
        )
    } else {
        None
    };
    if let Some(identity) = identity {
        let plan = eska_fmt::format_query(
            before,
            &metadata_edit::snapshot(before),
            &identity,
            file.effective.options.clone(),
            None,
        )?;
        let edits = plan
            .replacements()
            .iter()
            .map(|replacement| Edit {
                range: Range {
                    start: replacement.range.start,
                    end: replacement.range.end,
                },
                text: replacement.text.clone(),
            })
            .collect();
        return Ok((plan.output().to_owned(), edits));
    }
    let request = Request {
        schema_version: 1,
        language: file.language,
        text: before.into(),
        options: file.effective.options.clone(),
        range: None,
    };
    let response = eska_fmt::format(&request).map_err(|mut error| {
        if error.field == "text" {
            error.field = path.to_string_lossy().into();
        }
        error
    })?;
    let mut after = before.to_owned();
    for edit in response.edits.iter().rev() {
        after.replace_range(edit.range.start..edit.range.end, &edit.text);
    }
    Ok((after, response.edits))
}

/// Existing XML/BSL owners have an eight-MiB extraction budget; standalone formatter input stays four MiB.
fn read(path: &Path, owner: bool) -> Result<String, Error> {
    let maximum = if owner {
        8 * 1024 * 1024
    } else {
        4 * 1024 * 1024
    };
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(maximum + 1).read_to_end(&mut bytes))
        .map_err(|_| Error::new("source_io", path.to_string_lossy()))?;
    if bytes.len() as u64 > maximum {
        return Err(Error::new("text_too_large", path.to_string_lossy()));
    }
    String::from_utf8(bytes).map_err(|_| Error::new("invalid_utf8", path.to_string_lossy()))
}

/// Migration is an explicit manifest-only operation with a checked preview and deterministic backup.
fn migrate(cli: &Cli, context: &Context, report: &mut Report) -> Result<(), Error> {
    let path = context.root().join("eska.toml");
    let before = fs::read_to_string(&path).map_err(|_| Error::new("source_io", "eska.toml"))?;
    let plan = migration::preview(&before)?;
    let backup: Option<PathBuf> = if cli.arguments.apply_migration() {
        plan.as_ref()
            .map(|plan| migration::apply(&path, plan))
            .transpose()?
    } else {
        None
    };
    report.migration = Some(
        serde_json::json!({"path":path,"changed":plan.is_some(),"before":before,"after":plan.as_ref().map_or(before.as_str(),|plan|plan.after.as_str()),"backup":backup}),
    );
    Ok(())
}
