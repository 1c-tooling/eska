//! Salsa table estimates are observability, not a deduplicated process memory measurement.

#[cfg(test)]
#[path = "memory/tests.rs"]
mod tests;

use crate::{AnalysisSession, SessionRevision};

/// Names may repeat; `None` is an unavailable aggregate, while zero can include evicted memos.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct KernelMemoryTable {
    pub name: &'static str,
    /// Retained instances include metadata-only memos whose computed values were evicted.
    pub entries: usize,
    pub metadata_bytes: usize,
    pub field_stack_bytes: usize,
    pub heap_bytes: Option<usize>,
}

/// Summed estimates may count shared Arcs twice and omit non-Salsa or uninstrumented allocations.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct KernelMemoryReport {
    pub revision: SessionRevision,
    pub tables: Vec<KernelMemoryTable>,
    /// Count null native estimates; mixed aggregates may still omit uninstrumented live values.
    pub tables_without_heap_hook: usize,
    /// Count nonempty instance tables above, not tables with proven live computed values.
    pub live_tables_without_heap_hook: usize,
    pub summed_heap_estimates: usize,
}

impl AnalysisSession {
    /// Sample existing tables without evaluating queries or changing any current input revision.
    #[must_use]
    pub fn kernel_memory(&self) -> KernelMemoryReport {
        let mut tables = self
            .database
            .memory_report()
            .into_iter()
            .map(
                |(name, entries, metadata_bytes, field_stack_bytes, heap_bytes)| {
                    KernelMemoryTable {
                        name,
                        entries,
                        metadata_bytes,
                        field_stack_bytes,
                        heap_bytes,
                    }
                },
            )
            .collect::<Vec<_>>();
        // Salsa reports value types, which do not uniquely identify its ingredient tables.
        tables.sort_unstable_by_key(|table| {
            (
                table.name,
                table.entries,
                table.metadata_bytes,
                table.field_stack_bytes,
                table.heap_bytes,
            )
        });
        KernelMemoryReport {
            revision: self.revision(),
            tables_without_heap_hook: tables
                .iter()
                .filter(|table| table.heap_bytes.is_none())
                .count(),
            live_tables_without_heap_hook: tables
                .iter()
                .filter(|table| table.entries > 0 && table.heap_bytes.is_none())
                .count(),
            summed_heap_estimates: tables.iter().filter_map(|table| table.heap_bytes).sum(),
            tables,
        }
    }
}
