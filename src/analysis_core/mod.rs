//! Revisioned analysis inputs shared by resident and batch consumers, without an IDE runtime.

mod metadata_inputs;

pub use metadata_inputs::{
    Cancellation, InputFailure, InputOrigin, InputRequest, InputRevision, InputState, InputSummary,
    MetadataInputs, OwnerInput, SourceBudget, UpdateError,
};
