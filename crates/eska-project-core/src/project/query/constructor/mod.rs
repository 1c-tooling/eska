//! Token-preserving visual query operations are domain logic; IDEs display models and apply checked plans.

mod dcs;
mod editing;
mod items;
pub(super) mod lex;
mod model;
mod nested;
mod packages;
mod predicates;
mod scopes;
mod totals;
mod union_matrix;
mod unions;
mod visual;

use std::sync::Arc;

use crate::project::metadata_edit::{EditPlan, snapshot};

pub use dcs::{DcsKind, DcsModel, DcsNode, DcsStatement, DcsValue};
pub use model::{Clause, ClauseKind, Item, Model, Statement, Unavailable};
pub use packages::{PackageIssue, PackageIssueKind, PackageOperation, TemporaryTable};
pub use predicates::{JoinContext, Predicate};
pub use scopes::{QueryScope, Union};
pub use totals::{Hierarchy, PeriodRange, PeriodUnit, TotalSection};
pub use union_matrix::{UnionColumn, UnionMatrix};
pub use visual::{Aggregate, Comparison, Expression, SourceBinding};

/// One set is bounded by the same maximum used for adding metadata fields.
pub const MAX_ITEM_SET_ITEMS: usize = 512;

/// Destinations are gaps in the immutable clause: zero precedes the first item, length follows the last.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ItemSetOperation {
    Remove,
    Move { to: usize },
    Copy { to: usize },
}

/// A selection is valid only with the exact constructor revision that supplied its indices.
#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
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
    CopyStatement {
        statement: usize,
    },
    AlignUnion {
        statement: usize,
    },
    SetJoinKind {
        selection: Selection,
        join: JoinKind,
    },
    MovePredicate {
        selection: Selection,
        target: ClauseKind,
    },
    SetPredicate {
        statement: usize,
        clause: ClauseKind,
        selection: Option<Selection>,
        predicate: Predicate,
    },
    EditItems {
        statement: usize,
        clause: usize,
        items: Vec<usize>,
        operation: ItemSetOperation,
        comments: Option<CommentDecision>,
    },
    AddSourceFields {
        statement: usize,
        source: Expression,
        fields: Vec<String>,
    },
    SetDcs {
        statement: usize,
        node: Option<usize>,
        target: DcsKind,
        values: Vec<DcsValue>,
        enabled: bool,
        comments: Option<CommentDecision>,
    },
    AddTotals {
        statement: usize,
        after: Option<usize>,
    },
    RemoveTotals {
        statement: usize,
        section: usize,
        comments: Option<CommentDecision>,
    },
    MoveTotals {
        statement: usize,
        section: usize,
        to: usize,
    },
    ComposeTotal {
        statement: usize,
        section: usize,
        selection: Option<Selection>,
        expression: Expression,
    },
    AppendTotal {
        statement: usize,
        section: usize,
        clause: ClauseKind,
        text: String,
    },
    SetInto {
        statement: usize,
        name: Option<String>,
        comments: Option<CommentDecision>,
    },
    AddStatement {
        statement: usize,
        operation: PackageOperation,
        name: Option<String>,
    },
    RemoveStatement {
        statement: usize,
        comments: Option<CommentDecision>,
    },
    MoveStatement {
        statement: usize,
        to: usize,
    },
    AddUnion {
        statement: usize,
        all: bool,
    },
    SetUnion {
        statement: usize,
        all: bool,
    },
    RemoveUnion {
        statement: usize,
        comments: Option<CommentDecision>,
    },
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

