//! A query session owns its draft, resume decision and one native-application receipt, independently of an IDE.

use std::{path::PathBuf, sync::Arc};

use crate::{
    application::{self, ContextError, ContextSnapshot},
    project::{metadata_edit::EditPlan, metadata_model::ProjectScope, query},
};

use super::{
    CommentPlacement, NodeId, ProjectSession, Query, QueryDocument, QueryDocumentError,
    QueryDocumentKind, QueryError, QueryIdentity, QueryRevision, snapshot,
};

pub use query::QueryOwnerFormat;

/// Native restrictions are independent facts; the source lease belongs to the query session.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct QueryOwnerRestrictions {
    pub user_read_only: bool,
}

/// Only the IDE knows native versions and dirty markers; source hashes and permissions are checked by ESKA.
#[derive(Clone, Copy, Debug)]
pub struct QueryEditorState<'a> {
    pub version: u64,
    pub snapshot: &'a str,
    pub is_dirty: bool,
    pub user_read_only: bool,
    pub application: Option<&'a str>,
}

/// A retained checkpoint is inert data; restoring it never grants permission to apply automatically.
#[derive(Clone, Debug)]
pub struct QuerySessionRestore {
    pub root: PathBuf,
    pub scope: ProjectScope,
    pub node: NodeId,
    pub path: PathBuf,
    pub descriptor: PathBuf,
    pub descriptor_snapshot: String,
    pub disk_snapshot: String,
    pub owner_snapshot: String,
    pub owner_version: u64,
    pub context_fingerprint: String,
    pub application: Option<String>,
    pub identity: QueryIdentity,
    pub owner_text: Arc<str>,
    pub base: Arc<str>,
    pub draft: Arc<str>,
    pub writable: bool,
    pub prepared: Option<query::recovery::Prepared>,
}

/// A plan is not a source write; its token must be acknowledged against the resulting native model.
#[derive(Debug)]
pub struct QueryApplyPlan {
    pub token: u64,
    pub save_owner: bool,
    pub plan: EditPlan,
    pub query: Query,
}

/// Structured failures keep editor presentation and transport details outside the session model.
#[derive(Debug)]
pub enum QuerySessionError {
    Document(Box<QueryDocumentError>),
    Context(Box<ContextError>),
    ContextMemberMismatch,
    ContextChanged,
    ResumeRequired,
    OwnerBusy,
    UserReadOnly,
    StaleDraft,
    ApplyBusy,
    UnknownApply,
    InvalidReceipt,
    Closed,
    RevisionExhausted,
}

impl From<QueryDocumentError> for QuerySessionError {
    /// Preserve the checked source boundary instead of flattening it to a display message.
    fn from(error: QueryDocumentError) -> Self {
        Self::Document(Box::new(error))
    }
}

impl From<QueryError> for QuerySessionError {
    /// Codec errors remain distinguishable from lifecycle and context failures.
    fn from(error: QueryError) -> Self {
        QueryDocumentError::Query(error).into()
    }
}

#[derive(Clone, Debug)]
struct Pending {
    token: u64,
    output: Arc<str>,
    identity: QueryIdentity,
    applied_draft: Arc<str>,
    save_owner: bool,
    no_op: bool,
}

#[derive(Clone, Debug)]
enum Phase {
    Paused,
    Ready,
    Applying(Pending),
    Recovering(Pending),
    Closed,
}

/// Immutable text sharing permits a proposed state to be published only after its checkpoint CAS succeeds.
#[derive(Clone, Debug)]
pub struct QuerySession {
    document: QueryDocument,
    application: Option<String>,
    fingerprint: String,
    identity: QueryIdentity,
    base: Arc<str>,
    draft: Arc<str>,
    format: QueryOwnerFormat,
    restrictions: QueryOwnerRestrictions,
    source_restriction: Option<super::super::source_protection::SourceRestriction>,
    phase: Phase,
    revision: u64,
    plan_serial: u64,
}

