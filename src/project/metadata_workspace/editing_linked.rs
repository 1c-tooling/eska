//! Numbering edits bind their dependent descriptors to a separate, freshly checked revision.

mod context;
mod history;

use std::path::PathBuf;

use super::{
    ProjectSession, PropertyEditError, RefreshReport, WorkspaceError, editing_history::HistoryStep,
};
use crate::project::{
    metadata_edit::{
        EditError, EditPlan, PropertyFileEdit, Replacement,
        numbering::{self, Numbering},
    },
    metadata_model::{MetadataKind, ObjectId},
    metadata_rename::transaction::Guard,
};

use context::NumberingContext;
pub(super) use history::SavedLinkedEdit;

/// The primary property and its related changes share one validation and publication boundary.
#[derive(Debug)]
pub struct PropertyEditPlan {
    pub(crate) primary: PropertyFileEdit,
    pub(crate) related: Vec<PropertyFileEdit>,
    pub(crate) context_before: Option<String>,
    pub(crate) context_after: Option<String>,
}

impl PropertyEditPlan {
    /// Keep the primary descriptor preview available alongside the complete related-file list.
    #[must_use]
    pub fn output(&self) -> &str {
        self.primary.plan.output()
    }

    /// Existing scalar consumers retain their primary-file replacement coordinates.
    #[must_use]
    pub fn replacements(&self) -> &[Replacement] {
        self.primary.plan.replacements()
    }

    /// A successful no-op never writes a descriptor or creates a history entry.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files().all(|file| file.plan.is_empty())
    }

    /// AI clients can inspect every backend-computed source edit before publication.
    pub fn files(&self) -> impl Iterator<Item = &PropertyFileEdit> {
        std::iter::once(&self.primary).chain(&self.related)
    }
}

impl ProjectSession {
    /// Publish a dependency token only for the two owner kinds with shared numbering parameters.
    pub(super) fn property_context(
        &self,
        id: &ObjectId,
        kind: MetadataKind,
    ) -> Result<Option<(String, usize)>, PropertyEditError> {
        if !matches!(
            kind,
            MetadataKind::Document | MetadataKind::DocumentNumerator
        ) {
            return Ok(None);
        }
        let context = NumberingContext::read(&self.source, id, kind)?;
        let count = if kind == MetadataKind::DocumentNumerator {
            context.files.len()
        } else {
            0
        };
        Ok(Some((context.snapshot(), count)))
    }

    /// Follow only declared numerators and documents; never accept source paths or dependent XML from a client.
    pub(super) fn plan_linked_property(
        &self,
        primary: PropertyFileEdit,
        kind: MetadataKind,
        expected: Option<&str>,
        property: &str,
        modern: bool,
    ) -> Result<PropertyEditPlan, PropertyEditError> {
        let expected = expected.ok_or(EditError::ContextRequired)?;
        let context = NumberingContext::read(&self.source, &primary.object_id, kind)?;
        let before = context.snapshot();
        if before != expected {
            return Err(EditError::Conflict.into());
        }
        let mut result = PropertyEditPlan {
            primary,
            related: Vec::new(),
            context_before: Some(before),
            context_after: None,
        };
        if result.primary.plan.is_empty() {
            result.context_after.clone_from(&result.context_before);
            return Ok(result);
        }
        match kind {
            MetadataKind::Document => {
                if let Some(reference) =
                    numbering::document_numerator(result.primary.plan.output())?
                {
                    let target = context.numerator(&reference)?;
                    let numbering = Numbering::read(&target.input, modern)?;
                    result.primary.plan = numbering.synchronize(
                        result.primary.plan.original(),
                        &result.primary.object_id,
                        Some(&reference),
                        None,
                    )?;
                }
            }
            MetadataKind::DocumentNumerator => {
                let numbering = Numbering::read(result.primary.plan.output(), modern)?;
                for file in &context.files {
                    let plan = numbering
                        .synchronize(&file.input, &file.id, None, Some(property))
                        .map_err(|error| PropertyEditError::Related {
                            object_id: file.id.clone(),
                            name: file.name.clone(),
                            error,
                        })?;
                    result.related.push(PropertyFileEdit {
                        object_id: file.id.clone(),
                        path: file.path.clone(),
                        plan,
                    });
                }
            }
            _ => return Err(EditError::UnsupportedValue.into()),
        }
        result.context_after = Some(context.after(result.files()));
        Ok(result)
    }

