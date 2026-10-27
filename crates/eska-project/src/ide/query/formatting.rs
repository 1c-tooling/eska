//! Document and private-draft formatting share pure component calls while retaining their distinct source guards.

use super::{params::Key, session_error};
use crate::ide::{
    envelope::{domain, error},
    errors, formatting, params,
    server::Server,
};
use eska_project_core::formatting::{
    Error, Language, Patch,
    context::Context,
    protocol::{Range, Request, Response, source_sha256},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{fs, io::Read, path::Path, sync::atomic::AtomicBool};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    version: String,
    text: Option<String>,
    buffer_id: Option<String>,
    range: Option<Range>,
    user_read_only: bool,
    formatter: Option<params::Path>,
}

#[derive(Deserialize)]
struct Document {
    source: params::Path,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Draft {
    query_session_id: String,
    revision: String,
}

struct Formatted {
    request: Request,
    response: Response,
    effective: eska_project_core::formatting::Effective,
    version: Option<String>,
}

impl Server {
    /// No retained formatting plan mutates text or checkpoints; the IDE applies version-checked native edits.
    pub(in crate::ide) fn formatting(
        &mut self,
        method: &str,
        args: &Value,
        cancelled: &AtomicBool,
        closed: &AtomicBool,
    ) -> Result<Value, Value> {
        let key = buffer_key();
        match method {
            "format/bufferBegin" => return self.queries.begin_buffer(&key, args),
            "format/bufferAppend" => return self.queries.append_buffer(&key, args),
            "format/bufferClose" => return self.queries.close("query/bufferClose", &key, args),
            _ => {}
        }
        params::optional_fields(args, &["text", "bufferId", "range", "formatter"])?;
        let input: Input = params::decode(args)?;
        if params::counter(&input.version)? == 0 {
            return Err(error(-32602));
        }
        if input.user_read_only {
            return Err(formatting::failure(Error::new(
                "source_read_only",
                "document",
            )));
        }
        let text = self
            .queries
            .input_text(&key, input.text.as_deref(), input.buffer_id.as_deref())?
            .ok_or_else(|| error(-32602))?
            .to_owned();
        if method == "format/query" {
            return self.format_query(args, &input, text, cancelled, closed);
        }
        let document: Document = params::decode(args)?;
        let source = document.source.native()?;
        if !source.is_absolute() {
            return Err(error(-32602));
        }
        let path = fs::canonicalize(&source)
            .map_err(|_| formatting::failure(Error::new("source_io", "source")))?;
        let known = self.session.as_ref().is_some_and(|session| {
            session
                .workspace
                .projects()
                .iter()
                .any(|project| path.starts_with(project.project().root()))
        });
        if !known
            && !self
                .source_root
                .as_ref()
                .is_some_and(|root| path.starts_with(root))
        {
            return Err(formatting::failure(Error::new(
                "unsupported_source",
                "source",
            )));
        }
        format_document(&path, &input, text, cancelled, closed)
    }

