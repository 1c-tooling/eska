//! Backend source restrictions are owned leases, independent of native editor flags and support policy.

use std::{
    collections::BTreeMap,
    num::NonZeroU64,
    path::{Path, PathBuf},
    sync::{Arc, Weak},
};

/// Weak entries cannot keep a forgotten source session locked after its final owner is dropped.
#[derive(Clone, Debug, Default)]
pub(super) struct SourceProtections {
    serial: u64,
    owners: BTreeMap<PathBuf, Weak<Restriction>>,
}

#[derive(Debug)]
struct Restriction {
    id: NonZeroU64,
    owner: PathBuf,
    descriptor: PathBuf,
    _shared: crate::project::source_lock::SourceLease,
}

/// A public description is not the owned capability required to bypass this backend restriction.
#[derive(Clone, Debug)]
pub struct SourceRestrictionInfo {
    pub id: NonZeroU64,
    pub source: PathBuf,
}

/// Proposed checkpoint states share the same restriction until publication or rollback finishes.
#[derive(Clone, Debug)]
pub(super) struct SourceRestriction(Arc<Restriction>);

impl SourceRestriction {
    /// Numeric IDs describe ownership but do not grant the capability carried by this lease.
    pub(super) fn id(&self) -> u64 {
        self.0.id.get()
    }
}

impl SourceProtections {
    /// Report every live owner sealing this physical file, including a descriptor shared by several modules.
    pub(super) fn describe(&self, path: &Path) -> Vec<SourceRestrictionInfo> {
        self.live()
            .filter(|entry| entry.owner == path || entry.descriptor == path)
            .map(|entry| SourceRestrictionInfo {
                id: entry.id,
                source: entry.owner.clone(),
            })
            .collect()
    }

    /// A checked source acquires shared coordination state; source permissions and native editor flags stay intact.
    pub(super) fn acquire(
        &mut self,
        root: &Path,
        source: &Path,
        owner: &Path,
        descriptor: &Path,
    ) -> Option<SourceRestriction> {
        self.owners.retain(|_, entry| entry.strong_count() != 0);
        if self.owners.len() >= 16 || self.owners.contains_key(owner) {
            return None;
        }
        let id = NonZeroU64::new(self.serial.checked_add(1)?)?;
        let shared = crate::project::source_lock::SourceLease::acquire(
            root,
            &[source.join(owner), source.join(descriptor)],
        )
        .ok()?;
        let restriction = Arc::new(Restriction {
            id,
            owner: owner.to_path_buf(),
            descriptor: descriptor.to_path_buf(),
            _shared: shared,
        });
        self.serial = id.get();
        self.owners
            .insert(owner.to_path_buf(), Arc::downgrade(&restriction));
        Some(SourceRestriction(restriction))
    }

    /// Refresh and checkpoint cloning cannot turn an ID from another registry into an owned capability.
    pub(super) fn owns(&self, restriction: &SourceRestriction) -> bool {
        self.owners
            .get(&restriction.0.owner)
            .and_then(Weak::upgrade)
            .is_some_and(|entry| Arc::ptr_eq(&entry, &restriction.0))
    }

    /// A scalar writer may touch neither a retained owner nor the descriptor sealing its identity.
    pub(super) fn blocks(&self, path: &Path, permit: Option<&SourceRestriction>) -> bool {
        if permit.is_some_and(|permit| !self.owns(permit)) {
            return true;
        }
        self.live().any(|entry| {
            !permit.is_some_and(|permit| Arc::ptr_eq(&entry, &permit.0))
                && (entry.owner == path || entry.descriptor == path)
        })
    }

    /// Moving an ancestor affects a protected source even when no text replacement names that source.
    pub(super) fn blocks_move(&self, path: &Path) -> bool {
        self.live()
            .any(|entry| entry.owner.starts_with(path) || entry.descriptor.starts_with(path))
    }

    /// Recovery with an opaque project-wide write set must wait for every source owner to release its lease.
    pub(super) fn is_active(&self) -> bool {
        self.live().next().is_some()
    }

    /// Dead weak entries are inert immediately; collection waits until the next bounded acquisition.
    fn live(&self) -> impl Iterator<Item = Arc<Restriction>> + '_ {
        self.owners.values().filter_map(Weak::upgrade)
    }
}
