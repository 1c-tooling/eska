use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use eska::{
    config::ProjectConfig,
    project::discovery,
    vcs::workflow::{FinishRequirement, PublishPlan, SyncStrategy},
};

use crate::support::TestDir;

const FULL: &str = r#"
[project]
type = "report"
[vcs.workflow]
preset = "custom"
[vcs.workflow.policy]
base_branch = "main"
working_branch = "task-branch"
task_branch_template = "work/{task}"
remote = "team"
sync_strategy = "merge"
integration_target = "main"
publish = "push-task-branch"
finish = "require-integrated"
delete_local_branch = false
"#;

fn validate(root: &Path, locale: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(root)
        .args(["--lang", locale])
        .output()
        .unwrap()
}

#[test]
fn discovery_preserves_policy_and_cli_validation_does_not_modify_git() {
    let dir = TestDir::new();
    fs::create_dir_all(dir.0.join("src/nested")).unwrap();
    fs::write(dir.0.join("eska.toml"), FULL).unwrap();
    gix::init(&dir.0).unwrap();
    let git_config = fs::read(dir.0.join(".git/config")).unwrap();
    let head = fs::read(dir.0.join(".git/HEAD")).unwrap();
    let config = ProjectConfig::load(&dir.0.join("eska.toml")).unwrap();
    let expected = config
        .configuration()
        .workflow_settings()
        .unwrap()
        .resolve(None)
        .unwrap()
        .plan("FI-8")
        .unwrap();
    let project = discovery::discover(&dir.0.join("src/nested")).unwrap();
    assert_eq!(project.configuration(), config.configuration());
    assert_eq!(
        project
            .configuration()
            .workflow_settings()
            .unwrap()
            .resolve(None)
            .unwrap()
            .plan("FI-8")
            .unwrap(),
        expected
    );
    assert_eq!(expected.working_branch, "work/FI-8");
    assert_eq!(expected.sync_strategy, SyncStrategy::Merge);
    for locale in ["ru", "en"] {
        let result = validate(&dir.0.join("src/nested"), locale);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(result.stdout.is_empty() && result.stderr.is_empty());
    }
    assert_eq!(fs::read(dir.0.join(".git/config")).unwrap(), git_config);
    assert_eq!(fs::read(dir.0.join(".git/HEAD")).unwrap(), head);
    assert!(!dir.0.join(".git/index").exists());
    assert!(!dir.0.join(".git/refs/heads/work").exists());
    assert_eq!(fs::read_to_string(dir.0.join("eska.toml")).unwrap(), FULL);
}

