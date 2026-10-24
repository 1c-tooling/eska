//! Transport projections expose domain relationships and permissions; an adapter never reconstructs query structure.

use super::{MAX_EXPRESSION, MAX_MODEL_BYTES, failure};
use crate::ide::envelope::error;
use eska_project_core::project::{
    metadata_edit::snapshot,
    query::constructor::{Clause, ClauseKind, Constructor, Item, Selection, Statement},
};
use serde_json::{Value, json};

/// Exact text and bounded ranges are the only writable model; DTO choices contain no secondary query document.
pub(super) fn model(
    value: &Constructor,
    id: &str,
    writable: bool,
    minor: u32,
) -> Result<Value, Value> {
    let view = View {
        value,
        writable,
        minor,
    };
    let input = value.text();
    let mut statements: Vec<_> = value
        .model()
        .statements
        .iter()
        .enumerate()
        .map(|(index, statement)| view.statement(index, statement))
        .collect();
    if minor >= 29 {
        let dcs = value
            .dcs_model()
            .map_err(|_| failure("constructor_limit"))?;
        for (statement, dcs) in statements.iter_mut().zip(dcs.statements) {
            let mut dcs = json!(dcs);
            for node in dcs["nodes"].as_array_mut().into_iter().flatten() {
                if let (Some(start), Some(end)) = (
                    node["range"]["start"]
                        .as_u64()
                        .and_then(|v| usize::try_from(v).ok()),
                    node["range"]["end"]
                        .as_u64()
                        .and_then(|v| usize::try_from(v).ok()),
                ) {
                    node["preview"] = json!(preview(&input[start..end]));
                }
            }
            if !writable {
                dcs["addSelect"] = json!(false);
                dcs["addWhere"] = json!(false);
                for node in dcs["nodes"].as_array_mut().into_iter().flatten() {
                    node["editable"] = json!(false);
                }
            }
            statement["dcs"] = dcs;
        }
    }
    let mut output = json!({"id":id,"revision":value.revision().to_string(),"snapshot":snapshot(input),"text":input,
        "bytes":input.len(),"writable":writable,"statements":statements});
    if minor >= 23 {
        output["focus"] = json!(value.focus());
        output["issues"] = json!(
            value
                .model()
                .union_mismatches(input)
                .into_iter()
                .map(|scope| json!({"code":"union_field_count","scope":scope}))
                .collect::<Vec<_>>()
        );
        output["scopes"] = json!(value.model().scopes.iter().enumerate().map(|(index, scope)| json!({"index":index,"range":bytes(&scope.range),"parent":scope.parent,"branches":scope.branches,
            "result":scope.branches.last(),"unions":scope.unions.iter().map(|union|json!({"all":union.all.is_some()})).collect::<Vec<_>>()})).collect::<Vec<_>>());
    }
    if minor >= 28 {
        output["totalFocus"] = json!(value.total_focus());
    }
    if minor >= 26
        && let Some(issues) = output["issues"].as_array_mut()
    {
        issues.extend(
            value
                .model()
                .package_issues(input)
                .iter()
                .map(|issue| json!(issue)),
        );
    }
    if serde_json::to_vec(&output)
        .map_err(|_| error(-32603))?
        .len()
        > MAX_MODEL_BYTES
    {
        return Err(failure("constructor_limit"));
    }
    Ok(output)
}

struct View<'a> {
    value: &'a Constructor,
    writable: bool,
    minor: u32,
}