impl QuerySession {
    /// Capture the exact owner and fence competing backend writes; native edits are checked before application.
    ///
    /// # Errors
    /// Rejects stale source/context, an unsupported query and a competing source session.
    pub fn open(
        project: &mut ProjectSession,
        document: QueryDocument,
        identity: QueryIdentity,
        application: Option<&str>,
        format: QueryOwnerFormat,
        restrictions: QueryOwnerRestrictions,
    ) -> Result<Self, QuerySessionError> {
        let context = project.query_context(application)?;
        let query = selected(&document, &identity)?;
        project.check_query_read(&document, revision(&document))?;
        let application = context.context.name().map(|name| name.as_str().to_owned());
        let base: Arc<str> = query.text.into();
        let source_restriction = project
            .source_protections
            .acquire(&document.path, &document.descriptor)
            .ok_or(QuerySessionError::OwnerBusy)?;
        let session = Self {
            document,
            application,
            fingerprint: context.fingerprint,
            identity,
            draft: Arc::clone(&base),
            base,
            format,
            restrictions,
            source_restriction: Some(source_restriction),
            phase: Phase::Ready,
            revision: 1,
            plan_serial: 0,
        };
        session.check_context(project, session.application.as_deref())?;
        Ok(session)
    }

    /// Recovered source text remains available even when current source bytes cannot be resumed.
    ///
    /// # Errors
    /// Rejects a foreign project, malformed saved source binding or inconsistent base query.
    pub fn restore(
        project: &mut ProjectSession,
        saved: QuerySessionRestore,
        format: QueryOwnerFormat,
    ) -> Result<Self, QuerySessionError> {
        if saved.root != project.project().root() || &saved.scope != project.scope() {
            return Err(QueryDocumentError::OwnerChanged.into());
        }
        if saved.owner_text.len() > query::MAX_SNAPSHOT_BYTES
            || saved.base.len() > query::MAX_SNAPSHOT_BYTES
            || saved.draft.len() > query::MAX_SNAPSHOT_BYTES
        {
            return Err(QueryError::TooLarge.into());
        }
        if saved.owner_snapshot != snapshot(&saved.owner_text) {
            return Err(QueryError::Conflict.into());
        }
        reveal_owner(project, &saved.node)?;
        let mut document = project.query_document_shared(
            &saved.node,
            &saved.path,
            saved.owner_version,
            Some(saved.owner_text),
        )?;
        if document.descriptor != saved.descriptor {
            return Err(QueryDocumentError::OwnerChanged.into());
        }
        if selected(&document, &saved.identity)?.text.as_str() != saved.base.as_ref() {
            return Err(QueryError::Conflict.into());
        }
        // These are remembered guards, not fresh permission. Every resume/plan resolves a new checked document.
        document.descriptor_snapshot = saved.descriptor_snapshot;
        document.disk_snapshot = saved.disk_snapshot;
        document.writable = saved.writable;
        let pending = saved
            .prepared
            .map(|prepared| restore_pending(&document, &saved.identity, format, prepared))
            .transpose()?;
        let plan_serial = pending.as_ref().map_or(0, |pending| pending.token);
        let source_restriction = project
            .source_protections
            .acquire(&document.path, &document.descriptor)
            .ok_or(QuerySessionError::OwnerBusy)?;
        Ok(Self {
            document,
            application: saved.application,
            fingerprint: saved.context_fingerprint,
            identity: saved.identity,
            base: saved.base,
            draft: saved.draft,
            format,
            restrictions: QueryOwnerRestrictions::default(),
            source_restriction: Some(source_restriction),
            phase: pending.map_or(Phase::Paused, Phase::Recovering),
            revision: 1,
            plan_serial,
        })
    }

    /// Read the remembered owner without treating its initial writable flag as present authority.
    #[must_use]
    pub const fn document(&self) -> &QueryDocument {
        &self.document
    }

    /// This backend restriction blocks competing ESKA writes; it is not a native-editor lock acknowledgement.
    #[must_use]
    pub fn source_restriction_id(&self) -> Option<u64> {
        self.source_restriction
            .as_ref()
            .map(super::super::source_protection::SourceRestriction::id)
    }

