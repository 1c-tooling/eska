//! Token-preserving visual query operations are domain logic; IDEs display models and apply checked plans.

mod editing;
mod lex;
mod model;
mod visual;

use std::sync::Arc;

use crate::project::metadata_edit::{EditPlan, snapshot};

pub use model::{Clause, ClauseKind, Item, Model, Statement, Unavailable};
pub use visual::{Aggregate, Comparison, Expression, SourceBinding};

/// A selection is valid only with the exact constructor revision that supplied its indices.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub statement: usize,
    pub clause: usize,
    pub item: usize,
}

/// Deleting a commented node requires an explicit decision; retaining comments moves them to the next boundary.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CommentDecision {
    Keep,
    Discard,
}

/// New JOIN syntax is generated only for this limited, explicit set of operations.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JoinKind {
    #[default]
    Inner,
    Left,
    Right,
    Full,
}

/// Adapters transmit intentions and expressions; no adapter parses clauses or generates SDBL syntax.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Action {
    AppendFields {
        statement: usize,
        fields: Vec<String>,
    },
    Compose {
        statement: usize,
        selection: Option<Selection>,
        expression: Expression,
    },
    Append {
        statement: usize,
        clause: ClauseKind,
        text: String,
        #[serde(default)]
        join: JoinKind,
    },
    Replace {
        selection: Selection,
        text: String,
    },
    Remove {
        selection: Selection,
        comments: Option<CommentDecision>,
    },
    Move {
        selection: Selection,
        to: usize,
    },
}

/// Constructor failures are stable domain categories, with no localized text or native editor types.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Error {
    TooLarge,
    InvalidSyntax,
    Unsupported,
    InvalidSelection,
    InvalidExpression,
    RequiredItem,
    SourceRequired,
    UnsafeMove,
    CommentsRequireDecision,
    StaleRevision,
}

/// The original draft stays immutable until explicit finish; cancellation needs no reverse transformation.
#[derive(Clone, Debug)]
pub struct Constructor {
    original: Arc<str>,
    working: Arc<str>,
    model: Arc<Model>,
    revision: u64,
}

impl Constructor {
    /// Every supported query can open as an editable limited model or an opaque, lossless view.
    ///
    /// # Errors
    /// Rejects oversized text or excessive lexer/model resources.
    pub fn new(input: Arc<str>) -> Result<Self, Error> {
        if input.len() > super::MAX_SNAPSHOT_BYTES {
            return Err(Error::TooLarge);
        }
        let model = Arc::new(Model::parse(&input)?);
        Ok(Self {
            original: Arc::clone(&input),
            working: input,
            model,
            revision: 1,
        })
    }

    /// The current visual model contains only source ranges and stable machine identifiers.
    #[must_use]
    pub fn model(&self) -> &Model {
        &self.model
    }

    /// Read-only previews use these exact bytes, including all opaque statements and comments.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.working
    }

    /// Adapter responses cannot reuse indices after another accepted visual operation.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// A stable hash binds streamed preview bodies and final native text plans.
    #[must_use]
    pub fn snapshot(&self) -> String {
        snapshot(&self.working)
    }

    /// The session already charges the original draft; charge only additional working text and model allocations.
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.model.retained_bytes()
            + if Arc::ptr_eq(&self.original, &self.working) {
                0
            } else {
                self.working.len()
            }
    }

    /// One checked operation returns its minimal text plan and advances only the constructor's private working text.
    ///
    /// # Errors
    /// Rejects stale indices, unsafe/opaque operations, implicit comment deletion and malformed fragments.
    pub fn apply(&mut self, expected: u64, action: &Action) -> Result<EditPlan, Error> {
        if expected != self.revision {
            return Err(Error::StaleRevision);
        }
        if let Action::AppendFields { statement, fields } = action {
            if fields.len() > 512 {
                return Err(Error::TooLarge);
            }
            let mut staged = self.clone();
            for expression in fields {
                let owner = staged
                    .model
                    .statements
                    .get(*statement)
                    .ok_or(Error::InvalidSelection)?;
                let present = owner.clauses.iter().filter(|clause| clause.kind == ClauseKind::Fields)
                    .flat_map(|clause| clause.items.iter().filter_map(|item| item.visual(&staged.working, clause)))
                    .any(|field| matches!(field, Expression::Field { expression: existing, aggregate: None, .. } if existing.to_lowercase() == expression.to_lowercase()));
                if !present {
                    staged.apply(
                        staged.revision,
                        &Action::Compose {
                            statement: *statement,
                            selection: None,
                            expression: Expression::Field {
                                expression: expression.clone(),
                                alias: String::new(),
                                aggregate: None,
                            },
                        },
                    )?;
                }
            }
            let plan = super::minimal_text_plan(&self.working, &staged.working)
                .map_err(|_| Error::InvalidExpression)?;
            *self = staged;
            return Ok(plan);
        }
        let original_action = action;
        let composed;
        let action = if let Action::Compose {
            statement,
            selection,
            expression,
        } = action
        {
            composed = visual::compose(
                &self.working,
                &self.model,
                *statement,
                *selection,
                expression,
            )?;
            &composed
        } else {
            action
        };
        let plan = editing::plan(&self.working, &self.model, action)?;
        let plan = visual::augment(&self.working, &self.model, original_action, plan)?;
        if plan.output().len() > super::MAX_SNAPSHOT_BYTES {
            return Err(Error::TooLarge);
        }
        if !plan.is_empty() {
            let model = Model::parse(plan.output())?;
            check_shape(&self.model, &model, action)?;
            self.revision = self.revision.checked_add(1).ok_or(Error::TooLarge)?;
            self.working = Arc::from(plan.output());
            self.model = Arc::new(model);
        }
        Ok(plan)
    }

    /// Final application changes the same draft through one minimal plan; a no-op is byte-exact.
    ///
    /// # Errors
    /// Rejects an invalid internal text boundary without granting any source-write authority.
    pub fn finish(&self) -> Result<EditPlan, Error> {
        super::minimal_text_plan(&self.original, &self.working)
            .map_err(|_| Error::InvalidExpression)
    }
}

/// Token edits cannot accidentally merge or hide a neighboring clause while still forming a superficially valid SELECT.
fn check_shape(before: &Model, after: &Model, action: &Action) -> Result<(), Error> {
    let statement = match action {
        Action::Append { statement, .. }
        | Action::Compose { statement, .. }
        | Action::AppendFields { statement, .. } => *statement,
        Action::Replace { selection, .. }
        | Action::Remove { selection, .. }
        | Action::Move { selection, .. } => selection.statement,
    };
    let old = before
        .statements
        .get(statement)
        .ok_or(Error::InvalidSelection)?;
    let new = after
        .statements
        .get(statement)
        .ok_or(Error::InvalidExpression)?;
    if before.statements.len() != after.statements.len() || new.unavailable.is_some() {
        return Err(Error::InvalidExpression);
    }
    let mut expected: Vec<_> = old.clauses.iter().map(|clause| clause.kind).collect();
    match action {
        Action::Append { clause, .. }
            if *clause == ClauseKind::Joins || !expected.contains(clause) =>
        {
            let position = expected
                .iter()
                .position(|kind| kind.rank() > clause.rank())
                .unwrap_or(expected.len());
            expected.insert(position, *clause);
        }
        Action::Remove { selection, .. } if old.clauses[selection.clause].items.len() == 1 => {
            expected.remove(selection.clause);
        }
        _ => (),
    }
    if new.clauses.iter().map(|clause| clause.kind).eq(expected) {
        Ok(())
    } else {
        Err(Error::InvalidExpression)
    }
}

#[cfg(test)]
mod tests;
