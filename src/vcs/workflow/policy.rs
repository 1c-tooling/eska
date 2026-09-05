//! Pure policy validation and planning. No repository access or command execution.

use gix::bstr::ByteSlice;

use super::WorkflowPreset;

const TASK_PLACEHOLDER: &str = "{task}";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkingBranchPolicy {
    TaskBranch,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SyncStrategy {
    Rebase,
    Merge,
    FastForwardOnly,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PublishBehavior {
    PushTaskBranch,
    Disabled,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FinishRequirement {
    Published,
    Integrated,
}

/// Explicit overrides. `None` inherits a field; it never resets it to a hidden default.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct PolicyOverrides {
    pub base_branch: Option<String>,
    pub working_branch: Option<WorkingBranchPolicy>,
    pub task_branch_template: Option<String>,
    pub remote: Option<String>,
    pub sync_strategy: Option<SyncStrategy>,
    pub integration_target: Option<String>,
    pub publish: Option<PublishBehavior>,
    pub finish: Option<FinishRequirement>,
    pub delete_local_branch: Option<bool>,
}

/// Complete and validated policy; construct through a preset or `PolicyOverrides::resolve`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkflowPolicy {
    base_branch: String,
    working_branch: WorkingBranchPolicy,
    task_branch_template: String,
    remote: String,
    sync_strategy: SyncStrategy,
    integration_target: String,
    publish: PublishBehavior,
    finish: FinishRequirement,
    delete_local_branch: bool,
    release_branch: Option<ReservedBranchPolicy>,
    hotfix_branch: Option<ReservedBranchPolicy>,
}

/// Naming and source reserved for a future specialized branch workflow.
#[derive(Clone, Debug, Eq, PartialEq)]
struct ReservedBranchPolicy {
    base_branch: String,
    branch_template: String,
}

impl WorkflowPreset {
    /// Return the built-in policy when the selected preset is implemented.
    #[must_use]
    pub fn policy(self) -> Option<WorkflowPolicy> {
        match self {
            Self::Trunk => Some(WorkflowPolicy {
                base_branch: "main".into(),
                working_branch: WorkingBranchPolicy::TaskBranch,
                task_branch_template: "task/{task}".into(),
                remote: "origin".into(),
                sync_strategy: SyncStrategy::Rebase,
                integration_target: "main".into(),
                publish: PublishBehavior::PushTaskBranch,
                finish: FinishRequirement::Integrated,
                delete_local_branch: true,
                release_branch: None,
                hotfix_branch: None,
            }),
            Self::GitFlow => Some(WorkflowPolicy {
                base_branch: "develop".into(),
                working_branch: WorkingBranchPolicy::TaskBranch,
                task_branch_template: "feature/{task}".into(),
                remote: "origin".into(),
                sync_strategy: SyncStrategy::Rebase,
                integration_target: "develop".into(),
                publish: PublishBehavior::PushTaskBranch,
                finish: FinishRequirement::Integrated,
                delete_local_branch: true,
                release_branch: Some(ReservedBranchPolicy {
                    base_branch: "develop".into(),
                    branch_template: "release/{task}".into(),
                }),
                hotfix_branch: Some(ReservedBranchPolicy {
                    base_branch: "main".into(),
                    branch_template: "hotfix/{task}".into(),
                }),
            }),
            Self::GithubFlow => Some(WorkflowPolicy {
                base_branch: "main".into(),
                working_branch: WorkingBranchPolicy::TaskBranch,
                task_branch_template: "feature/{task}".into(),
                remote: "origin".into(),
                sync_strategy: SyncStrategy::Rebase,
                integration_target: "main".into(),
                publish: PublishBehavior::PushTaskBranch,
                finish: FinishRequirement::Integrated,
                delete_local_branch: true,
                release_branch: None,
                hotfix_branch: None,
            }),
            Self::Custom => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PolicyField {
    BaseBranch,
    WorkingBranch,
    TaskBranchTemplate,
    Remote,
    SyncStrategy,
    IntegrationTarget,
    Publish,
    Finish,
    DeleteLocalBranch,
}

impl PolicyField {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BaseBranch => "base_branch",
            Self::WorkingBranch => "working_branch",
            Self::TaskBranchTemplate => "task_branch_template",
            Self::Remote => "remote",
            Self::SyncStrategy => "sync_strategy",
            Self::IntegrationTarget => "integration_target",
            Self::Publish => "publish",
            Self::Finish => "finish",
            Self::DeleteLocalBranch => "delete_local_branch",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolicyError {
    MissingField {
        field: PolicyField,
    },
    InvalidValue {
        field: PolicyField,
        value: String,
    },
    ExtendsRequiresCustom,
    CustomBase,
    MissingPreset {
        preset: WorkflowPreset,
    },
    PresetMismatch {
        expected: WorkflowPreset,
        actual: WorkflowPreset,
    },
    UnexpectedPreset,
    PublishRequired,
    IntegrationRequiredForDeletion,
    InvalidTask {
        task: String,
    },
    ProtectedTaskBranch {
        branch: String,
    },
}

impl PolicyOverrides {
    /// Validate explicitly provided values and contradictions independent of a preset.
    ///
    /// # Errors
    /// Returns the invalid field or an incompatible publish/finish combination.
    pub fn validate(&self) -> Result<(), PolicyError> {
        for (field, value) in [
            (PolicyField::BaseBranch, &self.base_branch),
            (PolicyField::IntegrationTarget, &self.integration_target),
        ] {
            if let Some(value) = value {
                validate_branch(value, field)?;
            }
        }
        if let Some(remote) = &self.remote {
            if remote.contains('/') || remote == "." {
                return Err(invalid(PolicyField::Remote, remote));
            }
            validate_branch(remote, PolicyField::Remote)?;
        }
        if let Some(template) = &self.task_branch_template {
            let Some((before, after)) = template.split_once("{task}") else {
                return Err(invalid(PolicyField::TaskBranchTemplate, template));
            };
            if before.contains(['{', '}']) || after.contains(['{', '}']) {
                return Err(invalid(PolicyField::TaskBranchTemplate, template));
            }
            validate_branch(
                &format!("{before}TASK-1{after}"),
                PolicyField::TaskBranchTemplate,
            )
            .map_err(|_| invalid(PolicyField::TaskBranchTemplate, template))?;
        }
        validate_finish(self.publish, self.finish, self.delete_local_branch)
    }

    /// Merge explicit fields over a base, or build a complete standalone policy.
    ///
    /// # Errors
    /// Reports missing/invalid fields or contradictions introduced by overrides.
    pub fn resolve(&self, base: Option<&WorkflowPolicy>) -> Result<WorkflowPolicy, PolicyError> {
        self.validate()?;
        let policy = WorkflowPolicy {
            base_branch: required(
                self.base_branch
                    .as_ref()
                    .or_else(|| base.map(|p| &p.base_branch)),
                PolicyField::BaseBranch,
            )?
            .clone(),
            working_branch: required(
                self.working_branch
                    .or_else(|| base.map(|p| p.working_branch)),
                PolicyField::WorkingBranch,
            )?,
            task_branch_template: required(
                self.task_branch_template
                    .as_ref()
                    .or_else(|| base.map(|p| &p.task_branch_template)),
                PolicyField::TaskBranchTemplate,
            )?
            .clone(),
            remote: required(
                self.remote.as_ref().or_else(|| base.map(|p| &p.remote)),
                PolicyField::Remote,
            )?
            .clone(),
            sync_strategy: required(
                self.sync_strategy.or_else(|| base.map(|p| p.sync_strategy)),
                PolicyField::SyncStrategy,
            )?,
            integration_target: required(
                self.integration_target
                    .as_ref()
                    .or_else(|| base.map(|p| &p.integration_target)),
                PolicyField::IntegrationTarget,
            )?
            .clone(),
            publish: required(
                self.publish.or_else(|| base.map(|p| p.publish)),
                PolicyField::Publish,
            )?,
            finish: required(
                self.finish.or_else(|| base.map(|p| p.finish)),
                PolicyField::Finish,
            )?,
            delete_local_branch: required(
                self.delete_local_branch
                    .or_else(|| base.map(|p| p.delete_local_branch)),
                PolicyField::DeleteLocalBranch,
            )?,
            release_branch: base.and_then(|policy| policy.release_branch.clone()),
            hotfix_branch: base.and_then(|policy| policy.hotfix_branch.clone()),
        };
        validate_finish(
            Some(policy.publish),
            Some(policy.finish),
            Some(policy.delete_local_branch),
        )?;
        Ok(policy)
    }
}

fn required<T>(value: Option<T>, field: PolicyField) -> Result<T, PolicyError> {
    value.ok_or(PolicyError::MissingField { field })
}

fn invalid(field: PolicyField, value: &str) -> PolicyError {
    PolicyError::InvalidValue {
        field,
        value: value.to_owned(),
    }
}

fn validate_branch(value: &str, field: PolicyField) -> Result<(), PolicyError> {
    if value.is_empty()
        || value.starts_with('-')
        || value.starts_with("refs/")
        || value.contains(['{', '}'])
        || gix::validate::reference::branch_name(format!("refs/heads/{value}").as_bytes().as_bstr())
            .is_err()
    {
        return Err(invalid(field, value));
    }
    Ok(())
}

fn validate_finish(
    publish: Option<PublishBehavior>,
    finish: Option<FinishRequirement>,
    delete: Option<bool>,
) -> Result<(), PolicyError> {
    if publish == Some(PublishBehavior::Disabled) && finish == Some(FinishRequirement::Published) {
        return Err(PolicyError::PublishRequired);
    }
    if delete == Some(true) && finish == Some(FinishRequirement::Published) {
        return Err(PolicyError::IntegrationRequiredForDeletion);
    }
    Ok(())
}

/// Declarative publication intent, never an executable command line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublishPlan {
    Disabled,
    PushTaskBranch { remote: String, branch: String },
}

/// Desired task workflow. Repository existence, cleanliness and integration need runtime preflight.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskPlan {
    pub base_branch: String,
    pub working_branch: String,
    pub sync_strategy: SyncStrategy,
    /// Fully qualified remote-tracking ref, avoiding ambiguous revision expressions.
    pub sync_reference: String,
    pub integration_target: String,
    pub publish: PublishPlan,
    pub finish: FinishRequirement,
    /// Only local cleanup after verified integration; remote deletion is never implied.
    pub delete_local_branch: bool,
}

impl WorkflowPolicy {
    /// Return the branch from which task work is based.
    #[must_use]
    pub fn base_branch(&self) -> &str {
        &self.base_branch
    }

    /// Return the remote used to synchronize the base branch.
    #[must_use]
    pub fn remote(&self) -> &str {
        &self.remote
    }

    /// Return the fully qualified remote-tracking reference for the base branch.
    #[must_use]
    pub fn remote_base_reference(&self) -> String {
        format!("refs/remotes/{}/{}", self.remote, self.base_branch)
    }

    /// Extract a task ID only when the branch exactly matches this policy's task template.
    #[must_use]
    pub fn task_id<'a>(&self, branch: &'a str) -> Option<&'a str> {
        let (prefix, suffix) = self.task_branch_template.split_once(TASK_PLACEHOLDER)?;
        let task = branch.strip_prefix(prefix)?.strip_suffix(suffix)?;
        if task.is_empty() || task.contains('/') {
            return None;
        }
        self.plan(task)
            .ok()
            .filter(|plan| plan.working_branch == branch)
            .map(|_| task)
    }

    /// Resolve task naming and targets with no filesystem, clock, locale or Git mutations.
    /// Task IDs are literal single path components; no implicit slugification is performed.
    ///
    /// # Errors
    /// Rejects unsafe task IDs, invalid generated names, and collisions with protected branches.
    pub fn plan(&self, task_id: &str) -> Result<TaskPlan, PolicyError> {
        if task_id.contains('/')
            || validate_branch(task_id, PolicyField::TaskBranchTemplate).is_err()
        {
            return Err(PolicyError::InvalidTask {
                task: task_id.to_owned(),
            });
        }
        let working_branch = self.task_branch_template.replace("{task}", task_id);
        validate_branch(&working_branch, PolicyField::TaskBranchTemplate)?;
        if working_branch == self.base_branch || working_branch == self.integration_target {
            return Err(PolicyError::ProtectedTaskBranch {
                branch: working_branch,
            });
        }
        let publish = match self.publish {
            PublishBehavior::Disabled => PublishPlan::Disabled,
            PublishBehavior::PushTaskBranch => PublishPlan::PushTaskBranch {
                remote: self.remote.clone(),
                branch: working_branch.clone(),
            },
        };
        Ok(TaskPlan {
            base_branch: self.base_branch.clone(),
            working_branch,
            sync_strategy: self.sync_strategy,
            sync_reference: format!("refs/remotes/{}/{}", self.remote, self.base_branch),
            integration_target: self.integration_target.clone(),
            publish,
            finish: self.finish,
            delete_local_branch: self.delete_local_branch,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vcs::workflow::WorkflowSettings;

    fn complete() -> PolicyOverrides {
        PolicyOverrides {
            base_branch: Some("baseline".into()),
            working_branch: Some(WorkingBranchPolicy::TaskBranch),
            task_branch_template: Some("task/{task}".into()),
            remote: Some("team".into()),
            sync_strategy: Some(SyncStrategy::Rebase),
            integration_target: Some("integration".into()),
            publish: Some(PublishBehavior::PushTaskBranch),
            finish: Some(FinishRequirement::Integrated),
            delete_local_branch: Some(true),
        }
    }

    #[test]
    fn planning_resolves_all_targets_without_mutating_the_base() {
        let base = complete().resolve(None).unwrap();
        let original = base.clone();
        let plan = base.plan("FI-1234").unwrap();
        assert_eq!(
            plan,
            TaskPlan {
                base_branch: "baseline".into(),
                working_branch: "task/FI-1234".into(),
                sync_strategy: SyncStrategy::Rebase,
                sync_reference: "refs/remotes/team/baseline".into(),
                integration_target: "integration".into(),
                publish: PublishPlan::PushTaskBranch {
                    remote: "team".into(),
                    branch: "task/FI-1234".into()
                },
                finish: FinishRequirement::Integrated,
                delete_local_branch: true,
            }
        );
        assert_eq!(base.plan("FI-1234").unwrap(), plan);
        assert_eq!(base, original);
    }

    #[test]
    fn task_id_requires_an_exact_task_branch_match() {
        let policy = WorkflowPreset::GitFlow.policy().unwrap();

        assert_eq!(policy.task_id("feature/FI-1234"), Some("FI-1234"));
        assert_eq!(policy.task_id("develop"), None);
        assert_eq!(policy.task_id("feature/team/FI-1234"), None);
        assert_eq!(policy.task_id("feature/"), None);
        assert_eq!(policy.base_branch(), "develop");
        assert_eq!(
            policy.remote_base_reference(),
            "refs/remotes/origin/develop"
        );
    }

    #[test]
    fn overrides_merge_fields_and_preserve_explicit_false() {
        let base = complete().resolve(None).unwrap();
        for strategy in [
            SyncStrategy::Rebase,
            SyncStrategy::Merge,
            SyncStrategy::FastForwardOnly,
        ] {
            let overrides = PolicyOverrides {
                base_branch: Some("next".into()),
                remote: Some("upstream".into()),
                task_branch_template: Some("feature/{task}".into()),
                sync_strategy: Some(strategy),
                delete_local_branch: Some(false),
                ..PolicyOverrides::default()
            };
            let custom = WorkflowSettings::new(
                WorkflowPreset::Custom,
                Some(WorkflowPreset::Trunk),
                overrides,
            )
            .unwrap();
            let plan = custom
                .resolve(Some((WorkflowPreset::Trunk, &base)))
                .unwrap()
                .plan("ЗАДАЧА-1")
                .unwrap();
            assert_eq!(plan.base_branch, "next");
            assert_eq!(plan.working_branch, "feature/ЗАДАЧА-1");
            assert_eq!(plan.integration_target, "integration");
            assert_eq!(plan.sync_reference, "refs/remotes/upstream/next");
            assert_eq!(plan.sync_strategy, strategy);
            assert!(!plan.delete_local_branch);
            assert_eq!(
                plan.publish,
                PublishPlan::PushTaskBranch {
                    remote: "upstream".into(),
                    branch: "feature/ЗАДАЧА-1".into()
                }
            );
        }
    }

    #[test]
    fn complete_custom_policy_needs_no_preset_and_can_disable_publication() {
        let mut fields = complete();
        fields.publish = Some(PublishBehavior::Disabled);
        let custom = WorkflowSettings::new(WorkflowPreset::Custom, None, fields).unwrap();
        assert_eq!(
            custom
                .resolve(None)
                .unwrap()
                .plan("TASK-1")
                .unwrap()
                .publish,
            PublishPlan::Disabled
        );
        assert_eq!(
            WorkflowSettings::selection(WorkflowPreset::Custom).resolve(None),
            Err(PolicyError::MissingField {
                field: PolicyField::BaseBranch
            })
        );
    }

    #[test]
    fn trunk_selection_uses_builtin_policy_and_accepts_matching_explicit_base() {
        let base = complete().resolve(None).unwrap();
        let settings = WorkflowSettings::selection(WorkflowPreset::Trunk);
        assert_eq!(
            settings.resolve(None).unwrap(),
            WorkflowPreset::Trunk.policy().unwrap()
        );
        assert_eq!(
            settings
                .resolve(Some((WorkflowPreset::Trunk, &base)))
                .unwrap(),
            base
        );
    }

    #[test]
    fn all_named_presets_have_builtin_policies() {
        for preset in [
            WorkflowPreset::Trunk,
            WorkflowPreset::GitFlow,
            WorkflowPreset::GithubFlow,
        ] {
            assert_eq!(
                WorkflowSettings::selection(preset).resolve(None).unwrap(),
                preset.policy().unwrap()
            );
        }
    }

    #[test]
    fn explicit_base_must_match_the_selected_preset() {
        let base = complete().resolve(None).unwrap();
        let settings = WorkflowSettings::selection(WorkflowPreset::Trunk);
        assert!(matches!(
            settings.resolve(Some((WorkflowPreset::GitFlow, &base))),
            Err(PolicyError::PresetMismatch { .. })
        ));
    }

    #[test]
    fn named_presets_accept_overrides_but_only_custom_can_extend() {
        let settings = WorkflowSettings::new(
            WorkflowPreset::Trunk,
            None,
            PolicyOverrides {
                base_branch: Some("master".into()),
                integration_target: Some("master".into()),
                ..PolicyOverrides::default()
            },
        )
        .unwrap();
        let policy = settings.resolve(None).unwrap();
        assert_eq!(policy.base_branch(), "master");
        assert!(matches!(
            WorkflowSettings::new(
                WorkflowPreset::Trunk,
                Some(WorkflowPreset::GitFlow),
                PolicyOverrides::default()
            ),
            Err(PolicyError::ExtendsRequiresCustom)
        ));
        assert!(matches!(
            WorkflowSettings::new(
                WorkflowPreset::Custom,
                Some(WorkflowPreset::Custom),
                PolicyOverrides::default()
            ),
            Err(PolicyError::CustomBase)
        ));
    }

    #[test]
    fn trunk_plan_is_short_lived_rebased_published_and_integrated_into_main() {
        let plan = WorkflowPreset::Trunk
            .policy()
            .unwrap()
            .plan("FI-1234")
            .unwrap();
        assert_eq!(
            plan,
            TaskPlan {
                base_branch: "main".into(),
                working_branch: "task/FI-1234".into(),
                sync_strategy: SyncStrategy::Rebase,
                sync_reference: "refs/remotes/origin/main".into(),
                integration_target: "main".into(),
                publish: PublishPlan::PushTaskBranch {
                    remote: "origin".into(),
                    branch: "task/FI-1234".into(),
                },
                finish: FinishRequirement::Integrated,
                delete_local_branch: true,
            }
        );
    }

    #[test]
    fn custom_trunk_overrides_inherit_unspecified_builtin_fields() {
        for (template, remote, delete_local_branch) in [
            ("company/{task}", "team", false),
            ("issue-{task}", "upstream", true),
        ] {
            let settings = WorkflowSettings::new(
                WorkflowPreset::Custom,
                Some(WorkflowPreset::Trunk),
                PolicyOverrides {
                    task_branch_template: Some(template.into()),
                    remote: Some(remote.into()),
                    delete_local_branch: Some(delete_local_branch),
                    ..PolicyOverrides::default()
                },
            )
            .unwrap();
            let plan = settings.resolve(None).unwrap().plan("FI-9").unwrap();
            assert_eq!(plan.base_branch, "main");
            assert_eq!(plan.working_branch, template.replace("{task}", "FI-9"));
            assert_eq!(plan.sync_strategy, SyncStrategy::Rebase);
            assert_eq!(plan.sync_reference, format!("refs/remotes/{remote}/main"));
            assert_eq!(plan.integration_target, "main");
            assert_eq!(
                plan.publish,
                PublishPlan::PushTaskBranch {
                    remote: remote.into(),
                    branch: template.replace("{task}", "FI-9"),
                }
            );
            assert_eq!(plan.finish, FinishRequirement::Integrated);
            assert_eq!(plan.delete_local_branch, delete_local_branch);
        }
    }

    #[test]
    fn git_flow_plan_uses_feature_branch_from_develop() {
        let plan = WorkflowPreset::GitFlow
            .policy()
            .unwrap()
            .plan("FI-1234")
            .unwrap();
        assert_eq!(
            plan,
            TaskPlan {
                base_branch: "develop".into(),
                working_branch: "feature/FI-1234".into(),
                sync_strategy: SyncStrategy::Rebase,
                sync_reference: "refs/remotes/origin/develop".into(),
                integration_target: "develop".into(),
                publish: PublishPlan::PushTaskBranch {
                    remote: "origin".into(),
                    branch: "feature/FI-1234".into(),
                },
                finish: FinishRequirement::Integrated,
                delete_local_branch: true,
            }
        );
    }

    #[test]
    fn git_flow_reserves_release_and_hotfix_branch_policies_without_planning_them() {
        let policy = WorkflowPreset::GitFlow.policy().unwrap();
        assert_eq!(
            policy.release_branch,
            Some(ReservedBranchPolicy {
                base_branch: "develop".into(),
                branch_template: "release/{task}".into(),
            })
        );
        assert_eq!(
            policy.hotfix_branch,
            Some(ReservedBranchPolicy {
                base_branch: "main".into(),
                branch_template: "hotfix/{task}".into(),
            })
        );
    }

    #[test]
    fn custom_git_flow_overrides_keep_reserved_branch_policies() {
        let settings = WorkflowSettings::new(
            WorkflowPreset::Custom,
            Some(WorkflowPreset::GitFlow),
            PolicyOverrides {
                remote: Some("team".into()),
                delete_local_branch: Some(false),
                ..PolicyOverrides::default()
            },
        )
        .unwrap();
        let policy = settings.resolve(None).unwrap();
        assert_eq!(
            policy.release_branch,
            WorkflowPreset::GitFlow.policy().unwrap().release_branch
        );
        assert_eq!(
            policy.hotfix_branch,
            WorkflowPreset::GitFlow.policy().unwrap().hotfix_branch
        );
        let plan = policy.plan("FI-10").unwrap();
        assert_eq!(plan.sync_reference, "refs/remotes/team/develop");
        assert!(!plan.delete_local_branch);
    }

    #[test]
    fn github_flow_plan_uses_published_feature_branch_integrated_into_main() {
        let plan = WorkflowPreset::GithubFlow
            .policy()
            .unwrap()
            .plan("FI-1234")
            .unwrap();
        assert_eq!(
            plan,
            TaskPlan {
                base_branch: "main".into(),
                working_branch: "feature/FI-1234".into(),
                sync_strategy: SyncStrategy::Rebase,
                sync_reference: "refs/remotes/origin/main".into(),
                integration_target: "main".into(),
                publish: PublishPlan::PushTaskBranch {
                    remote: "origin".into(),
                    branch: "feature/FI-1234".into(),
                },
                finish: FinishRequirement::Integrated,
                delete_local_branch: true,
            }
        );
    }

    #[test]
    fn invalid_names_templates_and_task_ids_are_rejected() {
        for value in [
            "",
            "-option",
            "HEAD",
            "refs/heads/main",
            "a..b",
            "a.lock",
            "a/.hidden",
            "a//b",
            "a b",
            "a\nb",
            "a@{1}",
            "a\\b",
        ] {
            let fields = PolicyOverrides {
                base_branch: Some(value.into()),
                ..PolicyOverrides::default()
            };
            assert!(
                matches!(
                    fields.validate(),
                    Err(PolicyError::InvalidValue {
                        field: PolicyField::BaseBranch,
                        ..
                    })
                ),
                "{value:?}"
            );
        }
        for template in [
            "fixed",
            "{task}/{task}",
            "{unknown}/{task}",
            "{task}.lock",
            "../{task}",
            "-{task}",
        ] {
            assert!(
                PolicyOverrides {
                    task_branch_template: Some(template.into()),
                    ..PolicyOverrides::default()
                }
                .validate()
                .is_err(),
                "{template}"
            );
        }
        let policy = complete().resolve(None).unwrap();
        for task_id in [
            "", "../main", "a/b", "--force", "a b", "{task}", "HEAD", "a\nb",
        ] {
            assert!(
                matches!(policy.plan(task_id), Err(PolicyError::InvalidTask { .. })),
                "{task_id:?}"
            );
        }
    }

    #[test]
    fn merged_policy_validates_cross_field_constraints_and_protected_names() {
        let base = complete().resolve(None).unwrap();
        let overrides = PolicyOverrides {
            finish: Some(FinishRequirement::Published),
            ..PolicyOverrides::default()
        };
        assert_eq!(
            overrides.resolve(Some(&base)),
            Err(PolicyError::IntegrationRequiredForDeletion)
        );
        let overrides = PolicyOverrides {
            publish: Some(PublishBehavior::Disabled),
            finish: Some(FinishRequirement::Published),
            delete_local_branch: Some(false),
            ..PolicyOverrides::default()
        };
        assert_eq!(
            overrides.resolve(Some(&base)),
            Err(PolicyError::PublishRequired)
        );
        let policy = PolicyOverrides {
            task_branch_template: Some("{task}".into()),
            ..PolicyOverrides::default()
        }
        .resolve(Some(&base))
        .unwrap();
        for name in ["baseline", "integration"] {
            assert_eq!(
                policy.plan(name),
                Err(PolicyError::ProtectedTaskBranch {
                    branch: name.into()
                })
            );
        }
    }
}
