use super::{ProjectSession, WorkspaceError};
use crate::project::{
    metadata_model::{NodeId, ObjectId},
    metadata_parser::{self, LoadError, ParsedDescriptor, PropertiesMode},
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

/// Bounds retained parsed descriptors, not the lightweight visible navigation tree.
#[derive(Clone, Copy, Debug)]
pub struct CacheLimits {
    pub descriptors: usize,
    /// Budget in original XML bytes; decoded Rust allocations also have overhead.
    pub source_bytes: usize,
}

impl Default for CacheLimits {
    /// A modest session-local cache; oversized descriptors may be read but are not retained.
    fn default() -> Self {
        Self {
            descriptors: 32,
            source_bytes: 8 * 1024 * 1024,
        }
    }
}

/// Counters make actual parsing and eviction observable without retaining a growing event log.
#[derive(Clone, Debug, Default)]
pub struct CacheStats {
    pub parses: u64,
    pub hits: u64,
    pub evictions: u64,
    pub descriptors: usize,
    pub source_bytes: usize,
    pub last_parsed: Option<PathBuf>,
}

#[derive(Debug)]
struct Entry {
    parsed: Arc<ParsedDescriptor>,
    mode: PropertiesMode,
    bytes: usize,
    touched: u64,
}

#[derive(Debug, Default)]
pub(super) struct DescriptorCache {
    entries: BTreeMap<PathBuf, Entry>,
    limits: CacheLimits,
    stats: CacheStats,
    clock: u64,
}

impl DescriptorCache {
    /// Reuse summaries from complete entries, but upgrade summary-only data for properties.
    fn get(&mut self, path: &PathBuf, mode: PropertiesMode) -> Option<Arc<ParsedDescriptor>> {
        let entry = self.entries.get_mut(path)?;
        if mode == PropertiesMode::All && entry.mode != PropertiesMode::All {
            return None;
        }
        self.clock = self.clock.saturating_add(1);
        entry.touched = self.clock;
        self.stats.hits += 1;
        Some(Arc::clone(&entry.parsed))
    }

    /// Drop one owning descriptor; unrelated entries preserve their recency.
    pub(super) fn remove(&mut self, path: &PathBuf) {
        if let Some(entry) = self.entries.remove(path) {
            self.stats.source_bytes -= entry.bytes;
            self.stats.descriptors -= 1;
        }
    }

    /// Clear retained parsed values while preserving limits and diagnostic counters.
    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.stats.descriptors = 0;
        self.stats.source_bytes = 0;
    }

    /// Evict least-recently-used entries until both configured budgets are met.
    fn trim(&mut self) {
        while self.entries.len() > self.limits.descriptors
            || self.stats.source_bytes > self.limits.source_bytes
        {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.touched)
                .map(|(path, _)| path.clone());
            let Some(path) = oldest else { break };
            self.remove(&path);
            self.stats.evictions += 1;
        }
    }
}

impl ProjectSession {
    /// Adjust the session budget; zero disables parsed-descriptor retention.
    pub fn set_cache_limits(&mut self, limits: CacheLimits) {
        self.cache.limits = limits;
        self.cache.trim();
    }

    /// Inspect parser counters and retained-cache size without IO.
    #[must_use]
    pub fn cache_stats(&self) -> CacheStats {
        self.cache.stats.clone()
    }

    /// Read and validate a descriptor before admitting it to the session cache.
    pub(super) fn load(
        &mut self,
        id: &ObjectId,
        mode: PropertiesMode,
    ) -> Result<Arc<ParsedDescriptor>, WorkspaceError> {
        let (owner, candidates) = self
            .source
            .descriptor_candidates(id)
            .map_err(WorkspaceError::Source)?;
        for path in &candidates {
            if let Some(parsed) = self.cache.get(path, mode) {
                return Self::require_object(parsed, id);
            }
        }
        let location = self
            .source
            .object_descriptor(id)
            .map_err(WorkspaceError::Source)?
            .ok_or_else(|| WorkspaceError::MissingSource(NodeId::Object(id.clone())))?;
        let path = location.path;
        let input = self
            .source
            .read_xml(&path)
            .map_err(WorkspaceError::Source)?
            .ok_or_else(|| WorkspaceError::MissingSource(NodeId::Object(id.clone())))?;
        let hash: [u8; 32] = Sha256::digest(input.as_bytes()).into();
        if self
            .fingerprints
            .get(&path)
            .is_some_and(|previous| previous != &hash)
        {
            return Err(WorkspaceError::SourceChanged(path));
        }
        self.cache.stats.parses += 1;
        self.cache.stats.last_parsed = Some(path.clone());
        let parsed = metadata_parser::parse(&input, owner.parent(), mode).map_err(|source| {
            WorkspaceError::Load(LoadError::Parse {
                path: path.clone(),
                source,
            })
        })?;
        // Detect writers racing the read/parse; never admit partially superseded XML.
        if self
            .source
            .read_xml(&path)
            .map_err(WorkspaceError::Source)?
            .as_deref()
            != Some(input.as_str())
        {
            return Err(WorkspaceError::SourceChanged(path));
        }
        let parsed = Self::require_object(Arc::new(parsed), id)?;
        self.fingerprints.insert(path.clone(), hash);
        self.cache.remove(&path);
        self.cache.clock = self.cache.clock.saturating_add(1);
        self.cache.entries.insert(
            path,
            Entry {
                parsed: Arc::clone(&parsed),
                mode,
                bytes: input.len(),
                touched: self.cache.clock,
            },
        );
        self.cache.stats.descriptors += 1;
        self.cache.stats.source_bytes += input.len();
        self.cache.trim();
        Ok(parsed)
    }

    /// Check again after projection preparation, before publishing the branch.
    pub(super) fn verify_source(&mut self, id: &ObjectId) -> Result<(), WorkspaceError> {
        let location = self
            .source
            .object_descriptor(id)
            .map_err(WorkspaceError::Source)?
            .ok_or_else(|| WorkspaceError::MissingSource(NodeId::Object(id.clone())))?;
        let current = self
            .source
            .read_xml(&location.path)
            .map_err(WorkspaceError::Source)?;
        let hash = current
            .as_ref()
            .map(|text| <[u8; 32]>::from(Sha256::digest(text.as_bytes())));
        if hash.as_ref() != self.fingerprints.get(&location.path) {
            self.cache.remove(&location.path);
            self.fingerprints.remove(&location.path);
            return Err(WorkspaceError::SourceChanged(location.path));
        }
        Ok(())
    }

    /// A cached descriptor may contain many inline objects, but not every requested identity.
    fn require_object(
        parsed: Arc<ParsedDescriptor>,
        id: &ObjectId,
    ) -> Result<Arc<ParsedDescriptor>, WorkspaceError> {
        if parsed
            .objects
            .iter()
            .any(|object| object.metadata.id() == id)
        {
            Ok(parsed)
        } else {
            Err(WorkspaceError::Load(LoadError::ObjectNotFound(id.clone())))
        }
    }
}
