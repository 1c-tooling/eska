//! Generation-checked metadata queries and explicit refresh commit boundaries.

use super::{
    dto,
    envelope::{domain, error},
    errors, params,
    server::{ProjectState, Server},
};
use crate::project::{
    configurator::TreeOptions,
    metadata_model::ObjectId,
    metadata_workspace::{
        ProjectSession,
        search::{IndexFailure, SearchOptions},
    },
};
use serde::Deserialize;
use serde_json::{Value, json};

/// Recognize request-only methods before validating metadata parameters.
pub(super) fn is_method(method: &str) -> bool {
    matches!(
        method,
        "metadata/root"
            | "metadata/children"
            | "metadata/get"
            | "metadata/properties"
            | "metadata/source"
            | "metadata/search"
            | "metadata/reveal"
            | "metadata/refresh"
            | "metadata/index"
            | "metadata/indexErrors"
    )
}

impl Server {
    /// Resolve context, enforce recovery state, then expose only current-generation data.
    pub(super) fn metadata(
        &mut self,
        method: &str,
        args: &Value,
        events: &mut Vec<Value>,
    ) -> Result<Value, Value> {
        let input: params::Context = params::decode(args)?;

        if self
            .session
            .as_ref()
            .is_none_or(|session| session.id != input.session_id)
        {
            return Err(domain(
                "unknown_session",
                json!({"sessionId": input.session_id}),
            ));
        }
        params::counter(&input.generation)?;
        let session = self.session.as_mut().ok_or_else(|| error(-32603))?;
        let state = session
            .projects
            .iter_mut()
            .find(|state| state.id == input.project_id)
            .ok_or_else(|| domain("unknown_project", json!({"projectId":input.project_id})))?;
        let project = session
            .workspace
            .project_mut(&state.scope)
            .map_err(|failure| errors::workspace(&failure))?;
        let result = metadata_request(
            &self.labels,
            method,
            args,
            &session.id,
            state,
            project,
            events,
        );
        result
            .map(|mut result| {
                result["sessionId"] = json!(session.id);
                result["projectId"] = json!(state.id);
                result["generation"] = json!(project.generation().to_string());
                result["eventSequence"] = json!(state.event.to_string());
                result
            })
            .map_err(|mut failure| {
                if let Some(data) = failure.get_mut("data") {
                    data["sessionId"] = json!(session.id);
                    data["projectId"] = json!(state.id);
                    data["generation"] = json!(project.generation().to_string());
                }
                failure
            })
    }
}

/// Recovery refresh skips stale-generation checks only for the exact root node.
fn metadata_request(
    labels: &dto::Labels,
    method: &str,
    args: &Value,
    session: &str,
    state: &mut ProjectState,
    project: &mut ProjectSession,
    events: &mut Vec<Value>,
) -> Result<Value, Value> {
    let generation = params::counter(args["generation"].as_str().ok_or_else(|| error(-32602))?)?;
    if state.reopen {
        return Err(domain(
            "reopen_required",
            json!({"reason":"manifest_changed"}),
        ));
    }
    let root_refresh =
        method == "metadata/refresh" && params::node(&args["node"])? == *project.root();
    if state.refresh && !root_refresh && !(method == "metadata/index" && args["action"] == "status")
    {
        return Err(domain("resync_required", json!({"reason":"file_events"})));
    }
    if !(state.refresh && root_refresh) {
        project
            .check_generation(generation)
            .map_err(|failure| errors::workspace(&failure))?;
    }
    match method {
        "metadata/root" => Ok(
            json!({"node":labels.node(project.node(project.root()).map_err(|failure| errors::workspace(&failure))?)}),
        ),
        "metadata/children" => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Options {
                hide_empty_root_sections: Option<bool>,
            }
            let options: Options = params::decode(args)?;
            let id = params::node(&args["node"])?;
            let nodes = project
                .children(
                    &id,
                    TreeOptions {
                        hide_empty_root_sections: options.hide_empty_root_sections.unwrap_or(true),
                    },
                )
                .map_err(|failure| errors::workspace(&failure))?;
            Ok(json!({"nodes":nodes.into_iter().map(|node|labels.node(node)).collect::<Vec<_>>()}))
        }
        "metadata/get" => {
            let id: ObjectId = params::decode(&args["objectId"])?;
            Ok(
                json!({"object":dto::object(project.object(&id).map_err(|failure|errors::workspace(&failure))?)}),
            )
        }
        "metadata/properties" => {
            let id: ObjectId = params::decode(&args["objectId"])?;
            let properties = project
                .properties(&id)
                .map_err(|failure| errors::workspace(&failure))?;
            Ok(json!({"properties":properties.iter().map(dto::property).collect::<Vec<_>>()}))
        }
        "metadata/source" => {
            let id = params::node(&args["node"])?;
            let sources = project
                .source(&id)
                .map_err(|failure| errors::workspace(&failure))?;
            if sources.is_empty() {
                return Err(domain(
                    "source_missing",
                    json!({"node":dto::node_id(&id),"reason":"not_file"}),
                ));
            }
            Ok(json!({"sources":sources.iter().map(dto::source).collect::<Vec<_>>()}))
        }
        "metadata/search" => search(project, args),
        "metadata/reveal" => {
            let id: ObjectId = params::decode(&args["objectId"])?;
            let ancestry = project
                .reveal_indexed_object(&id)
                .map_err(|failure| errors::workspace(&failure))?;
            Ok(json!({"ancestry":ancestry.iter().map(dto::node_id).collect::<Vec<_>>()}))
        }
        "metadata/refresh" => refresh(session, state, project, args, events),
        "metadata/index" => index(session, state, project, args, events),
        "metadata/indexErrors" => index_errors(project, args),
        _ => Err(error(-32601)),
    }
}

