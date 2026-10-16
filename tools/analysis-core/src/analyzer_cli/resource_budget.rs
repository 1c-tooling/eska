//! Process retirement bounds retained identity lifetimes without claiming a byte-accurate RSS cap.

use serde_json::{Value, json};

use super::{args::Cli, protocol};
use crate::AnalysisSession;

const KEYS: [&str; 4] = [
    "cumulative_ready_source_bytes",
    "accepted_publications",
    "type_kernel_entries",
    "interner_norm_entries",
];

/// All counts use decimal strings on the wire, preserving the complete unsigned 64-bit range.
#[derive(Clone, Copy, Debug)]
pub(super) struct Values {
    pub cumulative_ready_source_bytes: u64,
    pub accepted_publications: u64,
    pub type_kernel_entries: u64,
    pub interner_norm_entries: u64,
}

impl Values {
    /// Keep exact counters independent of JSON number precision in editor runtimes.
    fn wire(self) -> Value {
        json!({
            "cumulative_ready_source_bytes":self.cumulative_ready_source_bytes.to_string(),
            "accepted_publications":self.accepted_publications.to_string(),
            "type_kernel_entries":self.type_kernel_entries.to_string(),
            "interner_norm_entries":self.interner_norm_entries.to_string(),
        })
    }

    /// Counter order matches the closed public key inventory, including zero thresholds.
    const fn ordered(self) -> [u64; 4] {
        [
            self.cumulative_ready_source_bytes,
            self.accepted_publications,
            self.type_kernel_entries,
            self.interner_norm_entries,
        ]
    }
}

pub(super) struct Policy {
    pub negotiated: bool,
    limits: Values,
}

impl Policy {
    /// Negotiation changes automatic replay admission, while the host's configured limits stay enforced.
    pub const fn new(cli: &Cli, negotiated: bool) -> Self {
        Self {
            negotiated,
            limits: Values {
                cumulative_ready_source_bytes: cli.max_process_source_bytes,
                accepted_publications: cli.max_process_input_publications,
                type_kernel_entries: cli.max_type_kernel_entries,
                interner_norm_entries: cli.max_interner_norm_entries,
            },
        }
    }

    /// Clients opt into one explicit resource cause instead of fabricating source-import events.
    pub fn capability(&self) -> Value {
        json!({"version":1,"enabled":self.negotiated,"scope":"process",
            "notification":"eska/restartRequired","cause":"resource_limit",
            "checkBoundary":"after-published-frame","overshoot":"one-operation",
            "processRssLimit":false,"limits":self.limits.wire()})
    }

    /// Called only at the serial worker's published boundary, with no native query handle alive.
    pub fn exceeded(&self, session: &AnalysisSession) -> Option<Value> {
        let traffic = session.input_traffic();
        let native = session.retention_counters();
        let counters = Values {
            cumulative_ready_source_bytes: traffic.cumulative_ready_source_bytes,
            accepted_publications: traffic.accepted_publications,
            type_kernel_entries: u64::try_from(native.type_kernel_entries).unwrap_or(u64::MAX),
            interner_norm_entries: u64::try_from(native.interner_norm_entries).unwrap_or(u64::MAX),
        };
        self.report(counters)
    }

    /// A completed operation may overshoot; the next operation cannot continue the retired generation.
    fn report(&self, counters: Values) -> Option<Value> {
        let exceeded = KEYS
            .into_iter()
            .zip(counters.ordered())
            .zip(self.limits.ordered())
            .filter_map(|((key, count), limit)| (count >= limit).then_some(key))
            .collect::<Vec<_>>();
        (!exceeded.is_empty()).then(|| json!({"schema_version":1,
            "limits":self.limits.wire(),"counters":counters.wire(),"exceeded":exceeded,
            "checked_after":"published-frame","overshoot":"one-operation","process_rss_limit":false}))
    }
}

/// Reject invalid opt-in values before opening a project or publishing a new session.
pub(super) fn negotiate(params: &Value) -> Result<bool, Value> {
    match params.pointer("/initializationOptions/eska/resourceRestart") {
        None => Ok(false),
        Some(Value::Bool(enabled)) => Ok(*enabled),
        Some(_) => Err(protocol::error_value(
            -32602,
            "rpc_invalid_params",
            Value::Null,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exact decimal limits and reached counters neither round nor wrap at the u64 boundary.
    #[test]
    fn exact_limits_zero_and_all_exceeded_keys_are_preserved() {
        let limits = Values {
            cumulative_ready_source_bytes: u64::MAX,
            accepted_publications: 2,
            type_kernel_entries: 0,
            interner_norm_entries: 4,
        };
        let policy = Policy {
            negotiated: true,
            limits,
        };
        let counters = Values {
            cumulative_ready_source_bytes: u64::MAX,
            accepted_publications: 1,
            type_kernel_entries: 6,
            interner_norm_entries: 4,
        };
        let report = policy.report(counters).unwrap();
        assert_eq!(report["exceeded"], json!([KEYS[0], KEYS[2], KEYS[3]]));
        assert_eq!(report["limits"][KEYS[0]], u64::MAX.to_string());
        assert_eq!(report["counters"][KEYS[0]], u64::MAX.to_string());
        assert_eq!(report["process_rss_limit"], false);
    }
}
