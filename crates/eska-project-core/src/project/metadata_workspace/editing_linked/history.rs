//! One undo step owns all related byte patches and the matching dependency graph revisions.

use super::{NumberingContext, PropertyEditPlan};
use crate::project::metadata_edit::SavedEdit;
use crate::project::{
    metadata_edit::{EditError, PropertyFileEdit},
    metadata_model::ObjectId,
    metadata_rename::transaction::Guard,
    metadata_workspace::{ProjectSession, PropertyEditError, editing_history::HistoryStep},
};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(in crate::project::metadata_workspace) struct SavedLinkedEdit {
    files: Vec<(ObjectId, PathBuf, SavedEdit)>,
    before: String,
    after: String,
}

impl SavedLinkedEdit {
    /// Keep minimal replacement text, including a primary no-op which still owns the undo action.
    pub fn new(plan: &PropertyEditPlan) -> Result<Self, EditError> {
        Ok(Self {
            files: plan
                .files()
                .map(|file| {
                    (
                        file.object_id.clone(),
                        file.path.clone(),
                        file.plan.history(),
                    )
                })
                .collect(),
            before: plan
                .context_before
                .clone()
                .ok_or(EditError::UnsupportedValue)?,
            after: plan
                .context_after
                .clone()
                .ok_or(EditError::UnsupportedValue)?,
        })
    }

    /// Include retained paths and identities in the shared session history budget.
    pub fn bytes(&self) -> usize {
        self.before.len()
            + self.after.len()
            + self
                .files
                .iter()
                .map(|(id, path, edit)| id.as_str().len() + path.as_os_str().len() + edit.bytes())
                .sum::<usize>()
    }
}

impl ProjectSession {
    /// Replaying a linked edit requires the same dependency graph as the original publication.
    pub(in crate::project::metadata_workspace) fn replay_linked_property(
        &mut self,
        guard: &Guard,
        id: &ObjectId,
        expected: Option<&str>,
        step: HistoryStep,
        undo: bool,
    ) -> Result<super::super::PropertyReplay, PropertyEditError> {
        let HistoryStep::Linked(saved) = &step else {
            return Err(EditError::HistoryUnavailable.into());
        };
        let kind = self.object(id)?.kind;
        let context = NumberingContext::read(&self.source, id, kind)?;
        let (before, after) = if undo {
            (&saved.after, &saved.before)
        } else {
            (&saved.before, &saved.after)
        };
        if expected != Some(before) || context.snapshot() != *before {
            return Err(EditError::Conflict.into());
        }
        let mut files = Vec::new();
        for (object_id, path, edit) in &saved.files {
            let (location, input) = self.editing_source(object_id)?;
            if location.path != *path {
                return Err(EditError::Conflict.into());
            }
            files.push(PropertyFileEdit {
                object_id: object_id.clone(),
                path: path.clone(),
                plan: edit.replay(&input, undo)?,
            });
        }
        let mut files = files.into_iter();
        let primary = files.next().ok_or(EditError::HistoryUnavailable)?;
        if primary.object_id != *id {
            return Err(EditError::Conflict.into());
        }
        let plan = PropertyEditPlan {
            primary,
            related: files.collect(),
            context_before: Some(before.clone()),
            context_after: Some(after.clone()),
        };
        let result = self.publish_linked_property(guard, &plan, kind);
        if result.is_ok() || matches!(result, Err(PropertyEditError::Committed(_))) {
            self.advance_history(id, step, undo);
        }
        result.map(|refresh| super::super::PropertyReplay {
            object_id: id.clone(),
            refresh,
        })
    }
}
