//! Conversion between TOML workflow fields and validated domain settings.

use super::{
    ProjectConfigError,
    schema::{RawWorkflow, SerializedPolicy, SerializedWorkflow},
};
use crate::vcs::workflow::{
    FinishRequirement, PolicyError, PolicyField, PolicyOverrides, PublishBehavior, SyncStrategy,
    WorkflowPreset, WorkflowSettings, WorkingBranchPolicy,
};

pub(super) fn parse(raw: RawWorkflow) -> Result<WorkflowSettings, ProjectConfigError> {
    let preset = parse_preset(raw.preset)?;
    let extends = raw.extends.map(parse_preset).transpose()?;
    let fields = raw.policy.unwrap_or_default();
    let policy = PolicyOverrides {
        base_branch: fields.base_branch,
        working_branch: parse_choice(fields.working_branch, PolicyField::WorkingBranch, |value| {
            match value {
                "task-branch" => Some(WorkingBranchPolicy::TaskBranch),
                _ => None,
            }
        })?,
        task_branch_template: fields.task_branch_template,
        remote: fields.remote,
        sync_strategy: parse_choice(fields.sync_strategy, PolicyField::SyncStrategy, |value| {
            match value {
                "rebase" => Some(SyncStrategy::Rebase),
                "merge" => Some(SyncStrategy::Merge),
                "fast-forward-only" => Some(SyncStrategy::FastForwardOnly),
                _ => None,
            }
        })?,
        integration_target: fields.integration_target,
        publish: parse_choice(fields.publish, PolicyField::Publish, |value| match value {
            "push-task-branch" => Some(PublishBehavior::PushTaskBranch),
            "disabled" => Some(PublishBehavior::Disabled),
            _ => None,
        })?,
        finish: parse_choice(fields.finish, PolicyField::Finish, |value| match value {
            "require-published" => Some(FinishRequirement::Published),
            "require-integrated" => Some(FinishRequirement::Integrated),
            _ => None,
        })?,
        delete_local_branch: fields.delete_local_branch,
    };
    WorkflowSettings::new(preset, extends, policy).map_err(ProjectConfigError::InvalidWorkflow)
}

fn parse_preset(value: String) -> Result<WorkflowPreset, ProjectConfigError> {
    WorkflowPreset::from_name(&value).ok_or(ProjectConfigError::UnknownWorkflow { value })
}

fn parse_choice<T>(
    value: Option<String>,
    field: PolicyField,
    parse: impl FnOnce(&str) -> Option<T>,
) -> Result<Option<T>, ProjectConfigError> {
    value
        .map(|value| {
            parse(&value).ok_or(ProjectConfigError::InvalidWorkflow(
                PolicyError::InvalidValue { field, value },
            ))
        })
        .transpose()
}

pub(super) fn serialize(settings: &WorkflowSettings) -> SerializedWorkflow<'_> {
    let fields = settings.policy();
    SerializedWorkflow {
        preset: settings.preset().as_str(),
        extends: settings.extends().map(WorkflowPreset::as_str),
        policy: (fields != &PolicyOverrides::default()).then(|| SerializedPolicy {
            base_branch: fields.base_branch.as_deref(),
            working_branch: fields.working_branch.map(|value| match value {
                WorkingBranchPolicy::TaskBranch => "task-branch",
            }),
            task_branch_template: fields.task_branch_template.as_deref(),
            remote: fields.remote.as_deref(),
            sync_strategy: fields.sync_strategy.map(|value| match value {
                SyncStrategy::Rebase => "rebase",
                SyncStrategy::Merge => "merge",
                SyncStrategy::FastForwardOnly => "fast-forward-only",
            }),
            integration_target: fields.integration_target.as_deref(),
            publish: fields.publish.map(|value| match value {
                PublishBehavior::PushTaskBranch => "push-task-branch",
                PublishBehavior::Disabled => "disabled",
            }),
            finish: fields.finish.map(|value| match value {
                FinishRequirement::Published => "require-published",
                FinishRequirement::Integrated => "require-integrated",
            }),
            delete_local_branch: fields.delete_local_branch,
        }),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        config::{ProjectConfig, ProjectConfigError},
        vcs::workflow::{PolicyError, PolicyField, PublishPlan, SyncStrategy},
    };

    const FULL: &str = r#"
