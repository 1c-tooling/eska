//! Cumulative accepted input traffic is independent of resident source quota and native heap.

#[cfg(test)]
mod tests;

use crate::{AnalysisSession, SessionRevision};

/// Traffic counts accepted publications, not distinct text, retained allocations or process RSS.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionTraffic {
    /// Saturating sum of each newly published Ready input's full UTF-8 byte length.
    pub cumulative_ready_source_bytes: u64,
    /// The shared revision also counts accepted unavailable states and explicit releases.
    pub accepted_publications: u64,
}

impl AnalysisSession {
    /// Inspect cumulative traffic without reading source, walking owners or changing the session.
    #[must_use]
    pub const fn input_traffic(&self) -> SessionTraffic {
        SessionTraffic {
            cumulative_ready_source_bytes: self.cumulative_ready_source_bytes,
            accepted_publications: self.revision().0,
        }
    }

    /// Call once after an accepted input publication; unavailable inputs and releases supply zero.
    pub(super) const fn commit_input(&mut self, next: SessionRevision, ready_bytes: u64) {
        self.cumulative_ready_source_bytes = self
            .cumulative_ready_source_bytes
            .saturating_add(ready_bytes);
        self.revision = next;
    }
}
