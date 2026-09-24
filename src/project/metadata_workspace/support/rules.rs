//! Parsed supplier rules survive tree generations, but never an unverified file change.

use crate::project::support::Support;
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path, sync::Arc};

const LIMIT: u64 = 64 * 1024 * 1024;

/// Content identity avoids relying on timestamp granularity or branch names.
#[derive(Debug, Default)]
pub(in crate::project::metadata_workspace) struct RuleCache {
    hash: Option<[u8; 32]>,
    readable: bool,
    policy: Option<Arc<Support>>,
    diagnostics: Vec<String>,
}

impl RuleCache {
    /// An unchanged rules-only filesystem event must not invalidate the tree or ownership map.
    pub(in crate::project::metadata_workspace) fn unchanged(&self, root: &Path) -> bool {
        self.readable
            && read_input(root)
                .is_ok_and(|input| self.hash == Some(Sha256::digest(input.as_bytes()).into()))
    }

    /// Reread bytes only when the support snapshot is rebuilt, not per tree node or page.
    pub(super) fn read(&mut self, root: &Path) -> (Option<Arc<Support>>, Vec<String>) {
        match read_input(root) {
            Ok(input) => {
                self.readable = true;
                self.parse(&input)
            }
            Err(error) => {
                self.readable = false;
                (None, vec![error])
            }
        }
    }

    /// Reuse both valid policies and deterministic parse failures for identical bytes.
    fn parse(&mut self, input: &str) -> (Option<Arc<Support>>, Vec<String>) {
        let hash = Sha256::digest(input.as_bytes()).into();
        if self.hash != Some(hash) {
            self.hash = Some(hash);
            self.diagnostics.clear();
            self.policy = match Support::parse(input) {
                Ok(policy) => Some(Arc::new(policy)),
                Err(error) => {
                    self.diagnostics
                        .push(format!("{}:{}", error.code, error.position));
                    None
                }
            };
        }
        (self.policy.clone(), self.diagnostics.clone())
    }
}

/// Validate containment and bound the read even if a file grows after its metadata check.
fn read_input(root: &Path) -> Result<String, String> {
    let path = root
        .join("Ext/ParentConfigurations.bin")
        .canonicalize()
        .map_err(|error| format!("support_read:{:?}", error.kind()))?;
    let root = root
        .canonicalize()
        .map_err(|_| "source_unavailable".to_owned())?;
    if !path.starts_with(root) {
        return Err("support_outside_source".to_owned());
    }
    let file = std::fs::File::open(path).map_err(|_| "support_read".to_owned())?;
    let metadata = file.metadata().map_err(|_| "support_stat".to_owned())?;
    if !metadata.is_file() || metadata.len() > LIMIT {
        return Err("support_size".to_owned());
    }
    let mut input = String::new();
    file.take(LIMIT + 1)
        .read_to_string(&mut input)
        .map_err(|_| "support_read".to_owned())?;
    if input.len() as u64 > LIMIT {
        return Err("support_size".to_owned());
    }
    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_bytes_reuse_parsed_rules_but_changed_or_invalid_bytes_do_not() {
        let mut cache = RuleCache::default();
        let (first, _) = cache.parse("{6,0,0,0,0,0}");
        let (again, _) = cache.parse("{6,0,0,0,0,0}");
        assert!(Arc::ptr_eq(
            first.as_ref().unwrap(),
            again.as_ref().unwrap()
        ));
        let (changed, _) = cache.parse("{6,1,0,0,0,0}");
        assert!(!Arc::ptr_eq(
            first.as_ref().unwrap(),
            changed.as_ref().unwrap()
        ));
        let (invalid, diagnostics) = cache.parse("damaged");
        assert!(invalid.is_none());
        assert!(!diagnostics.is_empty());
        assert!(cache.parse("{6,0,0,0,0,0}").0.is_some());
    }
}
