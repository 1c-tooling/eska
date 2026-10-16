//! Native retention samples distinguish database-local types from process-wide names.

use ide_db::RootDatabaseImpl;

/// Entry counts describe retention scopes, not deduplicated heap bytes or process RSS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetentionCounters {
    /// Includes native sentinels and all types retained by this database generation.
    pub type_kernel_entries: usize,
    /// Canonical folded names survive database replacement until the process exits.
    pub interner_norm_entries: usize,
}

/// Sample current retained entries without evaluating analysis or reading source files.
///
/// Interner sampling uses the existing pool statistics and walks retained strings,
/// so its cost is O(retained strings). The two concurrent samples are not one atomic
/// snapshot; neither count is a heap estimate, RSS measurement or enforced budget.
#[must_use]
pub fn retention_counters(database: &RootDatabaseImpl) -> RetentionCounters {
    RetentionCounters {
        type_kernel_entries: database.type_kernel_entries(),
        interner_norm_entries: ide_db::database::interner_norm_entries(),
    }
}
