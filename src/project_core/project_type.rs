//! Native artifact kinds shared without VCS or platform process dependencies.

/// The kind of 1C artifact represented by a project.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProjectType {
    Configuration,
    Extension,
    Processing,
    Report,
}

impl ProjectType {
    /// Return the stable machine-facing project type name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::Extension => "extension",
            Self::Processing => "processing",
            Self::Report => "report",
        }
    }
}
