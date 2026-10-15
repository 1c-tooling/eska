//! Standalone query output facts use the native AST/HIR and the owner's current metadata scope.

use std::collections::HashMap;

use ide_db::{query_resolver::FileQueryResolver, RootDatabaseImpl};
use salsa::Database;
use sdbl_hir::{ExprHir, FieldHir, FunctionKind, InValues, SdblHir, SdblType};
use syntax::{ast::AstNode, ast::SdblSelectedField, SyntaxKind, SyntaxNode, TextRange, TextSize};
use vfs::FileId;

use crate::Analysis;

pub use sdbl_hir::SdblDiagnostic as QueryNativeIssue;
pub use sdbl_hir::SdblType as QueryOutputType;

/// Explicit aliases and simple column names are proven; an expression's last identifier is not.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryOutputNameOrigin {
    Alias,
    Column,
    Unknown,
}

/// Field order and duplicates are preserved; a star is one unresolved entry rather than expansion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryOutputField {
    pub ordinal: usize,
    pub name: Option<String>,
    pub name_origin: QueryOutputNameOrigin,
    pub raw_name: Option<String>,
    pub range: TextRange,
    pub expression_range: TextRange,
    pub name_range: Option<TextRange>,
    pub ty: SdblType,
    pub type_complete: bool,
    pub is_asterisk: bool,
}

/// UNION branches remain separate because native lowering does not merge their output types.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuerySelectBranch {
    pub ordinal: usize,
    pub range: TextRange,
    pub fields: Vec<QueryOutputField>,
}

/// A top-level SELECT is identified by its AST boundary, not by flattened native package order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuerySelectStatement {
    pub range: TextRange,
    pub into_table: Option<String>,
    pub branches: Vec<QuerySelectBranch>,
    pub output_names_complete: bool,
    pub output_types_complete: bool,
    pub union_arity_consistent: bool,
}

/// DROP keeps its package position without being mistaken for an output SELECT.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueryTextStatement {
    Select(QuerySelectStatement),
    Drop { range: TextRange },
}

/// Every actual parameter occurrence has exact ranges; declarations and types require owner context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryParameterOccurrence {
    pub name: String,
    pub range: TextRange,
    pub name_range: TextRange,
}

/// Owned native facts do not grant consumer, parameter or temporary-table semantic completeness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryTextModel {
    pub statements: Vec<QueryTextStatement>,
    pub parameters: Vec<QueryParameterOccurrence>,
    pub diagnostics: Vec<QueryNativeIssue>,
    pub syntax_complete: bool,
    /// Known HIR operations do not prove parameter types or semantics outside the native model.
    pub native_expressions_supported: bool,
    pub temporary_table_model_complete: bool,
}

