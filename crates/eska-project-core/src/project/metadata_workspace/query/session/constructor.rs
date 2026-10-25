//! Exclusive draft ownership and constructor transitions stay in the common session model.

use std::sync::Arc;

use super::{Phase, QuerySession, QuerySessionError, query};
use crate::project::metadata_edit::{EditPlan, snapshot};

impl QuerySession {
    /// Acquire only this draft revision; neither source ownership nor a native read-only override is replaced.
    ///
    /// # Errors
    /// Rejects stale, closed, unresumed or already-owned drafts and resource exhaustion.
    pub fn open_constructor(&mut self, expected: u64) -> Result<u64, QuerySessionError> {
        self.check_revision(expected)?;
        self.check_constructor_idle()?;
        match self.phase {
            Phase::Ready => (),
            Phase::Applying(_) | Phase::Recovering(_) => return Err(QuerySessionError::ApplyBusy),
            _ => return Err(QuerySessionError::ResumeRequired),
        }
        let value = query::constructor::Constructor::new(Arc::clone(&self.draft))
            .map_err(QuerySessionError::Constructor)?;
        let token = self
            .plan_serial
            .checked_add(1)
            .ok_or(QuerySessionError::RevisionExhausted)?;
        self.plan_serial = token;
        self.constructor = Some((token, value));
        Ok(token)
    }

    /// Read the exact private working text and model belonging to this connection-scoped token.
    ///
    /// # Errors
    /// Rejects a closed session or an obsolete/foreign constructor token.
    pub fn constructor(
        &self,
        token: u64,
    ) -> Result<&query::constructor::Constructor, QuerySessionError> {
        self.not_closed()?;
        self.constructor
            .as_ref()
            .filter(|(id, _)| *id == token)
            .map(|(_, value)| value)
            .ok_or(QuerySessionError::UnknownConstructor)
    }

    /// Apply a visual action privately while ordinary checkpoint writes and source application remain fenced.
    ///
    /// # Errors
    /// Rejects stale model revisions, read-only drafts and unsupported or unsafe actions.
    pub fn constructor_action(
        &mut self,
        token: u64,
        expected: u64,
        action: &query::constructor::Action,
    ) -> Result<EditPlan, QuerySessionError> {
        self.constructor(token)?;
        if !self.document.writable || self.restrictions.user_read_only {
            return Err(QuerySessionError::UserReadOnly);
        }
        self.constructor
            .as_mut()
            .filter(|(id, _)| *id == token)
            .ok_or(QuerySessionError::UnknownConstructor)?
            .1
            .apply(expected, action)
            .map_err(QuerySessionError::Constructor)
    }

    /// Stage the exact draft edit before the IDE's undoable text transaction, without publishing a checkpoint.
    ///
    /// # Errors
    /// Rejects stale model/native snapshots and forbidden changes while retaining the exclusive draft lock.
    pub fn plan_constructor(
        &self,
        token: u64,
        expected: u64,
        native_snapshot: &str,
    ) -> Result<EditPlan, QuerySessionError> {
        let value = self.constructor(token)?;
        if value.revision() != expected {
            return Err(QuerySessionError::Constructor(
                query::constructor::Error::StaleRevision,
            ));
        }
        if snapshot(&self.draft) != native_snapshot {
            return Err(QuerySessionError::StaleDraft);
        }
        let plan = value.finish().map_err(QuerySessionError::Constructor)?;
        if !plan.is_empty() && !self.constructor_writable() {
            return Err(QuerySessionError::UserReadOnly);
        }
        Ok(plan)
    }

    /// Finish changes the canonical draft only after explicit acceptance; cancellation leaves its bytes and revision intact.
    ///
    /// # Errors
    /// Rejects stale visual/native snapshots and read-only changes before releasing the caller's lock.
    pub fn finish_constructor(
        &mut self,
        token: u64,
        expected: u64,
        native_snapshot: &str,
        commit: bool,
    ) -> Result<Option<EditPlan>, QuerySessionError> {
        let value = self.constructor(token)?;
        if value.revision() != expected {
            return Err(QuerySessionError::Constructor(
                query::constructor::Error::StaleRevision,
            ));
        }
        if snapshot(&self.draft) != native_snapshot {
            return Err(QuerySessionError::StaleDraft);
        }
        let plan = if commit {
            Some(value.finish().map_err(QuerySessionError::Constructor)?)
        } else {
            None
        };
        let changed = plan.as_ref().is_some_and(|plan| !plan.is_empty());
        if changed && (!self.document.writable || self.restrictions.user_read_only) {
            return Err(QuerySessionError::UserReadOnly);
        }
        let revision = if changed {
            self.revision
                .checked_add(1)
                .ok_or(QuerySessionError::RevisionExhausted)?
        } else {
            self.revision
        };
        if changed && let Some(plan) = &plan {
            self.draft = Arc::from(plan.output());
        }
        self.revision = revision;
        self.constructor = None;
        Ok(plan)
    }

    /// Explicit cancellation releases only this constructor, even when its last action reply was interrupted.
    ///
    /// # Errors
    /// Rejects an obsolete token without changing a newer constructor's ownership.
    pub fn cancel_constructor(&mut self, token: u64) -> Result<(), QuerySessionError> {
        self.constructor(token)?;
        self.constructor = None;
        Ok(())
    }

    /// Read-only model presentation follows the captured source and user restrictions, independently of its draft lock.
    #[must_use]
    pub const fn constructor_writable(&self) -> bool {
        self.constructor.is_some() && self.document.writable && !self.restrictions.user_read_only
    }

    /// Source-comment decisions use the original codec anchors and the exact acknowledged canonical draft.
    ///
    /// # Errors
    /// Rejects stale drafts, closed sessions and unsupported source structure; no source plan is issued.
    pub fn source_comment_state(
        &self,
        expected: u64,
    ) -> Result<(query::Query, bool), QuerySessionError> {
        self.check_revision(expected)?;
        let source = super::projected_text(&self.document.input, &self.identity)?;
        let needs_decision = match query::plan(
            &self.document.input,
            &self.document.snapshot,
            &self.identity,
            &self.draft,
            &[],
            None,
        ) {
            Ok(_) => false,
            Err(query::QueryError::CommentsRequireDecision) => true,
            Err(error) => return Err(error.into()),
        };
        Ok((source, needs_decision))
    }

    /// Presentation can mark the private draft read-only without using any native editor override.
    #[must_use]
    pub const fn constructor_active(&self) -> bool {
        self.constructor.is_some()
    }

    /// Source application, resumption and competing draft writers share this core guard.
    pub(super) const fn check_constructor_idle(&self) -> Result<(), QuerySessionError> {
        if self.constructor.is_some() {
            Err(QuerySessionError::ConstructorBusy)
        } else {
            Ok(())
        }
    }
}
