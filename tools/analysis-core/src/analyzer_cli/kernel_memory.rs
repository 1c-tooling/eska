//! Opt-in native table telemetry never masquerades as a measured or bounded process heap.

use serde::Deserialize;
use serde_json::{Value, json};

use super::{dto, protocol};
use crate::AnalysisSession;

pub const METHOD: &str = "eska/analysis/memory";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Params {
    pub snapshot: protocol::Snapshot,
}

/// Sampling is explicit, has no lazy reads and reports the estimator's process-memory limits.
pub fn capability() -> Value {
    json!({"method":METHOD,"scope":"current-session","lazyRead":false,
        "heapEstimateComplete":false,"processRss":false})
}

/// The connection's decimal snapshot stays separate from the SDK's numeric internal generation.
pub fn report(session: &AnalysisSession, id: &str) -> Value {
    let report = session.kernel_memory();
    json!({"schema_version":1,"snapshot":dto::snapshot(session,id),"resources":session.resources(),
        "kernel":{"tables":report.tables,"tables_without_heap_hook":report.tables_without_heap_hook,
            "live_tables_without_heap_hook":report.live_tables_without_heap_hook,
            "summed_heap_estimates":report.summed_heap_estimates},
        "heap_estimate_complete":false,"process_rss_bytes":null})
}
