//! Snapshot-bound text inspection reuses constructor tokens and the existing bounded upload protocol.

use super::{
    Deserialize, Key, State, Value, error, failure, json, params, session_error, sessions,
};
use eska_project_core::project::query::constructor::{Selection, TextTarget};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum TextView {
    Diff,
    Comments {
        offset: usize,
        limit: usize,
        selection: Option<Selection>,
    },
}

impl State {
    /// Hydrate only an explicitly submitted text action; a foreign or incomplete upload grants no authority.
    pub(super) fn constructor_text_args(
        &self,
        key: &Key,
        method: &str,
        args: &Value,
    ) -> Result<Option<Value>, Value> {
        if !matches!(
            method,
            "query/constructorAction" | "query/constructorPreview"
        ) || args["action"]["kind"] != "setText"
        {
            return Ok(None);
        }
        let action = &args["action"];
        if !action["text"].is_null() && !action["text"].is_string()
            || !action["bufferId"].is_null() && !action["bufferId"].is_string()
        {
            return Err(error(-32602));
        }
        let text = self
            .input_text(key, action["text"].as_str(), action["bufferId"].as_str())?
            .ok_or_else(|| error(-32602))?;
        let mut hydrated = args.clone();
        let object = hydrated["action"]
            .as_object_mut()
            .ok_or_else(|| error(-32602))?;
        object.remove("bufferId");
        object.insert("text".into(), json!(text));
        Ok(Some(hydrated))
    }

    /// Read-only inspection remains usable after a metadata generation change, on the exact retained revision.
    pub(super) fn constructor_text_read(
        entry: &sessions::Entry,
        method: &str,
        args: &Value,
    ) -> Result<Option<Value>, Value> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            query_session_id: String,
            constructor_id: String,
            constructor_revision: String,
            target: Option<TextTarget>,
            view: Option<TextView>,
        }
        if !matches!(method, "query/constructorText" | "query/constructorLocate") {
            return Ok(None);
        }
        let input: Input = params::decode(args)?;
        let value = entry
            .core
            .constructor(params::counter(&input.constructor_id)?)
            .map_err(session_error)?;
        if value.revision() != params::counter(&input.constructor_revision)? {
            return Err(failure("stale_constructor"));
        }
        let mut output = sessions::header(&input.query_session_id, entry)?;
        output["constructorRevision"] = json!(input.constructor_revision);
        match method {
            "query/constructorLocate" => {
                output["location"] = json!(
                    value
                        .locate(&input.target.ok_or_else(|| error(-32602))?)
                        .map_err(text_error)?
                );
            }
            _ => match input.view.ok_or_else(|| error(-32602))? {
                TextView::Diff => {
                    output["diff"] = json!(value.text_diff().map_err(text_error)?);
                }
                TextView::Comments {
                    offset,
                    limit,
                    selection,
                } => {
                    output["comments"] = json!(
                        value
                            .comment_page(offset, limit, selection)
                            .map_err(text_error)?
                    );
                }
            },
        }
        Ok(Some(output))
    }
}

/// Reuse stable structured session error codes instead of formatting a Rust debug name.
fn text_error(error: eska_project_core::project::query::constructor::Error) -> Value {
    session_error(
        eska_project_core::project::metadata_workspace::QuerySessionError::Constructor(error),
    )
}
