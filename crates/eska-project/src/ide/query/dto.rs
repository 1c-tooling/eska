//! Query values and byte maps are explicit wire projections, without Rust serde coupling.

use std::ops::Range;

use eska_project_core::project::query::{Affinity, Query, QueryIdentity, byte_position};
use serde_json::{Value, json};

/// XML identity survives byte shifts only within the document's independently checked snapshot.
pub(super) fn identity(identity: &QueryIdentity) -> Value {
    match identity {
        QueryIdentity::DataSet(path) => json!({"kind":"dataSet","path":path}),
        QueryIdentity::DynamicList { name, id } => {
            json!({"kind":"dynamicList","name":name,"id":id})
        }
        QueryIdentity::BslLiteral(range) => json!({"kind":"bslLiteral","range":bytes(range)}),
    }
}

/// UTF-8 ranges address the exact source text carried by its snapshot token.
pub(super) fn bytes(range: &Range<usize>) -> Value {
    json!({"start":range.start,"end":range.end})
}

/// UTF-16 editor positions never round an invalid CRLF or surrogate-pair interior.
pub(super) fn point(input: &str, offset: usize) -> Value {
    byte_position(input, offset).map_or(
        Value::Null,
        |point| json!({"line":point.line,"character":point.character}),
    )
}

/// Maps and anchors preserve service-syntax gaps rather than pretending a decoded value is contiguous.
pub(super) fn query(input: &str, query: &Query) -> Value {
    json!({"identity":identity(&query.identity),"text":query.text,"sourceRange":bytes(&query.source),
        "sourceRangeUtf16":{"start":point(input,query.source.start),"end":point(input,query.source.end)},
        "map":{"encoding":"utf-8","spans":query.map.spans().iter().map(|span|json!({"query":bytes(&span.query),"source":bytes(&span.source),"direct":span.direct})).collect::<Vec<_>>(),
        "anchors":{"startBefore":query.map.query_to_source(&query.text,0,Affinity::Before),"startAfter":query.map.query_to_source(&query.text,0,Affinity::After),"endBefore":query.map.query_to_source(&query.text,query.text.len(),Affinity::Before),"endAfter":query.map.query_to_source(&query.text,query.text.len(),Affinity::After)}},
        "comments":query.comments.iter().enumerate().map(|(index,comment)|json!({"comment":index,"source":bytes(&comment.source),"queryOffset":comment.query_offset,"text":comment.text})).collect::<Vec<_>>()})
}
