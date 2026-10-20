//! Immutable repository objects and prepared module bodies belong only to the VCS owner.

use crate::project::build::PlatformVersion;
pub use eska_project_core::patch::{PatchChange, PatchDescription, PatchError, PatchModule};
use gix::ObjectId;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug)]
pub struct PatchPlan {
    pub description: PatchDescription,
    pub platform_version: Option<PlatformVersion>,
    pub(super) repository_root: PathBuf,
    pub(super) source_files: BTreeMap<String, ObjectId>,
    pub(super) modules: Vec<ModuleProjection>,
}

#[derive(Debug)]
pub(super) struct ModuleProjection {
    pub name: String,
    pub methods: Vec<String>,
    pub descriptor: String,
    pub code: String,
}