impl Analysis {
    /// Lower exact standalone text against the descriptor FileId's existing per-file resolver.
    ///
    /// Callers must prepare current metadata and enter the database attachment first. Native parse
    /// and lowering have no inner cancellation hook; cancellation is checked before/after each and
    /// between projected items. No BSL body, saved resource or whole Configuration is loaded here.
    pub fn query_text_model(&self, owner: FileId, text: &str) -> QueryTextModel {
        self.database().unwind_if_revision_cancelled();
        let parse = parser::parse_sdbl(text);
        self.database().unwind_if_revision_cancelled();
        let syntax_complete = syntax::sdbl_query::collect_query_parse_errors(&parse).is_empty();
        let resolver = FileQueryResolver::new(self.database(), owner);
        let package = sdbl_hir::lower_sdbl_to_hir_with_resolver(&parse, Some(&resolver));
        self.database().unwind_if_revision_cancelled();
        let native_expressions_supported = package.queries().iter().all(|query| {
            self.database().unwind_if_revision_cancelled();
            supported_query(&query.hir)
        });
        let root = parse.syntax_node();
        let fields: HashMap<_, _> = root
            .descendants()
            .filter_map(SdblSelectedField::cast)
            .map(|field| {
                self.database().unwind_if_revision_cancelled();
                (range_key(field.syntax().text_range()), field)
            })
            .collect();
        let mut statements = Vec::new();
        let mut queries = package.queries().iter().peekable();
        for node in root.children() {
            self.database().unwind_if_revision_cancelled();
            if node.kind() == SyntaxKind::SDBL_DROP_QUERY {
                statements.push(QueryTextStatement::Drop { range: node.text_range() });
            } else if node.kind() == SyntaxKind::SDBL_SELECT_QUERY {
                let range = node.text_range();
                let mut branches = Vec::new();
                let mut into_table = None;
                while let Some(query) = queries.next_if(|query| range.contains_range(query.range)) {
                    if branches.is_empty() {
                        into_table = query.hir.into_table.as_ref().map(ToString::to_string);
                    }
                    let projected = query
                        .hir
                        .select
                        .fields
                        .iter()
                        .enumerate()
                        .map(|(ordinal, field)| {
                            self.database().unwind_if_revision_cancelled();
                            output_field(ordinal, field, fields.get(&range_key(field.range)))
                        })
                        .collect();
                    branches.push(QuerySelectBranch {
                        ordinal: branches.len(),
                        range: query.hir.range,
                        fields: projected,
                    });
                }
                statements.push(QueryTextStatement::Select(select_statement(
                    range,
                    into_table,
                    branches,
                    syntax_complete,
                )));
            }
        }
        let parameters = parameter_occurrences(&root, self.database());
        let diagnostics = package
            .all_diagnostics()
            .map(|issue| {
                self.database().unwind_if_revision_cancelled();
                issue.clone()
            })
            .collect();
        self.database().unwind_if_revision_cancelled();
        let temporary_table_model_complete = !statements.iter().any(|statement| {
            matches!(statement, QueryTextStatement::Select(select) if select.into_table.is_some())
                || matches!(statement, QueryTextStatement::Drop { .. })
        });
        QueryTextModel {
            statements,
            parameters,
            diagnostics,
            syntax_complete,
            native_expressions_supported,
            temporary_table_model_complete,
        }
    }
}

/// Unknown calls can hide under a known outer result type and native diagnostics may omit them.
fn supported_query(query: &SdblHir) -> bool {
    query.select.fields.iter().all(|field| field.is_asterisk || supported_expression(&field.expr))
        && query.all_tables().all(|table| {
            table.virtual_table_params.iter().all(supported_expression)
                && table.subquery.iter().all(|query| supported_query(query))
        })
        && query.joins.iter().all(|join| join.condition.as_ref().is_none_or(supported_expression))
        && query.where_clause.as_ref().is_none_or(supported_expression)
        && query.having.as_ref().is_none_or(supported_expression)
        && query.group_by.as_ref().is_none_or(|group| group.exprs.iter().all(supported_expression))
        && query
            .order_by
            .as_ref()
            .is_none_or(|order| order.items.iter().all(|item| supported_expression(&item.expr)))
        && query.unions.iter().all(|union| supported_query(&union.query))
}

/// Parameters retain their separate declaration/type uncertainty; missing or unknown operations do not.
fn supported_expression(expression: &ExprHir) -> bool {
    match expression {
        ExprHir::ColumnRef { .. } | ExprHir::Literal { .. } | ExprHir::Parameter { .. } => true,
        ExprHir::Missing { .. } => false,
        ExprHir::BinaryOp { lhs, rhs, .. } => {
            supported_expression(lhs) && supported_expression(rhs)
        }
        ExprHir::UnaryOp { expr, .. } | ExprHir::IsNull { expr, .. } => supported_expression(expr),
        ExprHir::FunctionCall { function, args, .. } => {
            !matches!(function, FunctionKind::Unknown(_)) && args.iter().all(supported_expression)
        }
        ExprHir::Case { operand, when_clauses, else_expr, .. } => {
            operand.as_deref().is_none_or(supported_expression)
                && when_clauses.iter().all(|clause| {
                    supported_expression(&clause.condition) && supported_expression(&clause.result)
                })
                && else_expr.as_deref().is_none_or(supported_expression)
        }
        ExprHir::Subquery { query, .. } => supported_query(query),
        ExprHir::In { expr, values, .. } => {
            supported_expression(expr)
                && match values {
                    InValues::List(values) => values.iter().all(supported_expression),
                    InValues::Subquery(query) => supported_query(query),
                }
        }
        ExprHir::Between { expr, low, high, .. } => {
            supported_expression(expr) && supported_expression(low) && supported_expression(high)
        }
        ExprHir::Like { expr, pattern, escape, .. } => {
            supported_expression(expr)
                && supported_expression(pattern)
                && escape.as_deref().is_none_or(supported_expression)
        }
        ExprHir::Tuple { elements, .. } => elements.iter().all(supported_expression),
    }
}