    /// A query owns its source lease and must acknowledge the native draft before formatting can begin.
    fn format_query(
        &mut self,
        args: &Value,
        input: &Input,
        text: String,
        cancelled: &AtomicBool,
        closed: &AtomicBool,
    ) -> Result<Value, Value> {
        self.check_session(args)?;
        let key = Key::decode(args)?;
        let draft: Draft = params::decode(args)?;
        let session = self.session.as_mut().ok_or_else(|| error(-32603))?;
        let state = session
            .projects
            .iter()
            .find(|state| state.id == key.project)
            .ok_or_else(|| domain("unknown_project", json!({"projectId":key.project})))?;
        if state.reopen {
            return Err(domain(
                "reopen_required",
                json!({"reason":"manifest_changed"}),
            ));
        }
        if state.refresh {
            return Err(domain("resync_required", json!({"reason":"file_events"})));
        }
        let project = session
            .workspace
            .project_mut(&state.scope)
            .map_err(|error| errors::workspace(&error))?;
        project
            .check_generation(params::counter(&key.generation)?)
            .map_err(|error| errors::workspace(&error))?;
        let entry = self
            .queries
            .sessions
            .get(&draft.query_session_id)
            .filter(|entry| entry.key.same_owner(&key))
            .ok_or_else(|| super::failure("unknown_query_session"))?;
        let revision = params::counter(&draft.revision)?;
        entry
            .core
            .check_draft_formatting(project, revision, &text, input.user_read_only)
            .map_err(session_error)?;
        let path = project
            .project()
            .source()
            .join(entry.core.document().path());
        let context =
            Context::open(project.project().root(), &[], false).map_err(formatting::failure)?;
        let request = request(&context, &path, Language::Sdbl, text, input.range)?;
        let result = execute(&context, request, input, cancelled, closed)?;
        entry
            .core
            .check_draft_formatting(
                project,
                revision,
                &result.request.text,
                input.user_read_only,
            )
            .map_err(session_error)?;
        context.check_current().map_err(formatting::failure)?;
        Ok(reply(
            input,
            &result.response,
            &result.effective,
            result.version.as_deref(),
        ))
    }
}

/// Native buffers may be dirty; the disk fingerprint detects independent external source replacement.
fn format_document(
    path: &Path,
    input: &Input,
    text: String,
    cancelled: &AtomicBool,
    closed: &AtomicBool,
) -> Result<Value, Value> {
    let language = match path.extension().and_then(|extension| extension.to_str()) {
        Some("bsl") => Language::Bsl,
        Some("sdbl") => Language::Sdbl,
        _ => {
            return Err(formatting::failure(Error::new(
                "unsupported_language",
                "source",
            )));
        }
    };
    let mut context = Context::open(path, &[], false).map_err(formatting::failure)?;
    let request = request(&context, path, language, text, input.range)?;
    if request.1.excluded {
        let result = execute(&context, request, input, cancelled, closed)?;
        context.check_current().map_err(formatting::failure)?;
        return Ok(reply(
            input,
            &result.response,
            &result.effective,
            result.version.as_deref(),
        ));
    }
    context.check_file(path).map_err(formatting::failure)?;
    let disk = fingerprint(path)?;
    let result = execute(&context, request, input, cancelled, closed)?;
    context.check_current().map_err(formatting::failure)?;
    if !result.effective.excluded {
        context.check_file(path).map_err(formatting::failure)?;
    }
    if fingerprint(path)? != disk {
        return Err(formatting::failure(Error::new("source_conflict", "source")));
    }
    Ok(reply(
        input,
        &result.response,
        &result.effective,
        result.version.as_deref(),
    ))
}

/// Operation styles always come from core; native tab settings are presentation preferences only.
fn request(
    context: &Context,
    path: &Path,
    language: Language,
    text: String,
    range: Option<Range>,
) -> Result<(Request, eska_project_core::formatting::Effective), Value> {
    let effective = context
        .policy(path, language, &Patch::default())
        .map_err(formatting::failure)?;
    let request = Request {
        schema_version: 1,
        language,
        text,
        options: effective.options.clone(),
        range,
    };
    request.validate().map_err(formatting::failure)?;
    Ok((request, effective))
}

/// Excluded sources issue a deterministic no-op without starting a missing optional component.
fn execute(
    context: &Context,
    pair: (Request, eska_project_core::formatting::Effective),
    input: &Input,
    cancelled: &AtomicBool,
    closed: &AtomicBool,
) -> Result<Formatted, Value> {
    let (request, effective) = pair;
    let (response, version) = if effective.excluded {
        (
            Response {
                schema_version: 1,
                source_sha256: source_sha256(&request.text),
                edits: Vec::new(),
                formatted_range: request.range.unwrap_or(Range {
                    start: 0,
                    end: request.text.len(),
                }),
            },
            None,
        )
    } else {
        let (response, version) = formatting::invoke(
            &request,
            input.formatter.as_ref(),
            context.root(),
            cancelled,
            closed,
        )?;
        (response, Some(version))
    };
    Ok(Formatted {
        request,
        response,
        effective,
        version,
    })
}

/// Native adapters receive one policy and byte plan, bound to their exact document version.
fn reply(
    input: &Input,
    response: &Response,
    effective: &eska_project_core::formatting::Effective,
    version: Option<&str>,
) -> Value {
    json!({"schemaVersion":1,"version":input.version,"plan":response,"effective":effective,"formatterVersion":version})
}

/// Uploaded formatting text has a connection-local namespace independent of metadata generations.
fn buffer_key() -> Key {
    Key {
        session: "@format".into(),
        project: "@format".into(),
        generation: "1".into(),
    }
}

/// The format API never reads an unbounded external file solely to check for an intervening disk change.
fn fingerprint(path: &Path) -> Result<String, Value> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes))
        .map_err(|_| formatting::failure(Error::new("source_io", "source")))?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(formatting::failure(Error::new("text_too_large", "disk")));
    }
    // A BSL/SDBL owner must be UTF-8; the exact BOM and line endings participate in its fingerprint.
    let text = String::from_utf8(bytes)
        .map_err(|_| formatting::failure(Error::new("invalid_utf8", "source")))?;
    Ok(source_sha256(&text))
}