    /// Read-only parent views use the captured owner even if an ordinary native buffer or disk has since changed.
    ///
    /// # Errors
    /// Rejects a closed or foreign session and unavailable source policy; no source write is authorized.
    pub fn source_snapshot(
        &self,
        project: &mut ProjectSession,
    ) -> Result<super::super::SourceSnapshot, super::super::SourceSnapshotError> {
        self.check_project(project)
            .map_err(|_| super::super::SourceSnapshotError::UnknownSource)?;
        let request = super::super::SourceAccessRequest {
            path: self.document.path.clone(),
            user_read_only: self.restrictions.user_read_only,
        };
        let protection = project
            .source_protection(&[request], true)?
            .pop()
            .ok_or(super::super::SourceSnapshotError::UnknownSource)?;
        Ok(super::super::SourceSnapshot::retained_owner(
            protection,
            Arc::clone(&self.document.input),
            self.document.version,
        ))
    }

    /// Query identity always belongs to the retained full-owner snapshot.
    #[must_use]
    pub const fn identity(&self) -> &QueryIdentity {
        &self.identity
    }

    /// The baseline changes only after a checked native application receipt.
    #[must_use]
    pub fn base(&self) -> &str {
        &self.base
    }

    /// Unfinished query text is retained independently of its source representation.
    #[must_use]
    pub fn draft(&self) -> &str {
        &self.draft
    }

    /// Private checkpoints share the three immutable texts instead of copying the full owner on each edit.
    #[must_use]
    pub fn texts(&self) -> [Arc<str>; 3] {
        [
            Arc::clone(&self.document.input),
            Arc::clone(&self.base),
            Arc::clone(&self.draft),
        ]
    }

    /// Draft editing preserves source restrictions without changing native editor overrides.
    #[must_use]
    pub const fn draft_writable(&self) -> bool {
        !matches!(self.phase, Phase::Closed)
            && self.document.writable
            && !self.restrictions.user_read_only
    }

    /// A selected application never silently follows a later client preference.
    #[must_use]
    pub fn application(&self) -> Option<&str> {
        self.application.as_deref()
    }

    /// Context inputs, rather than a generated analyzer configuration, identify the session.
    #[must_use]
    pub fn context_fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Revisions compare acknowledged state rather than native per-process editor counters.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Recovery and a context replacement require explicit resumption before another source plan.
    #[must_use]
    pub const fn resumed(&self) -> bool {
        matches!(self.phase, Phase::Ready | Phase::Applying(_))
    }

    /// Only one native application may await a receipt for this session.
    #[must_use]
    pub const fn applying(&self) -> bool {
        matches!(self.phase, Phase::Applying(_) | Phase::Recovering(_))
    }

    /// A durable intent describes exact past preparation; recovery never turns it into automatic source application.
    #[must_use]
    pub fn prepared(&self) -> Option<query::recovery::Prepared> {
        let (Phase::Applying(pending) | Phase::Recovering(pending)) = &self.phase else {
            return None;
        };
        Some(query::recovery::Prepared {
            token: pending.token,
            format: self.format,
            output: Arc::clone(&pending.output),
            applied_draft: Arc::clone(&pending.applied_draft),
            save_owner: pending.save_owner,
            no_op: pending.no_op,
        })
    }

    /// The adapter declares how its exact native buffer represents the encoding signature.
    #[must_use]
    pub const fn owner_format(&self) -> QueryOwnerFormat {
        self.format
    }

    /// Independent user restrictions are retained after this session releases its own lock.
    #[must_use]
    pub const fn restrictions(&self) -> QueryOwnerRestrictions {
        self.restrictions
    }

