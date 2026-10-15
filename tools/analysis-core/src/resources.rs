//! Bound current source reservations across YAML, metadata resources, BSL and queries without claiming RSS control.

#[cfg(test)]
#[path = "resources/tests.rs"]
mod tests;

use eska::analysis_core::InputRequest;

use crate::AnalysisSession;

/// Parsed heap, temporary allocations, composition and kernel memos are outside this byte meter.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct ResourceSummary {
    pub metadata_source_bytes: u64,
    pub module_text_bytes: u64,
    pub query_text_bytes: u64,
    pub total_source_bytes: u64,
    pub max_source_bytes: u64,
}

impl AnalysisSession {
    /// Count retained authoring text once across all input kinds, independently of projection readiness.
    #[must_use]
    pub const fn resources(&self) -> ResourceSummary {
        ResourceSummary {
            metadata_source_bytes: self.loaded_metadata_source_bytes + self.loaded_resource_bytes,
            module_text_bytes: self.loaded_module_bytes,
            query_text_bytes: self.loaded_query_bytes,
            total_source_bytes: self.loaded_metadata_source_bytes
                + self.loaded_resource_bytes
                + self.loaded_module_bytes
                + self.loaded_query_bytes,
            max_source_bytes: self.options.max_retained_source_bytes,
        }
    }

    /// Replacing one input may reuse its old reservation; only accepted publication changes usage.
    pub(super) fn source_budget_request(
        &self,
        previous: u64,
        request: &InputRequest,
    ) -> InputRequest {
        let occupied = (self.loaded_metadata_source_bytes
            + self.loaded_resource_bytes
            + self.loaded_module_bytes
            + self.loaded_query_bytes)
            - previous;
        request.with_source_budget(
            self.options
                .max_retained_source_bytes
                .saturating_sub(occupied),
            self.options.max_retained_source_bytes,
        )
    }
}
