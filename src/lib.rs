//! Optional ESKA shell: localized parsing and delegation to independently installed tools.
//!
//! Domain models, project sessions, native execution and Git live in their owning packages.

pub mod cli;
pub(crate) mod component;
mod update;
