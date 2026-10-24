//! DCS fragments are a separate lossless projection; masking is used only to prove insertion sites, never for writes.

mod edits;
mod scan;

use std::ops::Range;

use super::{Comparison, Error, Model, lex};

pub(super) use edits::plan;

/// Only documented DCS positions can acquire typed edit authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DcsKind {
    Select,
    Where,
    OptionalSource,
    Parameter,
    VirtualFilter,
    Opaque,
}

/// Projection aliases and source fields have different validation rules despite sharing field controls.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum DcsValue {
    Field {
        expression: String,
        children: bool,
        alias: String,
    },
    Condition {
        left: String,
        comparison: Comparison,
        right: String,
    },
    Parameter {
        name: String,
    },
}

/// A node addresses exact original bytes; opaque nodes deliberately expose no editable guessed values.
#[derive(Clone, Debug, serde::Serialize)]
pub struct DcsNode {
    pub index: usize,
    pub kind: DcsKind,
    pub range: Range<usize>,
    pub values: Vec<DcsValue>,
    pub enabled: bool,
    pub editable: bool,
    pub comments: usize,
}

/// All permissions and positions come from the backend's grammar and virtual-table signatures.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DcsStatement {
    pub russian: bool,
    pub nodes: Vec<DcsNode>,
    pub add_select: bool,
    pub add_where: bool,
    #[serde(skip)]
    select: Option<usize>,
    #[serde(skip)]
    condition: Option<usize>,
    #[serde(skip)]
    columns: Vec<String>,
}

/// This temporary projection has no second canonical query or persistent cache.
#[derive(Clone, Debug)]
pub struct DcsModel {
    pub statements: Vec<DcsStatement>,
}

/// A malformed query remains inspectable without granting fragment write authority.
pub(super) fn inspect(input: &str, original: &Model) -> Result<DcsModel, Error> {
    let tokens = match lex::tokens(input) {
        Ok(tokens) => tokens,
        Err(Error::InvalidSyntax) => return Ok(empty(original)),
        Err(error) => return Err(error),
    };
    scan::inspect(input, original, &tokens)
}

/// Invalid syntax does not turn an empty list into permission to insert a new extension block.
fn empty(original: &Model) -> DcsModel {
    DcsModel {
        statements: original
            .statements
            .iter()
            .map(|statement| DcsStatement {
                russian: statement.russian,
                nodes: Vec::new(),
                add_select: false,
                add_where: false,
                select: None,
                condition: None,
                columns: Vec::new(),
            })
            .collect(),
    }
}