    /// Include an unfinished plan's output and an older applied draft still awaiting its receipt.
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.document.input.len()
            + self.base.len()
            + self.draft.len()
            + match &self.phase {
                Phase::Applying(pending) | Phase::Recovering(pending) => {
                    let retained_draft = if Arc::ptr_eq(&pending.applied_draft, &self.draft)
                        || Arc::ptr_eq(&pending.applied_draft, &self.base)
                    {
                        0
                    } else {
                        pending.applied_draft.len()
                    };
                    retained_draft
                        + if pending.no_op {
                            0
                        } else {
                            pending.output.len()
                        }
                }
                _ => 0,
            }
    }

    /// Coalesced transport updates can replace only the draft, never its owner or baseline.
    ///
    /// # Errors
    /// Rejects stale revisions, oversized drafts and callbacks for closed sessions.
    pub fn set_draft(&mut self, expected: u64, draft: Arc<str>) -> Result<(), QuerySessionError> {
        self.check_revision(expected)?;
        if draft.len() > query::MAX_SNAPSHOT_BYTES {
            return Err(QueryError::TooLarge.into());
        }
        if self.draft != draft {
            self.advance()?;
            self.draft = draft;
        }
        Ok(())
    }

    /// Rebind a remembered buffer only by explicit user intent, including a separately saved matching XML owner.
    ///
    /// # Errors
    /// Rejects changed owner/descriptor/context and current restrictions.
    pub fn resume(
        &mut self,
        project: &mut ProjectSession,
        current: QueryEditorState<'_>,
        restrictions: QueryOwnerRestrictions,
    ) -> Result<(), QuerySessionError> {
        self.not_closed()?;
        if matches!(self.phase, Phase::Applying(_)) {
            return Err(QuerySessionError::ApplyBusy);
        }
        let recovered = if let Phase::Recovering(pending) = &self.phase {
            Some(pending.clone())
        } else {
            None
        };
        if recovered.is_none() {
            self.phase = Phase::Paused;
        }
        self.restrictions.user_read_only = restrictions.user_read_only;
        if restrictions.user_read_only || current.user_read_only {
            self.restrictions.user_read_only = true;
            return Err(QuerySessionError::UserReadOnly);
        }
        if let Some(pending) = recovered
            && current.snapshot != self.document.snapshot
        {
            self.restrictions = restrictions;
            self.phase = Phase::Applying(pending.clone());
            let result = self.acknowledge(project, pending.token, current, true);
            if result.is_err() {
                self.phase = Phase::Recovering(pending);
            }
            return result;
        }
        let document = self.checked_document(project, current, true)?;
        self.advance()?;
        self.document = document;
        self.restrictions = restrictions;
        self.phase = Phase::Ready;
        Ok(())
    }

    /// Return a minimal plan and retain its exact output until the native editor acknowledges the result.
    ///
    /// # Errors
    /// Rejects unresumed/stale/forbidden sessions and competing application attempts.
    pub fn prepare(
        &mut self,
        project: &mut ProjectSession,
        expected: u64,
        current: QueryEditorState<'_>,
        decisions: &[CommentPlacement],
        line_ending: Option<query::LineEnding>,
    ) -> Result<QueryApplyPlan, QuerySessionError> {
        self.check_revision(expected)?;
        match self.phase {
            Phase::Paused | Phase::Recovering(_) => return Err(QuerySessionError::ResumeRequired),
            Phase::Applying(_) => return Err(QuerySessionError::ApplyBusy),
            Phase::Closed => return Err(QuerySessionError::Closed),
            Phase::Ready => (),
        }
        if current.user_read_only {
            self.restrictions.user_read_only = true;
        }
        let checked = self.checked_plan(project, current, decisions, line_ending);
        let (document, plan, projected) = match checked {
            Ok(checked) => checked,
            Err(error) => {
                self.phase = Phase::Paused;
                return Err(error);
            }
        };
        let token = self
            .plan_serial
            .checked_add(1)
            .ok_or(QuerySessionError::RevisionExhausted)?;
        let save_owner =
            document.kind != QueryDocumentKind::Bsl && !current.is_dirty && !plan.is_empty();
        self.phase = Phase::Applying(Pending {
            token,
            output: if plan.is_empty() {
                Arc::clone(&self.document.input)
            } else {
                Arc::from(plan.output())
            },
            identity: projected.identity.clone(),
            applied_draft: if projected.text.as_str() == self.draft.as_ref() {
                Arc::clone(&self.draft)
            } else if projected.text.as_str() == self.base.as_ref() {
                Arc::clone(&self.base)
            } else {
                Arc::from(projected.text.as_str())
            },
            save_owner,
            no_op: plan.is_empty(),
        });
        self.plan_serial = token;
        Ok(QueryApplyPlan {
            token,
            save_owner,
            plan,
            query: projected,
        })
    }

    /// A valid receipt advances the owner/base while preserving a newer draft typed during application.
    ///
    /// # Errors
    /// Rejects a wrong token/output, stale native version and unexpected disk/context/policy changes.
    pub fn applied(
        &mut self,
        project: &mut ProjectSession,
        token: u64,
        current: QueryEditorState<'_>,
    ) -> Result<(), QuerySessionError> {
        self.acknowledge(project, token, current, false)
    }

    /// Explicit crash reconciliation accepts fresh native counters, including an applied but unsaved XML model.
    fn acknowledge(
        &mut self,
        project: &mut ProjectSession,
        token: u64,
        current: QueryEditorState<'_>,
        recovered: bool,
    ) -> Result<(), QuerySessionError> {
        self.not_closed()?;
        if current.user_read_only {
            self.restrictions.user_read_only = true;
        }
        self.check_restrictions(current)?;
        let pending = match &self.phase {
            Phase::Applying(pending) if pending.token == token => pending,
            _ => return Err(QuerySessionError::UnknownApply),
        };
        if current.snapshot != snapshot(&pending.output)
            || (!recovered
                && (current.version < self.document.version
                    || (!pending.no_op && current.version == self.document.version)))
        {
            return Err(QuerySessionError::InvalidReceipt);
        }
        self.check_project(project)?;
        self.check_context(project, current.application)?;
        let document = project.query_document_shared(
            &self.document.node,
            &self.document.path,
            current.version,
            Some(Arc::clone(&pending.output)),
        )?;
        self.check_descriptor(&document)?;
        if !self.document.writable || !document.writable {
            return Err(QueryDocumentError::ReadOnly.into());
        }
        let output_disk = disk_snapshot(&pending.output, self.document.disk_bom, self.format);
        if !current.is_dirty && document.disk_snapshot != output_disk {
            return Err(QueryError::Conflict.into());
        }
        let saved = pending.save_owner && !(recovered && current.is_dirty);
        if (saved && document.disk_snapshot != output_disk)
            || (!saved
                && document.disk_snapshot != self.document.disk_snapshot
                && (current.is_dirty || document.disk_snapshot != output_disk))
        {
            return Err(QueryError::Conflict.into());
        }
        let base = Arc::clone(&pending.applied_draft);
        let identity = pending.identity.clone();
        self.check_context(project, current.application)?;
        self.advance()?;
        self.document = document;
        self.identity = identity;
        self.base = base;
        self.phase = Phase::Ready;
        Ok(())
    }

    /// Errors/cancel release only this planned operation; later application needs another explicit resume.
    ///
    /// # Errors
    /// Rejects a token belonging to a different or already released application.
    pub fn abort_apply(&mut self, token: u64) -> Result<(), QuerySessionError> {
        self.not_closed()?;
        if !matches!(&self.phase, Phase::Applying(pending) | Phase::Recovering(pending) if pending.token == token)
        {
            return Err(QuerySessionError::UnknownApply);
        }
        self.phase = Phase::Paused;
        Ok(())
    }

    /// A source connection replacement invalidates authority while keeping both dirty snapshots intact.
    pub fn pause(&mut self) {
        self.phase = match &self.phase {
            Phase::Applying(pending) | Phase::Recovering(pending) => {
                Phase::Recovering(pending.clone())
            }
            Phase::Closed => Phase::Closed,
            _ => Phase::Paused,
        };
    }

    /// Only the owned query restriction is released; the user's independent restriction remains unchanged.
    pub fn close(&mut self) -> Option<u64> {
        if matches!(self.phase, Phase::Closed) {
            return None;
        }
        self.phase = Phase::Closed;
        self.source_restriction
            .take()
            .map(|restriction| restriction.id())
    }

    /// Every operation resolves current source ownership rather than reusing a stale metadata generation.
    fn checked_plan(
        &self,
        project: &mut ProjectSession,
        current: QueryEditorState<'_>,
        decisions: &[CommentPlacement],
        line_ending: Option<query::LineEnding>,
    ) -> Result<(QueryDocument, EditPlan, Query), QuerySessionError> {
        self.check_restrictions(current)?;
        let document = self.checked_document(project, current, false)?;
        project.check_query_source(
            &document,
            revision(&document),
            self.source_restriction.as_ref(),
        )?;
        let plan = document.plan(&self.identity, &self.draft, decisions, line_ending)?;
        project.check_query_source(
            &document,
            revision(&document),
            self.source_restriction.as_ref(),
        )?;
        self.check_context(project, current.application)?;
        let query = projected(&plan, &self.identity)?;
        Ok((document, plan, query))
    }

    /// Every operation resolves current source ownership rather than reusing a stale metadata generation.
    fn checked_document(
        &self,
        project: &mut ProjectSession,
        current: QueryEditorState<'_>,
        resuming: bool,
    ) -> Result<QueryDocument, QuerySessionError> {
        if current.snapshot != self.document.snapshot
            || (!resuming && current.version != self.document.version)
        {
            return Err(QueryDocumentError::StaleVersion.into());
        }
        self.check_project(project)?;
        self.check_context(project, current.application)?;
        let document = project.query_document_shared(
            &self.document.node,
            &self.document.path,
            current.version,
            Some(Arc::clone(&self.document.input)),
        )?;
        self.check_descriptor(&document)?;
        if !self.document.writable || !document.writable {
            return Err(QueryDocumentError::ReadOnly.into());
        }
        let matching_disk = document.disk_snapshot
            == disk_snapshot(&document.input, document.disk_bom, self.format);
        if (!current.is_dirty && !matching_disk)
            || (document.disk_snapshot != self.document.disk_snapshot
                && (!resuming || current.is_dirty || !matching_disk))
        {
            return Err(QueryError::Conflict.into());
        }
        if selected(&document, &self.identity)?.text.as_str() != self.base.as_ref() {
            return Err(QueryError::Conflict.into());
        }
        self.check_context(project, current.application)?;
        Ok(document)
    }

    /// Logical identity and full descriptor bytes are checked independently from application composition.
    fn check_descriptor(&self, document: &QueryDocument) -> Result<(), QuerySessionError> {
        if document.kind != self.document.kind
            || document.descriptor != self.document.descriptor
            || document.descriptor_snapshot != self.document.descriptor_snapshot
        {
            return Err(QueryDocumentError::OwnerChanged.into());
        }
        Ok(())
    }

    /// Restored data cannot be rebound to a different project/source/member merely because node IDs match.
    fn check_project(&self, project: &ProjectSession) -> Result<(), QuerySessionError> {
        if self.document.root != project.project().root()
            || self.document.source_root != project.project().source()
            || &self.document.scope != project.scope()
            || !self
                .source_restriction
                .as_ref()
                .is_some_and(|restriction| project.source_protections.owns(restriction))
        {
            return Err(QueryDocumentError::OwnerChanged.into());
        }
        Ok(())
    }

    /// Keep the same context across changed paths, a new connection and independent analyzer configuration.
    fn check_context(
        &self,
        project: &ProjectSession,
        requested: Option<&str>,
    ) -> Result<(), QuerySessionError> {
        if project.query_context(requested)?.fingerprint != self.fingerprint {
            return Err(QuerySessionError::ContextChanged);
        }
        Ok(())
    }

    /// A query-owned restriction never overrides the user's independent read-only state.
    const fn check_restrictions(
        &self,
        current: QueryEditorState<'_>,
    ) -> Result<(), QuerySessionError> {
        if self.restrictions.user_read_only || current.user_read_only {
            return Err(QuerySessionError::UserReadOnly);
        }
        Ok(())
    }

    /// All mutable callbacks must identify the acknowledged draft revision.
    fn check_revision(&self, expected: u64) -> Result<(), QuerySessionError> {
        self.not_closed()?;
        if expected != self.revision {
            return Err(QuerySessionError::StaleDraft);
        }
        Ok(())
    }

    /// A released session cannot be revived by a delayed save or receipt.
    const fn not_closed(&self) -> Result<(), QuerySessionError> {
        if matches!(self.phase, Phase::Closed) {
            return Err(QuerySessionError::Closed);
        }
        Ok(())
    }

    /// Checked advancement makes revision exhaustion an error rather than an alias of older state.
    fn advance(&mut self) -> Result<(), QuerySessionError> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(QuerySessionError::RevisionExhausted)?;
        Ok(())
    }
}

