//! Embedded queries use the same SDBL algorithm and core-owned source maps as standalone drafts.

use eska_project_core::{
    formatting::{Error, Language, Options, protocol::Request},
    project::{
        metadata_edit::EditPlan,
        query::{self, LineEnding, QueryIdentity},
    },
};

/// Format exactly one static BSL literal or XML query in a checked owner snapshot.
///
/// # Errors
/// Rejects dynamic expressions, stale snapshots, unsupported queries and unsafe source-comment mappings.
pub fn format_query(
    input: &str,
    snapshot: &str,
    identity: &QueryIdentity,
    options: Options,
    line_ending: Option<LineEnding>,
) -> Result<EditPlan, Error> {
    if eska_project_core::project::metadata_edit::snapshot(input) != snapshot {
        return Err(Error::new("source_conflict", "query"));
    }
    eska_project_core::formatting::protection::check_query(input, identity)?;
    let query = match identity {
        QueryIdentity::BslLiteral(range) => query::read_bsl(input, range.start),
        _ => query::read_xml(input).and_then(|queries| {
            queries
                .into_iter()
                .find(|query| &query.identity == identity)
                .ok_or(query::QueryError::NotFound)
        }),
    }
    .map_err(|_| Error::new("unsupported_source", "query"))?;
    let request = Request {
        schema_version: 1,
        language: Language::Sdbl,
        text: query.text,
        options,
        range: None,
    };
    let response = crate::format(&request)?;
    let mut text = request.text;
    for edit in response.edits.iter().rev() {
        text.replace_range(edit.range.start..edit.range.end, &edit.text);
    }
    query::format_plan(input, snapshot, identity, &text, line_ending).map_err(|error| {
        Error::new(
            match error {
                query::QueryError::Conflict => "source_conflict",
                _ => "unsupported_source",
            },
            "query",
        )
    })
}