impl View<'_> {
    /// Older negotiated adapters see composite queries as exact opaque text rather than unknown clause identifiers.
    fn statement(&self, index: usize, statement: &Statement) -> Value {
        let input = self.value.text();
        let opaque = (self.minor < 23 && self.value.model().requires_scopes(index))
            || (self.minor < 26 && self.value.model().requires_packages(index))
            || (self.minor < 27 && self.value.model().requires_having(index))
            || (self.minor < 28 && self.value.model().requires_multiple_totals(index));
        let clauses: Vec<_> = if opaque {
            Vec::new()
        } else {
            statement
                .clauses
                .iter()
                .enumerate()
                .map(|(clause, value)| self.clause(index, clause, value))
                .collect()
        };
        let kinds = ClauseKind::all(self.minor >= 23)
            .iter()
            .copied()
            .chain(
                (self.minor >= 26)
                    .then_some([ClauseKind::Into, ClauseKind::Destroy, ClauseKind::Index])
                    .into_iter()
                    .flatten(),
            )
            .chain((self.minor >= 27).then_some(ClauseKind::Having));
        let append: Vec<_> = kinds.map(|kind| json!({"kind":kind,"unavailable":
            if !self.writable {json!("read_only")} else if opaque {json!("unsupported")} else {json!(statement.append_unavailable(kind))}})).collect();
        let bindings = if opaque {
            Vec::new()
        } else {
            statement.source_bindings(input)
        };
        let sources: Vec<_> = bindings
            .into_iter()
            .map(|source| {
                let nested = source
                    .derived
                    .then(|| {
                        self.value.model().source_scope(Selection {
                            statement: index,
                            clause: source.clause,
                            item: source.item,
                        })
                    })
                    .flatten();
                let mut value = json!(source);
                value["nestedScope"] = json!(nested);
                value
            })
            .collect();
        let first = self.value.model().scopes[statement.scope].branches[0];
        let columns: Vec<_> = self.value.model().statements[first]
            .clauses
            .iter()
            .filter(|clause| clause.kind == ClauseKind::Fields)
            .flat_map(|clause| {
                clause
                    .items
                    .iter()
                    .filter_map(|item| item.output_name(input, clause))
            })
            .collect();
        let mut output = json!({"index":index,"range":bytes(&statement.range),"russian":statement.russian,"sources":sources,"scope":statement.scope,
            "branch":statement.branch,"result":statement.result,"nested":statement.nested,"columns":columns,
            "unavailable":if opaque {json!(if statement.clauses.iter().any(|c| c.kind==ClauseKind::Destroy) {"unsupported_statement"} else {"unsupported_construct"})} else {json!(statement.unavailable)},"preview":preview(&input[statement.range.clone()]),"clauses":clauses,"append":append});
        if self.minor >= 26 {
            let roots: Vec<_> = self
                .value
                .model()
                .scopes
                .iter()
                .filter(|scope| scope.parent.is_none())
                .collect();
            let position = roots
                .iter()
                .position(|scope| scope.branches.contains(&index));
            output["package"] = json!({"operation": self.value.model().package_operation(index), "position":position,
                "first":first, "destination": self.value.model().temporary_destination(input, statement.scope),
                "add":self.writable && !opaque && !statement.nested && statement.unavailable.is_none(),
                "remove":self.writable && position.is_some() && roots.len()>1 && statement.unavailable.is_none(),
                "moveUp":position.filter(|_|self.writable && statement.unavailable.is_none()).and_then(|at| at.checked_sub(1)),
                "moveDown":position.filter(|at|self.writable && statement.unavailable.is_none() && at+1<roots.len()).map(|at|at+1),
                "setInto":self.writable && !statement.nested && self.value.model().package_operation(index)==Some(eska_project_core::project::query::constructor::PackageOperation::Select)});
        }
        if self.minor >= 28 {
            let sections = if opaque {
                Vec::new()
            } else {
                statement.total_sections()
            };
            let allowed = self.writable
                && !opaque
                && statement.append_unavailable(ClauseKind::Totals).is_none();
            output["addTotals"] = json!(allowed && sections.len() < 32);
            output["totalSections"] = json!(sections.iter().map(|section| json!({"index":section.index,"fields":section.fields,"groups":section.groups,"range":bytes(&section.range),"remove":allowed,
                "moveUp":section.index.checked_sub(1).filter(|_|allowed), "moveDown":(allowed && section.index+1<sections.len()).then_some(section.index+1)})).collect::<Vec<_>>());
        }
        output
    }

    /// Clause ordering and exact headers are supplied once by ESKA.
    fn clause(&self, statement: usize, index: usize, clause: &Clause) -> Value {
        let items: Vec<_> = clause
            .items
            .iter()
            .enumerate()
            .map(|(item, value)| self.item(statement, index, item, value, clause))
            .collect();
        json!({"index":index,"kind":clause.kind,"header":preview(self.value.text()[clause.range.start..clause.body.start].trim()),"items":items})
    }

    /// Each button receives its own domain permission, including fixed OVERALL placement and nested navigation.
    fn item(
        &self,
        statement: usize,
        _clause: usize,
        index: usize,
        item: &Item,
        clause: &Clause,
    ) -> Value {
        let input = self.value.text();
        let expression = item.expression(input);
        let too_large = expression.len() > MAX_EXPRESSION;
        let visual = item.visual(input, clause).filter(|_| (self.minor >= 23) || !matches!(clause.kind, ClauseKind::Totals | ClauseKind::TotalGroups))
            .filter(|value| (self.minor >= 23) || !matches!(value, eska_project_core::project::query::constructor::Expression::VirtualSource { .. } | eska_project_core::project::query::constructor::Expression::VirtualJoin { .. }));
        let up = self.writable
            && index
                .checked_sub(1)
                .is_some_and(|to| clause.move_unavailable(input, index, to).is_none());
        let down = self.writable && clause.move_unavailable(input, index, index + 1).is_none();
        let removable = self.writable
            && (clause.items.len() > 1
                || !matches!(clause.kind, ClauseKind::Fields | ClauseKind::Sources));
        let children: Vec<_> = self
            .value
            .model()
            .scopes
            .iter()
            .enumerate()
            .filter(|(_, scope)| {
                scope.parent == Some(statement)
                    && scope.range.start >= item.range.start
                    && scope.range.end <= item.range.end
            })
            .map(|(index, _)| index)
            .collect();
        json!({"index":index,"range":bytes(&item.range),"preview":preview(&input[item.range.clone()]),
            "visual":if too_large {None} else {visual},"expression":if too_large {None} else {Some(expression)},"comments":item.comments.len(),
            "removable":removable,"movable":up||down,"moveUp":up,"moveDown":down,"children":children,
            "unavailable":{"replace":if !self.writable {Some("read_only")} else if too_large {Some("expression_too_large")} else {None},
                "remove":if !self.writable {Some("read_only")} else if !removable {Some("required_item")} else {None},
                "move":if !self.writable {Some("read_only")} else if clause.kind==ClauseKind::Joins {Some("join_order_dependency")}
                    else if clause.items.len()<2 {Some("single_item")} else if !up&&!down {Some("required_item")} else {None}}})
    }
}

/// Byte ranges remain machine-readable and independent of the interface locale.
fn bytes(range: &std::ops::Range<usize>) -> Value {
    json!({"start":range.start,"end":range.end})
}

/// Display clipping never changes the canonical text or operation coordinates.
fn preview(input: &str) -> String {
    input.chars().take(512).collect()
}
