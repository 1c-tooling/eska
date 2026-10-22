//! Workflow selection, validated policies and deterministic task planning.

mod policy;
pub use policy::{
    FinishRequirement, PolicyError, PolicyField, PolicyOverrides, PublishBehavior, PublishPlan,
    SyncStrategy, TaskPlan, WorkflowPolicy, WorkingBranchPolicy,
};

/// A saved workflow selection, not an executable workflow policy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkflowPreset {
    Trunk,
    GitFlow,
    GithubFlow,
    Custom,
}

/// A validated selection and its explicit overrides.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct WorkflowSettings {
    preset: WorkflowPreset,
    extends: Option<WorkflowPreset>,
    policy: PolicyOverrides,
}

impl WorkflowSettings {
    /// Preserve the compact selection produced by `new`/`init`, including unconfigured `custom`.
    #[must_use]
    pub fn selection(preset: WorkflowPreset) -> Self {
        Self {
            preset,
            extends: None,
            policy: PolicyOverrides::default(),
        }
    }

    /// Validate overrides without requiring builtin preset defaults to be available.
    ///
    /// # Errors
    /// Rejects inheritance on a named preset, recursive inheritance and invalid policy fields.
    /// A standalone custom policy must specify all fields if any are present.
    pub fn new(
        preset: WorkflowPreset,
        extends: Option<WorkflowPreset>,
        policy: PolicyOverrides,
    ) -> Result<Self, PolicyError> {
        if preset != WorkflowPreset::Custom && extends.is_some() {
            return Err(PolicyError::ExtendsRequiresCustom);
        }
        if extends == Some(WorkflowPreset::Custom) {
            return Err(PolicyError::CustomBase);
        }
        policy.validate()?;
        if preset == WorkflowPreset::Custom
            && extends.is_none()
            && policy != PolicyOverrides::default()
        {
            policy.resolve(None)?;
        }
        Ok(Self {
            preset,
            extends,
            policy,
        })
    }

    #[must_use]
    pub const fn preset(&self) -> WorkflowPreset {
        self.preset
    }

    #[must_use]
    pub const fn extends(&self) -> Option<WorkflowPreset> {
        self.extends
    }

    #[must_use]
    pub const fn policy(&self) -> &PolicyOverrides {
        &self.policy
    }

    #[must_use]
    pub const fn base_preset(&self) -> Option<WorkflowPreset> {
        match self.preset {
            WorkflowPreset::Custom => self.extends,
            preset => Some(preset),
        }
    }

    /// Apply overrides to a matching supplied policy or an available built-in preset.
    /// Pass `None` to use built-in defaults or a fully specified standalone custom policy.
    ///
    /// # Errors
    /// Reports a missing/mismatched base or an incomplete/inconsistent resolved policy.
    pub fn resolve(
        &self,
        base: Option<(WorkflowPreset, &WorkflowPolicy)>,
    ) -> Result<WorkflowPolicy, PolicyError> {
        match (self.base_preset(), base) {
            (Some(expected), Some((actual, policy))) if expected == actual => {
                self.policy.resolve(Some(policy))
            }
            (Some(expected), Some((actual, _))) => {
                Err(PolicyError::PresetMismatch { expected, actual })
            }
            (Some(preset), None) => preset
                .policy()
                .map_or(Err(PolicyError::MissingPreset { preset }), |policy| {
                    self.policy.resolve(Some(&policy))
                }),
            (None, Some(_)) => Err(PolicyError::UnexpectedPreset),
            (None, None) => self.policy.resolve(None),
        }
    }
}

impl WorkflowPreset {
    /// Parse the stable machine-facing preset name.
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

    /// Return the stable machine-facing preset name.
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
