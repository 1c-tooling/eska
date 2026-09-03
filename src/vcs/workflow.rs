//! Machine-facing workflow preset names stored in project configuration.

/// A saved workflow selection, not an executable workflow policy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkflowPreset {
    Trunk,
    GitFlow,
    GithubFlow,
    Custom,
}

impl WorkflowPreset {
    #[must_use]
    pub fn from_name(value: &str) -> Option<Self> {
        match value {
            "trunk" => Some(Self::Trunk),
            "git-flow" => Some(Self::GitFlow),
            "github-flow" => Some(Self::GithubFlow),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trunk => "trunk",
            Self::GitFlow => "git-flow",
            Self::GithubFlow => "github-flow",
            Self::Custom => "custom",
        }
    }
}
