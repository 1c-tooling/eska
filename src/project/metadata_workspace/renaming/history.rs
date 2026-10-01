//! Structural replay verifies the whole source snapshot and uses the same recovery journal.

use super::super::{
    ProjectSession, PropertyEditError, PropertyReplay, RefreshReport, WorkspaceError,
    editing_history::HistoryStep,
};
use crate::project::{
    metadata_edit::EditError,
    metadata_model::ObjectId,
    metadata_rename::{inventory, transaction::Guard},
};

impl ProjectSession {
    /// Reopen structural identities while preserving history and advancing failed-refresh generations too.
    pub(super) fn refresh_after_rename(&mut self) -> Result<(), WorkspaceError> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(WorkspaceError::GenerationExhausted)?;
        let source = self.source.reopen().map_err(WorkspaceError::Source)?;
        let mut fresh = Self::open(source)?;
        fresh.generation = self.generation;
        fresh.property_history = std::mem::take(&mut self.property_history);
        *self = fresh;
        Ok(())
    }

    /// Replay backend-owned replacements only when no later source changes can leave dangling references.
    pub(in crate::project::metadata_workspace) fn replay_rename(
        &mut self,
        guard: &Guard,
        id: &ObjectId,
        step: HistoryStep,
        undo: bool,
    ) -> Result<PropertyReplay, PropertyEditError> {
        let HistoryStep::Rename(saved) = &step else {
            return Err(EditError::HistoryUnavailable.into());
        };
        let plan = if undo { &saved.reverse } else { &saved.forward };
        if id != &plan.object_id {
            return Err(EditError::Conflict.into());
        }
        self.generation
            .checked_add(1)
            .ok_or(WorkspaceError::GenerationExhausted)?;
        let exclusions = self.rename_exclusions();
        if inventory::snapshot(
            self.project().source(),
            &exclusions,
            id.as_str(),
            &plan.new_name,
        )
        .map_err(EditError::Io)?
            != plan.snapshot
        {
            return Err(EditError::Conflict.into());
        }
        let mut current = Self::open(self.source.reopen().map_err(WorkspaceError::Source)?)?;
        let mut checked = plan.clone();
        current.rename_support(&mut checked)?;
        if !checked.issues.is_empty() {
            return Err(EditError::ReadOnly.into());
        }
        let old_root = self.source.root().id().clone();
        guard.publish(self.project().source(), plan, &exclusions)?;
        let new_id = plan.new_object_id.clone();
        self.rename_histories(plan);
        self.advance_history(&new_id, step, undo);
        self.refresh_after_rename()
            .map_err(|error| PropertyEditError::Committed(Box::new(error)))?;
        let mut affected = vec![
            old_root,
            self.source.root().id().clone(),
            id.clone(),
            new_id.clone(),
        ];
        affected.sort();
        affected.dedup();
        Ok(PropertyReplay {
            object_id: new_id,
            refresh: RefreshReport {
                generation: self.generation,
                affected,
            },
        })
    }
}