/// A private checkpoint is untrusted: rediscover only its declared ancestry and verify the actual module role.
fn reveal_owner(project: &mut ProjectSession, node: &NodeId) -> Result<(), QuerySessionError> {
    if project.node(node).is_ok() {
        return Ok(());
    }
    let owner = match node {
        NodeId::Object(owner) | NodeId::Module { owner, .. } => owner,
        NodeId::Collection { .. } => return Err(QueryDocumentError::OwnerChanged.into()),
    };
    project
        .reveal_declared_object(owner)
        .map_err(QueryDocumentError::from)?;
    if matches!(node, NodeId::Module { .. }) {
        project
            .children(
                &NodeId::Object(owner.clone()),
                crate::project::configurator::TreeOptions::default(),
            )
            .map_err(QueryDocumentError::from)?;
    }
    project.node(node).map_err(QueryDocumentError::from)?;
    Ok(())
}

impl ProjectSession {
    /// Resolve current application inputs without requiring an analyzer or generated configuration.
    ///
    /// # Errors
    /// Rejects unavailable composition and a selected project outside the resolved application.
    pub fn query_context(
        &self,
        application: Option<&str>,
    ) -> Result<ContextSnapshot, QuerySessionError> {
        let context = application::resolve_snapshot(self.project().root(), application)
            .map_err(|error| QuerySessionError::Context(Box::new(error)))?;
        if !context.context.sources().iter().any(|source| {
            source.project().root() == self.project().root()
                && source.project().source() == self.project().source()
        }) {
            return Err(QuerySessionError::ContextMemberMismatch);
        }
        Ok(context)
    }
}