#[test]
fn implemented_presets_and_overrides_resolve_to_deterministic_plans() {
    let dir = TestDir::new();
    fs::create_dir(dir.0.join("src")).unwrap();
    for (workflow, expected_base, expected_branch, expected_remote, delete_local_branch) in [
        (
            "[vcs.workflow]\npreset = 'trunk'\n",
            "main",
            "task/FI-9",
            "origin",
            true,
        ),
        (
            "[vcs.workflow]\npreset = 'trunk'\n[vcs.workflow.policy]\nbase_branch = 'master'\ntask_branch_template = 'feature/{task}'\nintegration_target = 'master'\n",
            "master",
            "feature/FI-9",
            "origin",
            true,
        ),
        (
            "[vcs.workflow]\npreset = 'custom'\nextends = 'trunk'\n[vcs.workflow.policy]\ntask_branch_template = 'company/{task}'\nremote = 'team'\ndelete_local_branch = false\n",
            "main",
            "company/FI-9",
            "team",
            false,
        ),
        (
            "[vcs.workflow]\npreset = 'git-flow'\n",
            "develop",
            "feature/FI-9",
            "origin",
            true,
        ),
        (
            "[vcs.workflow]\npreset = 'custom'\nextends = 'git-flow'\n[vcs.workflow.policy]\ntask_branch_template = 'company/{task}'\nremote = 'team'\ndelete_local_branch = false\n",
            "develop",
            "company/FI-9",
            "team",
            false,
        ),
        (
            "[vcs.workflow]\npreset = 'github-flow'\n",
            "main",
            "feature/FI-9",
            "origin",
            true,
        ),
        (
            "[vcs.workflow]\npreset = 'custom'\nextends = 'github-flow'\n[vcs.workflow.policy]\ntask_branch_template = 'company/{task}'\nremote = 'team'\ndelete_local_branch = false\n",
            "main",
            "company/FI-9",
            "team",
            false,
        ),
    ] {
        let text = format!("[project]\ntype = 'report'\n{workflow}");
        fs::write(dir.0.join("eska.toml"), &text).unwrap();
        let config = ProjectConfig::load(&dir.0.join("eska.toml")).unwrap();
        let plan = config
            .configuration()
            .workflow_settings()
            .unwrap()
            .resolve(None)
            .unwrap()
            .plan("FI-9")
            .unwrap();
        assert_eq!(plan.base_branch, expected_base);
        assert_eq!(plan.working_branch, expected_branch);
        assert_eq!(plan.sync_strategy, SyncStrategy::Rebase);
        assert_eq!(
            plan.sync_reference,
            format!("refs/remotes/{expected_remote}/{expected_base}")
        );
        assert_eq!(plan.integration_target, expected_base);
        assert_eq!(
            plan.publish,
            PublishPlan::PushTaskBranch {
                remote: expected_remote.into(),
                branch: expected_branch.into(),
            }
        );
        assert_eq!(plan.finish, FinishRequirement::Integrated);
        assert_eq!(plan.delete_local_branch, delete_local_branch);
        for locale in ["ru", "en"] {
            let result = validate(&dir.0, locale);
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(result.stdout.is_empty() && result.stderr.is_empty());
        }
        assert_eq!(fs::read_to_string(dir.0.join("eska.toml")).unwrap(), text);
        assert!(!dir.0.join(".git").exists());
    }
}

#[test]
fn policy_errors_are_localized_and_do_not_write_files() {
    let dir = TestDir::new();
    fs::create_dir(dir.0.join("src")).unwrap();
    let header =
        "[project]\ntype = 'report'\n[vcs.workflow]\npreset = 'custom'\nextends = 'trunk'\n";
    for (text, ru, en, field) in [
        (format!("{header}[vcs.workflow.policy]\nbase_branch = '../main'"), "Неверное значение", "Invalid workflow policy value", "base_branch"),
        (format!("{header}[vcs.workflow.policy]\nsync_strategy = 'reset-hard'"), "Неверное значение", "Invalid workflow policy value", "sync_strategy"),
        ("[project]\ntype = 'report'\n[vcs.workflow]\npreset = 'custom'\n[vcs.workflow.policy]\nbase_branch = 'main'".into(), "Не задано поле", "Missing workflow policy field", "working_branch"),
        (header.replace("extends = 'trunk'", "extends = 'custom'"), "наследование от custom недопустимо", "not custom", "extends"),
        ("[project]\ntype = 'report'\n[vcs.workflow]\npreset = 'trunk'\nextends = 'git-flow'".into(), "только при preset", "requires preset", "custom"),
        (format!("{header}[vcs.workflow.policy]\npublish = 'disabled'\nfinish = 'require-published'"), "несовместим", "incompatible", "publish"),
        (format!("{header}[vcs.workflow.policy]\nfinish = 'require-published'\ndelete_local_branch = true"), "требует", "requires", "delete_local_branch"),
        (format!("{header}[vcs.workflow.policy]\ndelete_remote_branch = true"), "Некорректный TOML", "Invalid TOML", "eska.toml"),
    ] {
        fs::write(dir.0.join("eska.toml"), &text).unwrap();
        for (locale, expected) in [("ru", ru), ("en", en)] {
            let result = validate(&dir.0, locale);
            assert_eq!(result.status.code(), Some(1));
            assert!(result.stdout.is_empty());
            let error = String::from_utf8(result.stderr).unwrap();
            assert!(error.contains(expected) && error.contains(field), "{error}");
            assert!(!error.contains("PolicyError") && !error.contains("panicked"));
        }
        assert_eq!(fs::read_to_string(dir.0.join("eska.toml")).unwrap(), text);
        assert!(!dir.0.join(".git").exists());
    }
}
