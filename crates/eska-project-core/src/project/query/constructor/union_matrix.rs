//! Positional UNION correspondence and explicit NULL filling belong to the shared constructor.

use super::{
    Action, ClauseKind, Constructor, Error, JoinKind, Selection, editing, items::Staged, visual,
};
use std::collections::BTreeSet;

/// Empty cells are visible missing projections; viewing the matrix never inserts a value.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnionMatrix {
    pub scope: usize,
    pub branches: Vec<usize>,
    pub columns: Vec<UnionColumn>,
    pub can_align: bool,
}

/// One output position has a shared first-branch name and an optional source selection per branch.
#[derive(Clone, Debug, serde::Serialize)]
pub struct UnionColumn {
    pub index: usize,
    pub alias: Option<String>,
    pub cells: Vec<Option<Selection>>,
}

impl Constructor {
    /// Construct a positional view from this snapshot, including mismatches and opaque/wildcard restrictions.
    #[must_use]
    pub fn union_matrix(&self, statement: usize) -> Option<UnionMatrix> {
        let owner = self.model.statements.get(statement)?;
        let scope = self.model.scopes.get(owner.scope)?;
        let fields: Option<Vec<_>> = scope
            .branches
            .iter()
            .map(|&branch| {
                self.model.statements[branch]
                    .clauses
                    .iter()
                    .enumerate()
                    .find(|(_, clause)| clause.kind == ClauseKind::Fields)
            })
            .collect();
        let fields = fields?;
        let count = fields.iter().map(|(_, clause)| clause.items.len()).max()?;
        if count > super::MAX_ITEM_SET_ITEMS {
            return None;
        }
        let supported = count <= super::MAX_ITEM_SET_ITEMS
            && scope
                .branches
                .iter()
                .all(|&branch| self.model.statements[branch].unavailable.is_none())
            && fields.iter().all(|(_, clause)| {
                clause.items.iter().all(|item| {
                    item.wildcard_source(self.text(), clause).is_none()
                        && item.range.len() <= 16_384
                })
            });
        let columns = (0..count)
            .map(|index| UnionColumn {
                index,
                alias: fields[0]
                    .1
                    .items
                    .get(index)
                    .and_then(|item| item.output_name(self.text(), fields[0].1))
                    .filter(|name| name.len() <= 16_384),
                cells: fields
                    .iter()
                    .zip(&scope.branches)
                    .map(|(&(clause, fields), &statement)| {
                        fields.items.get(index).map(|_| Selection {
                            statement,
                            clause,
                            item: index,
                        })
                    })
                    .collect(),
            })
            .collect();
        Some(UnionMatrix {
            scope: owner.scope,
            branches: scope.branches.clone(),
            columns,
            can_align: supported
                && scope.branches.len() > 1
                && fields.iter().any(|(_, clause)| clause.items.len() != count),
        })
    }
}

/// Explicit alignment fills only absent cells; all original expressions and comments remain untouched.
pub(super) fn align(value: &Constructor, statement: usize) -> Result<Staged, Error> {
    let matrix = value.union_matrix(statement).ok_or(Error::Unsupported)?;
    if !matrix.can_align {
        if matrix
            .columns
            .iter()
            .all(|column| column.cells.iter().all(Option::is_some))
        {
            return Ok(Staged {
                value: value.clone(),
                focus: Vec::new(),
            });
        }
        return Err(Error::Unsupported);
    }
    let mut staged = value.clone();
    staged.revision = 1;
    let mut names: BTreeSet<_> = matrix
        .columns
        .iter()
        .filter_map(|column| column.alias.as_ref().map(|name| name.to_lowercase()))
        .collect();
    let mut aliases = Vec::new();
    for column in &matrix.columns {
        let base = column
            .alias
            .clone()
            .or_else(|| {
                column.cells.iter().flatten().find_map(|selection| {
                    let clause =
                        &value.model.statements[selection.statement].clauses[selection.clause];
                    clause.items[selection.item].output_name(value.text(), clause)
                })
            })
            .unwrap_or_else(|| format!("Column{}", column.index + 1));
        visual::identifier(&base)?;
        let mut alias = base.clone();
        let mut suffix = 1;
        while column.alias.is_none() && names.contains(&alias.to_lowercase()) {
            suffix += 1;
            alias = format!("{base}{suffix}");
        }
        names.insert(alias.to_lowercase());
        aliases.push(alias);
    }
    for &branch in &matrix.branches {
        let before = value.model.statements[branch]
            .clauses
            .iter()
            .find(|clause| clause.kind == ClauseKind::Fields)
            .ok_or(Error::Unsupported)?
            .items
            .len();
        for alias in aliases.iter().skip(before) {
            let owner = &staged.model.statements[branch];
            let text = format!("NULL {} {alias}", if owner.russian { "КАК" } else { "AS" });
            let action = Action::Append {
                statement: branch,
                clause: ClauseKind::Fields,
                text,
                join: JoinKind::Inner,
            };
            let plan = editing::plan(staged.text(), &staged.model, &action)?;
            let model = super::Model::parse(plan.output())?;
            staged.accept(&plan, model, &action)?;
        }
    }
    if staged
        .model
        .union_mismatches(staged.text())
        .contains(&matrix.scope)
    {
        return Err(Error::InvalidExpression);
    }
    Ok(Staged {
        value: staged,
        focus: Vec::new(),
    })
}