/// Literal search never schedules indexing implicitly.
fn search(project: &ProjectSession, args: &Value) -> Result<Value, Value> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Search {
        text: String,
        limit: Option<usize>,
        synonym_language: Option<String>,
    }
    let input: Search = params::decode(args)?;
    let limit = limit(input.limit)?;
    let result = project.search(
        &input.text,
        &SearchOptions {
            limit,
            synonym_language: input.synonym_language,
        },
    );
    Ok(
        json!({"progress":dto::progress(&result.progress),"hits":result.hits.iter().map(dto::hit).collect::<Vec<_>>(),"truncated":result.truncated}),
    )
}

/// Refresh publishes changed state even when rereading the root fails.
fn refresh(
    session: &str,
    state: &mut ProjectState,
    project: &mut ProjectSession,
    args: &Value,
    events: &mut Vec<Value>,
) -> Result<Value, Value> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Options {
        reset_file_sequence: Option<bool>,
    }
    let options: Options = params::decode(args)?;
    let node = params::node(&args["node"])?;
    let root = &node == project.root();
    if options.reset_file_sequence.unwrap_or(false) && !root {
        return Err(error(-32602));
    }
    // Validate identity before claiming that a source mutation has occurred.
    project
        .node(&node)
        .map_err(|failure| errors::workspace(&failure))?;
    let result = project
        .refresh(&node)
        .map_err(|failure| errors::workspace(&failure));
    if root {
        state.refresh = result.is_err();
    }
    if result.is_ok() && options.reset_file_sequence.unwrap_or(false) {
        state.file = 0;
    }
    let affected = result
        .as_ref()
        .map_or(Value::Null, |report| json!(report.affected));
    changed(session, state, project, affected, events)?;
    Server::progress_event(session, state, project, events);
    result.map(|report| json!({"affected":report.affected}))
}

/// Start and resume return immediately; the worker schedules bounded descriptor steps later.
fn index(
    session: &str,
    state: &mut ProjectState,
    project: &mut ProjectSession,
    args: &Value,
    events: &mut Vec<Value>,
) -> Result<Value, Value> {
    match args["action"].as_str() {
        Some("start") => project.start_search_index(),
        Some("cancel") => project.cancel_search_index(),
        Some("resume") => project.resume_search_index(),
        Some("status") => (),
        _ => return Err(error(-32602)),
    }
    Server::progress_event(session, state, project, events);
    Ok(json!({"progress":dto::progress(&project.search_progress())}))
}

/// Paginate the current stable `ObjectId` ordering without retaining stale page snapshots.
fn index_errors(project: &ProjectSession, args: &Value) -> Result<Value, Value> {
    #[derive(Deserialize)]
    struct Options {
        offset: Option<usize>,
        limit: Option<usize>,
    }
    let input: Options = params::decode(args)?;
    let count = limit(input.limit)?;
    let offset = input.offset.unwrap_or(0);
    let mut entries = project.search_failures().skip(offset);
    let errors: Vec<_> = entries.by_ref().take(count).map(|(id, failure)| {
        let (code, details) = match failure {
            IndexFailure::Read(value) => { let failure = errors::workspace(value); (failure["data"]["kind"].clone(), failure["data"]["details"].clone()) }
            IndexFailure::Unsupported(diagnostics) => (json!("unsupported_metadata"), json!({"diagnostics":diagnostics.iter().map(dto::diagnostic).collect::<Vec<_>>()})),
        };
        json!({"objectId":id.as_str(),"code":code,"details":details})
    }).collect();
    let next = entries
        .next()
        .and_then(|_| offset.checked_add(errors.len()));
    Ok(json!({"errors":errors,"nextOffset":next}))
}

/// Reject out-of-contract limits rather than silently clamping client input.
fn limit(value: Option<usize>) -> Result<usize, Value> {
    let value = value.unwrap_or(50);
    if (1..=500).contains(&value) {
        Ok(value)
    } else {
        Err(error(-32602))
    }
}

/// Each invalidation has its own counter, including failed changes with unchanged generation.
pub(super) fn changed(
    session: &str,
    state: &mut ProjectState,
    project: &ProjectSession,
    affected: Value,
    events: &mut Vec<Value>,
) -> Result<(), Value> {
    state.event = state
        .event
        .checked_add(1)
        .ok_or_else(|| domain("generation_exhausted", json!({})))?;
    let mut notification = json!({"jsonrpc":"2.0","method":"metadata/changed","params":{"sessionId":session,"projectId":state.id,"generation":project.generation().to_string(),"eventSequence":state.event.to_string(),"affected":null,"requiresRefresh":state.refresh,"requiresReopen":state.reopen}});
    notification["params"]["affected"] = affected;
    if serde_json::to_vec(&notification)
        .map_or(true, |body| body.len() > super::framing::MAX_RESPONSE)
    {
        notification["params"]["affected"] = Value::Null;
    }
    events.push(notification);
    Ok(())
}
