//! Stable patch plan data shared by planning, execution and CLI presentation.

use crate::project::build::PlatformVersion;
use gix::ObjectId;
use serde::Serialize;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug)]
pub struct PatchError {
    pub code: &'static str,
    pub detail: String,
}

impl PatchError {
    /// Keep machine-facing classification separate from localized presentation.
    pub(super) fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PatchPlan {
    pub base: String,
    pub base_commit: String,
    pub merge_base: String,
    pub head: String,
    pub name: String,
    pub modules: Vec<PatchModule>,
    pub ignored_paths: Vec<String>,
    pub changes: Vec<PatchChange>,
    #[serde(skip)]
    pub(super) repository_root: PathBuf,
    #[serde(skip)]
    pub(super) source_files: BTreeMap<String, ObjectId>,
    #[serde(skip)]
    pub platform_version: Option<PlatformVersion>,
}

#[derive(Debug, Serialize)]
pub struct PatchModule {
    pub name: String,
    pub methods: Vec<String>,
    #[serde(skip)]
    pub(super) descriptor: String,
    #[serde(skip)]
    pub(super) code: String,
}

#[derive(Debug, Serialize)]
pub struct PatchChange {
    pub path: String,
    pub decision: &'static str,
    pub reason: &'static str,
}
