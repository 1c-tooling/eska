//! Scalar edits and structural renames share one bounded sequence per logical object.

use super::{ProjectSession, RefreshReport};
use crate::project::{
    metadata_edit::SavedEdit,
    metadata_model::ObjectId,
    metadata_rename::{RenamePlan, history::SavedRename},
};

/// Undo can change logical identity; callers must replace their old object address.
#[derive(Debug)]
pub struct PropertyReplay {
    pub object_id: ObjectId,
    pub refresh: RefreshReport,
}

#[derive(Clone, Debug)]
pub(super) enum HistoryStep {
    Scalar(SavedEdit),
    Rename(Box<SavedRename>),
}

impl HistoryStep {
    /// Bound retained replacement text and structural paths independently of descriptor size.
    fn bytes(&self) -> usize {
        match self {
            Self::Scalar(step) => step.bytes(),
            Self::Rename(step) => step.bytes(),
        }
    }
}

/// Histories are session-local and migrate with renamed objects and their descendants.
#[derive(Debug, Default)]
pub(super) struct PropertyHistory {
    pub undo: Vec<HistoryStep>,
    pub redo: Vec<HistoryStep>,
}

impl ProjectSession {
    /// Keep one ordered sequence, dropping old operations once the existing memory limits are reached.
    pub(super) fn record_property(&mut self, id: &ObjectId, step: HistoryStep) {
        let entry = self.property_history.entry(id.clone()).or_default();
        entry.redo.clear();
        entry.undo.push(step);
        while entry.undo.len() > 100
            || entry.undo.iter().map(HistoryStep::bytes).sum::<usize>() > 8 * 1024 * 1024
        {
            entry.undo.remove(0);
        }
        while self.property_history.len() > 256
            || self
                .property_history
                .values()
                .flat_map(|history| history.undo.iter().chain(&history.redo))
                .map(HistoryStep::bytes)
                .sum::<usize>()
                > 32 * 1024 * 1024
        {
            self.property_history.pop_first();
        }
    }

    /// Preserve unrelated histories; descendant keys use a segment boundary, never a textual prefix.
    pub(super) fn rename_histories(&mut self, plan: &RenamePlan) {
        self.property_history = std::mem::take(&mut self.property_history)
            .into_iter()
            .map(|(id, history)| (id.renamed(&plan.object_id, &plan.new_object_id), history))
            .collect();
    }

    /// Move a successful replay between stacks; failed writes leave its original position intact.
    pub(super) fn advance_history(&mut self, id: &ObjectId, step: HistoryStep, undo: bool) {
        if let Some(history) = self.property_history.get_mut(id) {
            let (from, to) = if undo {
                (&mut history.undo, &mut history.redo)
            } else {
                (&mut history.redo, &mut history.undo)
            };
            from.pop();
            to.push(step);
        }
    }
}
