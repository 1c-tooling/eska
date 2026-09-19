//! One serial domain worker owns the complete active discovery context.

use super::{
    dto,
    envelope::{domain, error},
    errors, params,
};
use crate::project::{
    metadata_model::ProjectScope,
    metadata_workspace::{MetadataWorkspace, search::IndexState},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

pub(super) struct ProjectState {
    pub id: String,
    pub scope: ProjectScope,
    pub event: u64,
    pub file: u64,
    pub refresh: bool,
    pub reopen: bool,
    pub last_progress: Option<(Value, Instant)>,
}
pub(super) struct Session {
    pub id: String,
    pub workspace: MetadataWorkspace,
    pub projects: Vec<ProjectState>,
}
pub(super) struct Server {
    pub initialized: bool,
    pub closing: bool,
    pub session: Option<Session>,
    pub labels: dto::Labels,
    serial: u64,
    round_robin: usize,
}

impl Server {
    /// Construct locale-independent protocol state without discovering a project.
    pub(super) fn new() -> Result<Self, crate::cli::localization::LocalizationError> {
        Ok(Self {
            initialized: false,
            closing: false,
            session: None,
            labels: dto::Labels::new()?,
            serial: 0,
            round_robin: 0,
        })
    }

    /// Dispatch lifecycle requests or delegate to the metadata adapter.
    pub(super) fn execute(
        &mut self,
        method: &str,
        args: &Value,
        events: &mut Vec<Value>,
    ) -> Result<Value, Value> {
        if self.closing || (!self.initialized && method != "initialize") {
            return Err(domain(
                "invalid_state",
                json!({"state":if self.closing { "closing" } else { "uninitialized" }}),
            ));
        }
        if !args.is_object() {
            return Err(error(-32602));
        }
        let optional = match method {
            "initialize" => &["locale"][..],
            "workspace/open" => &["diskCache"],
            "metadata/children" => &["hideEmptyRootSections"],
            "metadata/search" => &["limit", "synonymLanguage"],
            "metadata/refresh" => &["resetFileSequence"],
            "metadata/indexErrors" => &["offset", "limit"],
            _ => &[],
        };
        params::optional_fields(args, optional)?;
        match method {
            "initialize" => self.initialize(args),
            "workspace/open" => self.open(args),
            "project/info" => {
                self.check_session(args)?;
                self.info()
            }
            "workspace/close" => {
                self.check_session(args)?;
                self.session = None;
                Ok(Value::Null)
            }
            "shutdown" => {
                self.session = None;
                self.closing = true;
                Ok(Value::Null)
            }
            method if super::metadata::is_method(method) => self.metadata(method, args, events),
            _ => Err(error(-32601)),
        }
    }

    /// Negotiate API independently of the crate version and retain only the handshake state.
    fn initialize(&mut self, args: &Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        struct Version {
            major: u32,
            minor: u32,
        }
        #[derive(Deserialize)]
        struct Client {
            #[serde(rename = "name")]
            _name: String,
            #[serde(rename = "version")]
            _version: String,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Init {
            api_version: Version,
            #[serde(rename = "client")]
            _client: Client,
            locale: Option<String>,
        }
        if self.initialized {
            return Err(domain("invalid_state", json!({"state":"initialized"})));
        }
        let input: Init = params::decode(args)?;
        if input
            .locale
            .as_deref()
            .is_some_and(|locale| !["ru-RU", "en-US"].contains(&locale))
        {
            return Err(error(-32602));
        }
        if input.api_version.major != 1 || input.api_version.minor > 1 {
            return Err(domain(
                "unsupported_version",
                json!({"requested":args["apiVersion"],"supported":{"major":1,"minor":1}}),
            ));
        }
        self.initialized = true;
        Ok(
            json!({"apiVersion":{"major":1,"minor":1},"server":{"name":"eska","version":env!("CARGO_PKG_VERSION")},"capabilities":{"selfUpdate":true,"designerXml":true,"readOnly":true,"diskCache":true,"search":true,"clientFileEvents":true,"batch":true,"multiContext":false,"supportedProjectTypes":["configuration","extension","processing","report"]},"limits":{"maxHeaderBytes":8192,"maxRequestBytes":1_048_576,"maxResponseBytes":67_108_864,"maxDepth":64,"maxBatchItems":16,"maxPendingRequests":128,"maxQueuedBytes":4_194_304}}),
        )
    }

    /// Publish a session only after all selected projects have opened successfully.
    fn open(&mut self, args: &Value) -> Result<Value, Value> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "lowercase")]
        enum Selection {
            Current,
            All,
            Named { names: Vec<String> },
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Open {
            start: params::Path,
            selection: Selection,
            disk_cache: Option<bool>,
        }
        if self.session.is_some() {
            return Err(domain("workspace_already_open", json!({"state":"open"})));
        }
        let input: Open = params::decode(args)?;
        let start = input.start.native()?;
        if !start.is_absolute() {
            return Err(error(-32602));
        }
        let (names, all) = match input.selection {
            Selection::Current => (Vec::new(), false),
            Selection::All => (Vec::new(), true),
            Selection::Named { names } => {
                if names.is_empty()
                    || names.iter().any(String::is_empty)
                    || names.iter().collect::<std::collections::HashSet<_>>().len() != names.len()
                {
                    return Err(domain(
                        "project_open_failed",
                        json!({"reason":"selection_invalid"}),
                    ));
                }
                (names, false)
            }
        };
        let workspace = if input.disk_cache.unwrap_or(true) {
            MetadataWorkspace::open_cached(&start, &names, all)
        } else {
            MetadataWorkspace::open(&start, &names, all)
        }
        .map_err(|failure| errors::opening(&failure))?;
        self.serial = self
            .serial
            .checked_add(1)
            .ok_or_else(|| domain("generation_exhausted", json!({})))?;
        let projects = workspace
            .projects()
            .iter()
            .enumerate()
            .map(|(index, project)| ProjectState {
                id: format!("p{index}"),
                scope: project.scope().clone(),
                event: 0,
                file: 0,
                refresh: false,
                reopen: false,
                last_progress: None,
            })
            .collect();
        self.session = Some(Session {
            id: format!("s{}", self.serial),
            workspace,
            projects,
        });
        self.info()
    }

    /// Validate session tokens before resolving any project-local identity.
    pub(super) fn check_session(&self, args: &Value) -> Result<(), Value> {
        let input: params::Session = params::decode(args)?;
        if self
            .session
            .as_ref()
            .is_none_or(|session| session.id != input.session_id)
        {
            return Err(domain(
                "unknown_session",
                json!({"sessionId":input.session_id}),
            ));
        }
        Ok(())
    }

    /// Report fresh counters and validated project paths without reading source files.
    fn info(&self) -> Result<Value, Value> {
        let session = self.session.as_ref().ok_or_else(|| error(-32603))?;
        let projects = session.projects.iter().map(|state| {
            let project = session.workspace.project(&state.scope).map_err(|failure| errors::workspace(&failure))?;
            let scope = match &state.scope { ProjectScope::Standalone => json!({"kind":"standalone"}), ProjectScope::Member(name) => json!({"kind":"member","name":name.as_str()}) };
            Ok(json!({"projectId":state.id,"scope":scope,"type":project.project().configuration().project_type().as_str(),"rootPath":dto::path(project.project().root()),"sourcePath":dto::path(project.project().source()),"root":dto::node_id(project.root()),"generation":project.generation().to_string(),"eventSequence":state.event.to_string(),"requiresRefresh":state.refresh,"requiresReopen":state.reopen}))
        }).collect::<Result<Vec<_>, Value>>()?;
        Ok(json!({"sessionId":session.id,"projects":projects}))
    }

    /// Index at most one descriptor and rotate projects so no member monopolizes idle time.
    pub(super) fn index_step(&mut self, events: &mut Vec<Value>) -> bool {
        let Some(session) = &mut self.session else {
            return false;
        };
        for _ in 0..session.projects.len() {
            let index = self.round_robin % session.projects.len();
            self.round_robin = self.round_robin.wrapping_add(1);
            let state = &mut session.projects[index];
            if state.refresh || state.reopen {
                continue;
            }
            let Ok(project) = session.workspace.project_mut(&state.scope) else {
                continue;
            };
            if project.search_progress().state != IndexState::Building {
                continue;
            }
            project.index_search_step(1);
            Self::progress_event(&session.id, state, project, events);
            return true;
        }
        false
    }

    /// State changes are immediate; count-only updates are coalesced to ten per second.
    pub(super) fn progress_event(
        id: &str,
        state: &mut ProjectState,
        project: &crate::project::metadata_workspace::ProjectSession,
        events: &mut Vec<Value>,
    ) {
        let progress = dto::progress(&project.search_progress());
        let now = Instant::now();
        let send = state.last_progress.as_ref().is_none_or(|(previous, sent)| {
            previous["state"] != progress["state"]
                || (*previous != progress
                    && now.duration_since(*sent) >= Duration::from_millis(100))
        });
        if send {
            events.push(json!({"jsonrpc":"2.0","method":"metadata/indexProgress","params":{"sessionId":id,"projectId":state.id,"generation":project.generation().to_string(),"progress":progress}}));
            state.last_progress = Some((progress, now));
        }
    }
}
