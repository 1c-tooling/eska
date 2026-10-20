//! Project command presentation shared by the standalone tool and the existing ESKA CLI.

pub mod ide;
pub mod metadata;
pub mod version;

mod analyzer_config;
mod cli;
mod context;
mod operations;
mod response;

pub use cli::run;
