//! Temporary tables belong to ordered root scopes, not to an IDE or a global metadata cache.

mod edits;
pub(super) use edits::{focus, plan};

use std::collections::{BTreeMap, BTreeSet};

use super::{
    ClauseKind, Error, Model, Selection,
    lex::{self, Kind},
};

/// A package operator is an explicit backend intention; the adapter supplies no statement text.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PackageOperation {
    Select,
    Destroy,
}

/// One visible table identifies the producing scope's first projection, including UNION and wildcards.
#[derive(Clone, Debug, serde::Serialize)]
pub struct TemporaryTable {
    pub name: String,
    pub scope: usize,
    pub statement: usize,
}

/// Dependency diagnostics retain machine identifiers independently of editor and query language.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageIssueKind {
    TemporaryMissing,
    TemporaryDuplicate,
    TemporaryColumn,
    TemporaryAmbiguousColumns,
}

/// A known invalid dependency stays visible, while new invalid dependencies cannot be introduced atomically.
#[derive(Clone, Debug, serde::Serialize)]
pub struct PackageIssue {
    pub code: PackageIssueKind,
    pub scope: usize,
    pub table: String,
    pub column: Option<String>,
}

impl Model {
    /// Only recognized root operators can be added, removed or reordered through the package controls.
    #[must_use]
    pub fn package_operation(&self, statement: usize) -> Option<PackageOperation> {
        let owner = self.statements.get(statement)?;
        if owner.unavailable.is_some() || owner.nested {
            return None;
        }
        Some(
            if owner
                .clauses
                .iter()
                .any(|clause| clause.kind == ClauseKind::Destroy)
            {
                PackageOperation::Destroy
            } else {
                PackageOperation::Select
            },
        )
    }

    /// INTO belongs to the first branch, while the shared indexes and result tail belong to the last.
    #[must_use]
    pub fn temporary_destination(&self, input: &str, scope: usize) -> Option<String> {
        let first = *self.scopes.get(scope)?.branches.first()?;
        if self.statements[first].unavailable.is_some() {
            return None;
        }
        let clause = self.statements[first]
            .clauses
            .iter()
            .find(|clause| clause.kind == ClauseKind::Into)?;
        Some(clause.items.first()?.expression(input))
    }

    /// Nested SELECTs see exactly the temporary tables visible before their containing package operator.
    ///
    /// # Errors
    /// Rejects an index outside this immutable model.
    pub fn temporary_tables_at(
        &self,
        input: &str,
        statement: usize,
    ) -> Result<Vec<TemporaryTable>, Error> {
        let scope = self.root_scope(statement)?;
        Ok(flow(input, self, Some(scope), &known_tables(input, self))
            .0
            .into_values()
            .collect())
    }

    /// Resolve case-insensitively through the current package lifetime, never through a later recreation.
    ///
    /// # Errors
    /// Rejects an index outside this immutable model.
    pub fn temporary_source(
        &self,
        input: &str,
        statement: usize,
        name: &str,
    ) -> Result<Option<TemporaryTable>, Error> {
        Ok(self
            .temporary_tables_at(input, statement)?
            .into_iter()
            .find(|table| table.name.to_lowercase() == name.to_lowercase()))
    }

    /// Existing invalid text remains inspectable and lossless; clients receive the exact package diagnostics.
    #[must_use]
    pub fn package_issues(&self, input: &str) -> Vec<PackageIssue> {
        flow(input, self, None, &known_tables(input, self)).1
    }

    /// Follow only explicit parent edges; a flat statement ordinal is never a package position.
    pub(super) fn root_scope(&self, statement: usize) -> Result<usize, Error> {
        let mut scope = self
            .statements
            .get(statement)
            .ok_or(Error::InvalidSelection)?
            .scope;
        while let Some(parent) = self.scopes[scope].parent {
            scope = self.statements[parent].scope;
        }
        Ok(scope)
    }
}