    /// Every affected object keeps its own support boundary, including exact history replay.
    pub(super) fn validate_linked_support(
        &mut self,
        plan: &PropertyEditPlan,
    ) -> Result<(), PropertyEditError> {
        for file in plan.files().filter(|file| !file.plan.is_empty()) {
            self.reveal_declared_object(&file.object_id)?;
            if !self.can_edit(&file.object_id, &file.path)?
                || std::fs::metadata(self.project().source().join(&file.path))
                    .map_err(EditError::Io)?
                    .permissions()
                    .readonly()
            {
                if file.object_id != plan.primary.object_id {
                    return Err(PropertyEditError::Related {
                        object_id: file.object_id.clone(),
                        name: self.object(&file.object_id)?.name.clone(),
                        error: EditError::ReadOnly,
                    });
                }
                return Err(EditError::ReadOnly.into());
            }
        }
        Ok(())
    }

    /// A staged journal is checked against both the old dependency graph and its predicted final state.
    pub(super) fn publish_linked_property(
        &mut self,
        guard: &Guard,
        plan: &PropertyEditPlan,
        kind: MetadataKind,
    ) -> Result<RefreshReport, PropertyEditError> {
        self.validate_linked_support(plan)?;
        if self.generation == u64::MAX {
            return Err(WorkspaceError::GenerationExhausted.into());
        }
        if plan.is_empty() {
            return Ok(RefreshReport {
                generation: self.generation,
                affected: Vec::new(),
            });
        }
        let id = &plan.primary.object_id;
        let verify = |expected: Option<&str>| -> Result<(), EditError> {
            // An unreadable dependency cannot establish the required revision and must stop publication.
            let current = NumberingContext::read(&self.source, id, kind)
                .map_err(|_| EditError::Conflict)?
                .snapshot();
            (Some(current.as_str()) == expected)
                .then_some(())
                .ok_or(EditError::Conflict)
        };
        let files: Vec<_> = plan.files().collect();
        guard.publish_properties(
            self.project().source(),
            &files,
            || verify(plan.context_before.as_deref()),
            || verify(plan.context_after.as_deref()),
        )?;
        let paths: Vec<PathBuf> = files
            .iter()
            .filter(|file| !file.plan.is_empty())
            .map(|file| file.path.clone())
            .collect();
        self.changed_paths(&paths)
            .map_err(|error| PropertyEditError::Committed(Box::new(error)))
    }

    /// A scalar plan retains its existing representation and acquires no unrelated dependencies.
    pub(super) fn scalar_property_plan(
        id: &ObjectId,
        path: PathBuf,
        plan: EditPlan,
    ) -> PropertyEditPlan {
        PropertyEditPlan {
            primary: PropertyFileEdit {
                object_id: id.clone(),
                path,
                plan,
            },
            related: Vec::new(),
            context_before: None,
            context_after: None,
        }
    }

    /// Failed refresh after publication still records the exact operation once.
    pub(super) fn save_linked_property(
        &mut self,
        guard: &Guard,
        plan: &PropertyEditPlan,
        kind: MetadataKind,
    ) -> Result<RefreshReport, PropertyEditError> {
        let history = SavedLinkedEdit::new(plan)?;
        let result = self.publish_linked_property(guard, plan, kind);
        if !plan.is_empty()
            && (result.is_ok() || matches!(result, Err(PropertyEditError::Committed(_))))
        {
            self.record_property(
                &plan.primary.object_id,
                HistoryStep::Linked(Box::new(history)),
            );
        }
        result
    }
}