/// The first branch defines names; all branches must have a proven shape before absence is useful.
fn select_statement(
    range: TextRange,
    into_table: Option<String>,
    branches: Vec<QuerySelectBranch>,
    syntax_complete: bool,
) -> QuerySelectStatement {
    let first = branches.first();
    let union_arity_consistent = first.is_some_and(|first| {
        branches.iter().all(|branch| branch.fields.len() == first.fields.len())
    });
    let output_names_complete = syntax_complete
        && union_arity_consistent
        && branches.iter().all(|branch| {
            !branch.fields.is_empty()
                && branch.fields.iter().all(|field| !field.is_asterisk && field.name.is_some())
        });
    let output_types_complete = output_names_complete
        && branches.len() == 1
        && first.is_some_and(|branch| branch.fields.iter().all(|field| field.type_complete));
    QuerySelectStatement {
        range,
        into_table,
        branches,
        output_names_complete,
        output_types_complete,
        union_arity_consistent,
    }
}

/// Only explicit alias or a simple column grants a runtime name; the native raw-name heuristic stays separate.
fn output_field(
    ordinal: usize,
    field: &FieldHir,
    ast: Option<&SdblSelectedField>,
) -> QueryOutputField {
    let (name, name_origin) = if let Some(alias) = &field.alias {
        (Some(alias.to_string()), QueryOutputNameOrigin::Alias)
    } else if let ExprHir::ColumnRef { parts, .. } = &field.expr {
        (parts.last().map(ToString::to_string), QueryOutputNameOrigin::Column)
    } else {
        (None, QueryOutputNameOrigin::Unknown)
    };
    let name_range = match name_origin {
        QueryOutputNameOrigin::Alias => ast
            .and_then(SdblSelectedField::alias)
            .and_then(|alias| alias.identifier())
            .map(|token| token.text_range()),
        QueryOutputNameOrigin::Column => ast
            .and_then(SdblSelectedField::expression)
            .and_then(|expression| {
                expression
                    .descendants_with_tokens()
                    .filter_map(|element| element.into_token())
                    .filter(|token| token.kind().is_name_token())
                    .last()
            })
            .map(|token| token.text_range()),
        QueryOutputNameOrigin::Unknown => None,
    };
    QueryOutputField {
        ordinal,
        name,
        name_origin,
        raw_name: field.raw_name.as_ref().map(ToString::to_string),
        range: field.range,
        expression_range: field.expr.range(),
        name_range,
        ty: field.ty.clone(),
        type_complete: !field.has_parse_error && !field.is_asterisk && type_complete(&field.ty),
        is_asterisk: field.is_asterisk,
    }
}

/// Unresolved alternatives inside a composite or defined type remain unknown at every nesting depth.
fn type_complete(ty: &SdblType) -> bool {
    match ty {
        SdblType::Unknown
        | SdblType::Error
        | SdblType::DefinedType { underlying_type: None, .. } => false,
        SdblType::DefinedType { underlying_type: Some(inner), .. } | SdblType::Aggregate(inner) => {
            type_complete(inner)
        }
        SdblType::Composite { types } => !types.is_empty() && types.iter().all(type_complete),
        _ => true,
    }
}

/// AST parameter nodes exclude comments/strings and preserve repeated spelling in native source order.
fn parameter_occurrences(
    root: &SyntaxNode,
    db: &RootDatabaseImpl,
) -> Vec<QueryParameterOccurrence> {
    root.descendants()
        .filter(|node| node.kind() == SyntaxKind::SDBL_PARAMETER)
        .filter_map(|node| {
            db.unwind_if_revision_cancelled();
            let token = node
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .find(|token| token.text().starts_with('&') && token.text().len() > 1)?;
            let range = token.text_range();
            Some(QueryParameterOccurrence {
                name: token.text()[1..].to_owned(),
                range,
                name_range: TextRange::new(range.start() + TextSize::from(1), range.end()),
            })
        })
        .collect()
}

/// Tuple keys avoid relying on an ordering contract for the parser's range type.
fn range_key(range: TextRange) -> (u32, u32) {
    (range.start().into(), range.end().into())
}

#[cfg(test)]
mod tests;
