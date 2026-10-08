//! Caller-supplied repository facts; native builds do not inspect Git.

/// Repository facts recorded before temporary build paths affect its status.
///
/// A caller that does not inspect Git must use `Unavailable`, not `Absent`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GitProvenance {
    Available { commit: Option<String>, dirty: bool },
    Absent,
    Unavailable,
}

/// Optional project identity and Git facts supplied by the invoking tool.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildProvenance {
    pub(super) project_name: Option<String>,
    pub(super) git: GitProvenance,
}

impl BuildProvenance {
    /// Capture caller-provided facts without reading a project or repository.
    #[must_use]
    pub fn new(project_name: Option<&str>, git: GitProvenance) -> Self {
        Self {
            project_name: project_name.map(str::to_owned),
            git,
        }
    }
}