impl Action {
    /// Every intention addresses one branch of the revision that supplied its indices.
    #[must_use]
    pub const fn statement(&self) -> usize {
        match self {
            Self::CopyStatement { statement }
            | Self::AlignUnion { statement }
            | Self::SetPredicate { statement, .. }
            | Self::EditItems { statement, .. }
            | Self::AddSourceFields { statement, .. }
            | Self::SetDcs { statement, .. }
            | Self::AddTotals { statement, .. }
            | Self::RemoveTotals { statement, .. }
            | Self::MoveTotals { statement, .. }
            | Self::ComposeTotal { statement, .. }
            | Self::AppendTotal { statement, .. }
            | Self::SetInto { statement, .. }
            | Self::AddStatement { statement, .. }
            | Self::RemoveStatement { statement, .. }
            | Self::MoveStatement { statement, .. }
            | Self::AddUnion { statement, .. }
            | Self::SetUnion { statement, .. }
            | Self::RemoveUnion { statement, .. }
            | Self::AppendFields { statement, .. }
            | Self::Compose { statement, .. }
            | Self::Append { statement, .. } => *statement,
            Self::SetJoinKind { selection, .. }
            | Self::MovePredicate { selection, .. }
            | Self::Replace { selection, .. }
            | Self::Remove { selection, .. }
            | Self::Move { selection, .. } => selection.statement,
        }
    }
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
    focus: Option<usize>,
    total_focus: Option<usize>,
    item_focus: Vec<Selection>,
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
        let focus = (!model.statements.is_empty()).then_some(0);
        Ok(Self {
            original: Arc::clone(&input),
            working: input,
            model,
            revision: 1,
            focus,
            total_focus: None,
            item_focus: Vec::new(),
        })
    }

    /// The current visual model contains only source ranges and stable machine identifiers.
    #[must_use]
    pub fn model(&self) -> &Model {
        &self.model
    }

    /// DCS is a read-only projection of the same canonical bytes; unknown blocks never grant write authority.
    ///
    /// # Errors
    /// Rejects resource exhaustion while scanning the bounded query.
    pub fn dcs_model(&self) -> Result<DcsModel, Error> {
        dcs::inspect(&self.working, &self.model)
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

    /// Structural operations return the surviving statement in the same scope, even when flat indices change.
    #[must_use]
    pub const fn focus(&self) -> Option<usize> {
        self.focus
    }

    /// A section intention selects its survivor without requiring an IDE to infer shifted clause indices.
    #[must_use]
    pub const fn total_focus(&self) -> Option<usize> {
        self.total_focus
    }

    /// A grouped action returns its resulting selections without client-side index remapping.
    #[must_use]
    pub fn item_focus(&self) -> &[Selection] {
        &self.item_focus
    }

    /// A negotiated old client cannot edit newly recognized boolean structures.
    #[must_use]
    pub fn requires_predicates(&self, statement: usize) -> bool {
        self.model
            .statements
            .get(statement)
            .is_some_and(|owner| predicates::complex(self.text(), owner))
    }

    /// Breadcrumbs use core scope edges; adapters never reconstruct nested SQL ownership.
    #[must_use]
    pub fn statement_path(&self, statement: usize) -> Vec<usize> {
        let mut path = Vec::new();
        let mut current = Some(statement);
        while let Some(index) = current {
            let Some(owner) = self.model.statements.get(index) else {
                return Vec::new();
            };
            path.push(index);
            current = self.model.scopes[owner.scope].parent;
        }
        path.reverse();
        path
    }

    /// Copy controls reflect recognized syntax and never infer permission from a field name.
    #[must_use]
    pub fn copy_unavailable(&self, selection: Selection) -> Option<Error> {
        items::copy_unavailable(self, selection)
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
            + self.item_focus.capacity() * std::mem::size_of::<Selection>()
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
        if let Some(plan) = predicates::plan(self, action)? {
            return self.accept_predicate(plan, action);
        }
        if let Some(staged) = items::stage(self, action)? {
            let plan = super::minimal_text_plan(&self.working, staged.value.text())
                .map_err(|_| Error::InvalidExpression)?;
            let model = Model::parse(plan.output())?;
            self.accept(&plan, model, action)?;
            self.focus = Some(action.statement());
            self.item_focus = staged.focus;
            return Ok(plan);
        }
        if let Some(plan) = dcs::plan(&self.working, &self.model, action)? {
            let model = Model::parse(plan.output())?;
            self.accept(&plan, model, action)?;
            return Ok(plan);
        }
        if let Some(plan) = totals::section_plan(&self.working, &self.model, action)? {
            let model = Model::parse(plan.output())?;
            totals::check_sections(&self.model, &model, action)?;
            self.accept(&plan, model, action)?;
            return Ok(plan);
        }
        if let Some(plan) = packages::plan(&self.working, &self.model, action)? {
            let model = Model::parse(plan.output())?;
            packages::check(&self.working, &self.model, plan.output(), &model)?;
            self.accept(&plan, model, action)?;
            return Ok(plan);
        }
        if let Some(plan) = unions::plan(&self.working, &self.model, action)? {
            let model = Model::parse(plan.output())?;
            unions::check_shape(&self.model, &model, action)?;
            self.accept(&plan, model, action)?;
            return Ok(plan);
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
        if let Some(actions) = unions::field_actions(&self.working, &self.model, action)? {
            let mut replacements = Vec::new();
            for intention in &actions {
                replacements.extend(
                    self.plan_action(intention)?
                        .0
                        .replacements()
                        .iter()
                        .cloned(),
                );
            }
            replacements.sort_by_key(|edit| edit.range.start);
            let plan = EditPlan::from_replacements(&self.working, replacements)
                .map_err(|_| Error::InvalidExpression)?;
            let model = Model::parse(plan.output())?;
            check_shape(&self.model, &model, action)?;
            self.accept(&plan, model, action)?;
            return Ok(plan);
        }
        let (plan, lowered) = self.plan_action(action)?;
        let model = Model::parse(plan.output())?;
        check_shape(&self.model, &model, &lowered)?;
        self.accept(&plan, model, action)?;
        Ok(plan)
    }

    /// Lower one intention without mutating text, so independent UNION-column edits can be published atomically.
    fn plan_action(&self, action: &Action) -> Result<(EditPlan, Action), Error> {
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
        let plan = if let Some(plan) = totals::plan(&self.working, &self.model, original_action)? {
            plan
        } else if let Some(plan) = nested::plan(&self.working, &self.model, original_action)? {
            plan
        } else {
            editing::plan(&self.working, &self.model, action)?
        };
        let plan = visual::augment(&self.working, &self.model, original_action, plan)?;
        let plan = nested::output_plan(&self.working, &self.model, original_action, plan)?;
        Ok((plan, action.clone()))
    }

    /// A structural predicate cannot turn its selected operator into an opaque or missing statement.
    fn accept_predicate(&mut self, plan: EditPlan, action: &Action) -> Result<EditPlan, Error> {
        let model = Model::parse(plan.output())?;
        if model
            .statements
            .get(action.statement())
            .is_none_or(|owner| owner.unavailable.is_some())
        {
            return Err(Error::Unsupported);
        }
        self.accept(&plan, model, action)?;
        Ok(plan)
    }

    /// All multi-part operations share one checked publication and one revision increment.
    fn accept(&mut self, plan: &EditPlan, model: Model, action: &Action) -> Result<(), Error> {
        if plan.output().len() > super::MAX_SNAPSHOT_BYTES {
            return Err(Error::TooLarge);
        }
        packages::check(&self.working, &self.model, plan.output(), &model)?;
        let focus = packages::focus(&self.model, &model, action)
            .or_else(|| scopes::focus(&self.model, &model, plan, action));
        let total_focus = totals::section_focus(&self.model, &model, action);
        if !plan.is_empty() {
            self.revision = self.revision.checked_add(1).ok_or(Error::TooLarge)?;
            self.working = Arc::from(plan.output());
            self.model = Arc::new(model);
        }
        self.focus = focus;
        self.total_focus = total_focus;
        self.item_focus.clear();
        Ok(())
    }

    /// Final application changes the same draft through one minimal plan; a no-op is byte-exact.
    ///
    /// # Errors
    /// Rejects an invalid internal text boundary without granting any source-write authority.
    pub fn finish(&self) -> Result<EditPlan, Error> {
        if self.original != self.working && !self.model.union_mismatches(&self.working).is_empty() {
            return Err(Error::InvalidExpression);
        }
        packages::check(
            &self.original,
            &Model::parse(&self.original)?,
            &self.working,
            &self.model,
        )?;
        super::minimal_text_plan(&self.original, &self.working)
            .map_err(|_| Error::InvalidExpression)
    }
}

/// Token edits cannot accidentally merge or hide a neighboring clause while still forming a superficially valid SELECT.
fn check_shape(before: &Model, after: &Model, action: &Action) -> Result<(), Error> {
    let statement = action.statement();
    let old = before
        .statements
        .get(statement)
        .ok_or(Error::InvalidSelection)?;
    let new = after
        .statements
        .get(statement)
        .ok_or(Error::InvalidExpression)?;
    if before
        .scopes
        .iter()
        .filter(|scope| scope.parent.is_none())
        .count()
        != after
            .scopes
            .iter()
            .filter(|scope| scope.parent.is_none())
            .count()
        || old.scope != new.scope
        || old.branch != new.branch
        || new.unavailable.is_some()
    {
        return Err(Error::InvalidExpression);
    }
    let mut expected: Vec<_> = old.clauses.iter().map(|clause| clause.kind).collect();
    match action {
        Action::Append {
            clause: ClauseKind::Totals | ClauseKind::TotalGroups,
            ..
        } if !expected.contains(&ClauseKind::Totals) => {
            expected.extend([ClauseKind::Totals, ClauseKind::TotalGroups]);
        }
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
            let kind = old.clauses[selection.clause].kind;
            if kind == ClauseKind::TotalGroups {
                let fields = selection
                    .clause
                    .checked_sub(1)
                    .ok_or(Error::InvalidExpression)?;
                expected.drain(fields..=selection.clause);
            } else if kind != ClauseKind::Totals {
                expected.remove(selection.clause);
            }
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
