//! Session-local roots qualify authoring paths without changing their source-relative spelling.

use eska::project_core::source_format::SourcePath;
use vfs::FileId;

/// A root index is meaningful only inside the session that admitted its topology.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize)]
#[serde(transparent)]
pub struct RootId(u32);

/// Stable file keys distinguish identical relative paths in different canonical roots.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize)]
pub struct SourceKey {
    pub root: RootId,
    pub source: SourcePath,
}

/// Identity exhaustion refuses admission instead of aliasing an existing tracked input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentityError {
    TooManyRoots,
    FileIdExhausted,
}

impl RootId {
    /// Checked conversion preserves the same index used by the workspace snapshot.
    ///
    /// # Errors
    /// Returns `TooManyRoots` when the source-root ID range is exhausted.
    pub fn from_index(index: usize) -> Result<Self, IdentityError> {
        u32::try_from(index)
            .map(Self)
            .map_err(|_| IdentityError::TooManyRoots)
    }

    /// The admitted order connects this identity to the corresponding source root.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// Every owner, nested form and module consumes the same monotonically increasing allocator.
#[derive(Debug)]
pub struct FileIdAllocator {
    next: Option<u32>,
}

impl Default for FileIdAllocator {
    /// Zero is the first file identity; all subsequent roots continue the same sequence.
    fn default() -> Self {
        Self { next: Some(0) }
    }
}

impl FileIdAllocator {
    /// Allocate exactly one identity, including the final representable `FileId`.
    ///
    /// # Errors
    /// Returns `FileIdExhausted` after the final identity, without restarting the sequence.
    pub fn allocate(&mut self) -> Result<FileId, IdentityError> {
        let value = self.next.ok_or(IdentityError::FileIdExhausted)?;
        self.next = value.checked_add(1);
        Ok(FileId(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exhaustion never discards the last valid identity or wraps to an existing input.
    #[test]
    fn file_id_exhaustion_preserves_the_final_identity() {
        let mut allocator = FileIdAllocator {
            next: Some(u32::MAX),
        };
        assert_eq!(allocator.allocate(), Ok(FileId(u32::MAX)));
        assert_eq!(allocator.allocate(), Err(IdentityError::FileIdExhausted));
        assert_eq!(allocator.allocate(), Err(IdentityError::FileIdExhausted));
    }
}
