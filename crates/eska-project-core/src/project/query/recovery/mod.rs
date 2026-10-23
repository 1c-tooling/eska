//! Private query checkpoints are shared by IDE adapters and never authorize a source write.

mod prepared;
mod record;
mod store;

pub use prepared::Prepared;
pub use record::{Part, Record, is_id};
pub use store::{Contents, Store, Stored};

/// Logical text storage is independent of JSON/base64 expansion and transport framing.
pub const MAX_RETAINED_BYTES: usize = 32 * 1024 * 1024;
/// Each owner has at most one retained query draft in an adapter's private store.
pub const MAX_RECORDS: usize = 16;
/// Binding metadata cannot consume the text quota through unbounded opaque fields.
pub const MAX_METADATA_BYTES: usize = 1024 * 1024;
/// Base64 keeps even unfinished control-character drafts within this independent file limit.
pub const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;

/// Storage failures carry machine categories; UI wording belongs to the adapter.
#[derive(Debug)]
pub enum Error {
    InvalidRecord,
    ResourceLimit,
    UnsafePath,
    Conflict,
    Busy,
    Io(std::io::Error),
}

impl From<std::io::Error> for Error {
    /// Preserve the original IO category without source text or localized messages.
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
