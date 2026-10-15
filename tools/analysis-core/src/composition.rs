//! Root-qualified canonical inputs share one topology and one kernel file-ID universe.

#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
#[path = "composition/admission.rs"]
mod admission;
#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
pub(crate) mod cli;
#[path = "composition/identities.rs"]
mod identities;

#[cfg(any(feature = "analyzer-cli", feature = "check-cli"))]
pub use admission::{AdmissionError, AdmittedComposition, AdmittedRoot, admit};
pub use identities::{FileIdAllocator, IdentityError, RootId, SourceKey};

#[cfg(all(test, any(feature = "analyzer-cli", feature = "check-cli")))]
#[path = "composition/tests.rs"]
mod tests;