/// A portable table name is one complete query identifier, excluding every statement/control keyword.
pub(super) fn identifier(name: &str) -> bool {
    if name.len() > 1024 {
        return false;
    }
    let Ok(tokens) = lex::tokens(name) else {
        return false;
    };
    tokens.len() == 1
        && tokens[0].kind == Kind::Word
        && tokens[0].range == (0..name.len())
        && !tokens[0].keyword(
            name,
            &[
                "SELECT",
                "ВЫБРАТЬ",
                "INTO",
                "ПОМЕСТИТЬ",
                "FROM",
                "ИЗ",
                "WHERE",
                "ГДЕ",
                "JOIN",
                "СОЕДИНЕНИЕ",
                "ON",
                "ПО",
                "BY",
                "GROUP",
                "СГРУППИРОВАТЬ",
                "ORDER",
                "УПОРЯДОЧИТЬ",
                "TOTALS",
                "ИТОГИ",
                "DROP",
                "УНИЧТОЖИТЬ",
                "AS",
                "КАК",
                "UNION",
                "ОБЪЕДИНИТЬ",
                "INDEX",
                "ИНДЕКСИРОВАТЬ",
                "HAVING",
                "ИМЕЮЩИЕ",
                "NULL",
                "TRUE",
                "FALSE",
                "ИСТИНА",
                "ЛОЖЬ",
            ],
        )
}

/// Retain removed destinations as known names while checking that no dependent reference becomes dangling.
pub(super) fn check(before: &str, old: &Model, after: &str, new: &Model) -> Result<(), Error> {
    let mut known = known_tables(before, old);
    known.extend(known_tables(after, new));
    let counts = |input, model| {
        let mut result = BTreeMap::new();
        for issue in flow(input, model, None, &known).1 {
            *result
                .entry((
                    issue.code,
                    issue.table.to_lowercase(),
                    issue.column.map(|c| c.to_lowercase()),
                ))
                .or_insert(0usize) += 1;
        }
        result
    };
    let old = counts(before, old);
    if counts(after, new)
        .iter()
        .any(|(key, count)| *count > old.get(key).copied().unwrap_or(0))
    {
        return Err(Error::InvalidExpression);
    }
    Ok(())
}

/// Known temporary names are separate from metadata tables and external parameter sources.
fn known_tables(input: &str, model: &Model) -> BTreeSet<String> {
    model
        .scopes
        .iter()
        .enumerate()
        .filter(|(_, scope)| scope.parent.is_none())
        .filter_map(|(index, _)| model.temporary_destination(input, index))
        .map(|name| name.to_lowercase())
        .collect()
}

/// Apply package lifetimes before exposing each SELECT; a table does not see its own creation.
fn flow(
    input: &str,
    model: &Model,
    until: Option<usize>,
    known: &BTreeSet<String>,
) -> (BTreeMap<String, TemporaryTable>, Vec<PackageIssue>) {
    let mut state = FlowState::default();
    for (index, scope) in model
        .scopes
        .iter()
        .enumerate()
        .filter(|(_, scope)| scope.parent.is_none())
    {
        if until == Some(index) {
            break;
        }
        let first = scope.branches[0];
        if let Some(clause) = model.statements[first]
            .clauses
            .iter()
            .find(|clause| clause.kind == ClauseKind::Destroy)
        {
            let name = clause.items[0].expression(input);
            state.destroy(index, name);
            continue;
        }
        for (statement, owner) in model
            .statements
            .iter()
            .enumerate()
            .filter(|(s, _)| until.is_none() && model.root_scope(*s).ok() == Some(index))
        {
            for source in owner
                .source_bindings(input)
                .into_iter()
                .filter(|s| !s.derived && !s.table.contains('.'))
            {
                let key = source.table.to_lowercase();
                if let Some(table) = state.tables.get(&key) {
                    column_issues(input, model, statement, &source, table, &mut state.issues);
                } else if known.contains(&key) {
                    state.issues.push(PackageIssue {
                        code: PackageIssueKind::TemporaryMissing,
                        scope: index,
                        table: source.table,
                        column: None,
                    });
                }
            }
        }
        if let Some(name) = model.temporary_destination(input, index) {
            state.create(input, model, index, first, name);
        }
    }
    (state.tables, state.issues)
}