[project]
type = "configuration"
[vcs.workflow]
preset = "custom"
[vcs.workflow.policy]
base_branch = "main"
working_branch = "task-branch"
task_branch_template = "feature/{task}"
remote = "origin"
sync_strategy = "rebase"
integration_target = "main"
publish = "push-task-branch"
finish = "require-integrated"
delete_local_branch = true
"#;

    #[test]
    fn complete_policy_round_trips_into_project_and_builds_the_same_plan() {
        let config = ProjectConfig::from_toml(FULL).unwrap();
        let settings = config.configuration().workflow_settings().unwrap();
        let plan = settings.resolve(None).unwrap().plan("TASK-8").unwrap();
        let canonical = config.to_toml().unwrap();
        let parsed = ProjectConfig::from_toml(&canonical).unwrap();
        assert_eq!(parsed, config);
        assert_eq!(parsed.to_toml().unwrap(), canonical);
        let root = std::env::current_dir().unwrap().join("example");
        let project = parsed.into_project(root).unwrap();
        assert_eq!(
            project
                .configuration()
                .workflow_settings()
                .unwrap()
                .resolve(None)
                .unwrap()
                .plan("TASK-8")
                .unwrap(),
            plan
        );
        assert_eq!(plan.sync_reference, "refs/remotes/origin/main");
        assert_eq!(plan.working_branch, "feature/TASK-8");
    }

    #[test]
    fn explicit_overrides_are_preserved_without_materializing_defaults() {
        let input = "[project]\ntype = 'report'\n[vcs.workflow]\npreset = 'custom'\nextends = 'git-flow'\n[vcs.workflow.policy]\nbase_branch = 'next'\ndelete_local_branch = false\n";
        let config = ProjectConfig::from_toml(input).unwrap();
        let canonical = config.to_toml().unwrap();
        assert!(canonical.contains("extends = \"git-flow\""));
        assert!(canonical.contains("delete_local_branch = false"));
        assert!(!canonical.contains("sync_strategy"));
        assert_eq!(ProjectConfig::from_toml(&canonical).unwrap(), config);
    }

    #[test]
    fn named_preset_branch_overrides_round_trip_without_custom_inheritance() {
        let input = "[project]\ntype = 'report'\n[vcs.workflow]\npreset = 'trunk'\n[vcs.workflow.policy]\nbase_branch = 'master'\ntask_branch_template = 'feature/{task}'\nintegration_target = 'master'\n";
        let config = ProjectConfig::from_toml(input).unwrap();
        let policy = config
            .configuration()
            .workflow_settings()
            .unwrap()
            .resolve(None)
            .unwrap();
        let plan = policy.plan("FI-9").unwrap();

        assert_eq!(plan.base_branch, "master");
        assert_eq!(plan.working_branch, "feature/FI-9");
        assert_eq!(plan.integration_target, "master");
        assert_eq!(plan.sync_reference, "refs/remotes/origin/master");
        assert_eq!(
            ProjectConfig::from_toml(&config.to_toml().unwrap()).unwrap(),
            config
        );
    }

    #[test]
    fn machine_values_round_trip_for_all_supported_strategies_and_behaviors() {
        for (value, strategy) in [
            ("rebase", SyncStrategy::Rebase),
            ("merge", SyncStrategy::Merge),
            ("fast-forward-only", SyncStrategy::FastForwardOnly),
        ] {
            for (publish, finish) in [
                ("push-task-branch", "require-integrated"),
                ("push-task-branch", "require-published"),
                ("disabled", "require-integrated"),
            ] {
                let text = FULL
                    .replace(
                        "sync_strategy = \"rebase\"",
                        &format!("sync_strategy = \"{value}\""),
                    )
                    .replace(
                        "publish = \"push-task-branch\"",
                        &format!("publish = \"{publish}\""),
                    )
                    .replace(
                        "finish = \"require-integrated\"",
                        &format!("finish = \"{finish}\""),
                    )
                    .replace("delete_local_branch = true", "delete_local_branch = false");
                let config = ProjectConfig::from_toml(&text).unwrap();
                assert_eq!(
                    ProjectConfig::from_toml(&config.to_toml().unwrap()).unwrap(),
                    config
                );
                let plan = config
                    .configuration()
                    .workflow_settings()
                    .unwrap()
                    .resolve(None)
                    .unwrap()
                    .plan("T-1")
                    .unwrap();
                assert_eq!(plan.sync_strategy, strategy);
                assert_eq!(plan.publish == PublishPlan::Disabled, publish == "disabled");
            }
        }
    }

    #[test]
    fn unknown_or_incomplete_policy_is_rejected_with_structured_errors() {
        let base = "[project]\ntype = 'report'\n[vcs.workflow]\npreset = 'custom'\nextends = 'trunk'\n[vcs.workflow.policy]\n";
        for (field, value, expected) in [
            ("working_branch", "direct-trunk", PolicyField::WorkingBranch),
            ("sync_strategy", "reset-hard", PolicyField::SyncStrategy),
            ("publish", "force-push", PolicyField::Publish),
            ("finish", "delete-remote", PolicyField::Finish),
            ("base_branch", "../main", PolicyField::BaseBranch),
            ("integration_target", "HEAD", PolicyField::IntegrationTarget),
            (
                "remote",
                "https://example.invalid/repo",
                PolicyField::Remote,
            ),
            (
                "task_branch_template",
                "{other}",
                PolicyField::TaskBranchTemplate,
            ),
        ] {
            let result = ProjectConfig::from_toml(&format!("{base}{field} = '{value}'\n"));
            assert!(
                matches!(result, Err(ProjectConfigError::InvalidWorkflow(PolicyError::InvalidValue { field, .. })) if field == expected),
                "{result:?}"
            );
        }
        for text in [
            "delete_remote_branch = true",
            "delete_local_branch = 'yes'",
            "base_branch = 42",
            "[vcs.workflow.policy.extra]\nvalue = 1",
        ] {
            assert!(matches!(
                ProjectConfig::from_toml(&format!("{base}{text}")),
                Err(ProjectConfigError::Toml(_))
            ));
        }
        let incomplete = base.replace("extends = 'trunk'\n", "") + "base_branch = 'main'\n";
        assert!(matches!(
            ProjectConfig::from_toml(&incomplete),
            Err(ProjectConfigError::InvalidWorkflow(
                PolicyError::MissingField {
                    field: PolicyField::WorkingBranch
                }
            ))
        ));
        assert!(matches!(
            ProjectConfig::from_toml(&base.replace("extends = 'trunk'", "extends = 'custom'")),
            Err(ProjectConfigError::InvalidWorkflow(PolicyError::CustomBase))
        ));
    }
}