/// Codec reads never accept a caller-supplied range/value as a replacement for checked source syntax.
fn selected(document: &QueryDocument, identity: &QueryIdentity) -> Result<Query, QueryError> {
    if !document.accepts(identity) {
        return Err(QueryError::UnsupportedStructure);
    }
    let query = match identity {
        QueryIdentity::BslLiteral(range) => document.bsl_query(range.start)?,
        _ => document
            .xml_queries()?
            .into_iter()
            .find(|query| &query.identity == identity)
            .ok_or(QueryError::NotFound)?,
    };
    if &query.identity != identity {
        return Err(QueryError::Conflict);
    }
    Ok(query)
}

/// The BSL identity's end changes after an edit; XML logical identity remains stable.
fn projected(plan: &EditPlan, identity: &QueryIdentity) -> Result<Query, QueryError> {
    projected_text(plan.output(), identity)
}

/// Saved output is re-extracted through the same codec; a remembered range cannot inject a different baseline.
fn projected_text(text: &str, identity: &QueryIdentity) -> Result<Query, QueryError> {
    match identity {
        QueryIdentity::BslLiteral(range) => query::read_bsl(text, range.start),
        _ => query::read_xml(text)?
            .into_iter()
            .find(|query| &query.identity == identity)
            .ok_or(QueryError::NotFound),
    }
}