/// Explicit projection names support dependency checks; wildcard schemas are resolved lazily by the catalog.
fn column_issues(
    input: &str,
    model: &Model,
    statement: usize,
    source: &super::SourceBinding,
    table: &TemporaryTable,
    issues: &mut Vec<PackageIssue>,
) {
    let Some(fields) = model.statements[table.statement]
        .clauses
        .iter()
        .find(|c| c.kind == ClauseKind::Fields)
    else {
        return;
    };
    if fields
        .items
        .iter()
        .any(|item| item.wildcard_source(input, fields).is_some())
    {
        return;
    }
    let names: Vec<_> = fields
        .items
        .iter()
        .filter_map(|item| item.output_name(input, fields))
        .map(|name| name.to_lowercase())
        .collect();
    if names.len() != fields.items.len() {
        return;
    }
    let unique: BTreeSet<_> = names.iter().collect();
    if unique.len() != names.len() {
        return;
    }
    let owner = &model.statements[statement];
    let Ok(protected) = super::visual::source_prefixes(input, owner) else {
        return;
    };
    let Ok(tokens) = lex::tokens(&input[owner.range.clone()]) else {
        return;
    };
    let tokens: Vec<_> = tokens.iter().filter(|t| t.kind != Kind::Comment).collect();
    for (index, path) in tokens.windows(3).enumerate() {
        if path[0].kind != Kind::Word
            || path[1].kind != Kind::Symbol('.')
            || path[2].kind != Kind::Word
            || index
                .checked_sub(1)
                .is_some_and(|i| matches!(tokens[i].kind, Kind::Symbol('.' | '&')))
        {
            continue;
        }
        let at = owner.range.start + path[0].range.start;
        if protected.iter().any(|range| range.contains(&at))
            || model
                .source_scope(Selection {
                    statement,
                    clause: source.clause,
                    item: source.item,
                })
                .is_some()
            || model
                .scopes
                .iter()
                .any(|scope| scope.parent == Some(statement) && scope.range.contains(&at))
        {
            continue;
        }
        let text = &input[owner.range.clone()];
        let column = &text[path[2].range.clone()];
        if text[path[0].range.clone()].to_lowercase() == source.alias.to_lowercase()
            && !unique.contains(&column.to_lowercase())
        {
            issues.push(PackageIssue {
                code: PackageIssueKind::TemporaryColumn,
                scope: owner.scope,
                table: table.name.clone(),
                column: Some(column.to_owned()),
            });
        }
    }
}

/// Invalid lifetimes remain unavailable until DROP; a third creation cannot hide an earlier duplicate.
#[derive(Default)]
struct FlowState {
    tables: BTreeMap<String, TemporaryTable>,
    unavailable: BTreeSet<String>,
    issues: Vec<PackageIssue>,
}

impl FlowState {
    /// Destroy closes either a valid or a known invalid lifetime, preserving unknown-name diagnostics.
    fn destroy(&mut self, scope: usize, name: String) {
        let key = name.to_lowercase();
        let invalid = self.unavailable.remove(&key);
        if self.tables.remove(&key).is_none() && !invalid {
            self.issues.push(PackageIssue {
                code: PackageIssueKind::TemporaryMissing,
                scope,
                table: name,
                column: None,
            });
        }
    }

    /// A producer needs a unique live name and, when known, unique projected column names.
    fn create(&mut self, input: &str, model: &Model, scope: usize, statement: usize, name: String) {
        let key = name.to_lowercase();
        let duplicate = self.tables.remove(&key).is_some() || self.unavailable.contains(&key);
        let columns = model.statements[statement]
            .clauses
            .iter()
            .find(|c| c.kind == ClauseKind::Fields)
            .and_then(|fields| {
                fields
                    .items
                    .iter()
                    .map(|item| item.output_name(input, fields))
                    .collect::<Option<Vec<_>>>()
            });
        let ambiguous = columns.is_some_and(|names| {
            names
                .iter()
                .map(|name| name.to_lowercase())
                .collect::<BTreeSet<_>>()
                .len()
                != names.len()
        });
        if duplicate || ambiguous {
            self.unavailable.insert(key);
            self.issues.push(PackageIssue {
                code: if duplicate {
                    PackageIssueKind::TemporaryDuplicate
                } else {
                    PackageIssueKind::TemporaryAmbiguousColumns
                },
                scope,
                table: name,
                column: None,
            });
        } else {
            self.tables.insert(
                key,
                TemporaryTable {
                    name,
                    scope,
                    statement,
                },
            );
        }
    }
}