/// Inert storage cannot supply a contradictory output, a foreign encoding model or an invented applied query.
fn restore_pending(
    document: &QueryDocument,
    identity: &QueryIdentity,
    format: QueryOwnerFormat,
    prepared: query::recovery::Prepared,
) -> Result<Pending, QuerySessionError> {
    prepared.validate().map_err(|_| QueryError::Conflict)?;
    if prepared.format != format
        || prepared.no_op != (prepared.output == document.input)
        || (prepared.save_owner && document.kind == QueryDocumentKind::Bsl)
    {
        return Err(QueryError::Conflict.into());
    }
    let projected = projected_text(&prepared.output, identity)?;
    if projected.text.as_str() != prepared.applied_draft.as_ref() {
        return Err(QueryError::Conflict.into());
    }
    Ok(Pending {
        token: prepared.token,
        output: prepared.output,
        identity: projected.identity,
        applied_draft: prepared.applied_draft,
        save_owner: prepared.save_owner,
        no_op: prepared.no_op,
    })
}

/// An encoding signature is restored only when the supplied editor model deliberately omits it.
fn disk_snapshot(text: &str, bom: bool, format: QueryOwnerFormat) -> String {
    use sha2::{Digest as _, Sha256};
    if bom && format == QueryOwnerFormat::Editor {
        use std::fmt::Write as _;
        Sha256::new()
            .chain_update(b"\xef\xbb\xbf")
            .chain_update(text.as_bytes())
            .finalize()
            .iter()
            .fold(String::with_capacity(64), |mut value, byte| {
                let _ = write!(value, "{byte:02x}");
                value
            })
    } else {
        snapshot(text)
    }
}

/// Source revisions and draft revisions deliberately use different counters.
fn revision(document: &QueryDocument) -> QueryRevision<'_> {
    QueryRevision {
        version: document.version,
        snapshot: &document.snapshot,
    }
}
