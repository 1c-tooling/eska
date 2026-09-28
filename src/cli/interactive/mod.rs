//! Shared keyboard-driven menus for CLI commands.

mod confirm;
mod keyboard;
mod render;
mod select;
mod terminal;

pub(super) use confirm::confirm_overwrite;
pub(super) use select::Selector;

#[derive(Clone, Copy, Debug)]
pub(super) enum PromptError {
    Cancelled,
    Io,
}

pub(super) const PROJECT_TYPE_CHOICES: [(&str, &str); 4] = [
    ("configuration", "new-type-configuration"),
    ("extension", "new-type-extension"),
    ("processing", "new-type-processing"),
    ("report", "new-type-report"),
];

pub(super) const WORKFLOW_CHOICES: [(&str, &str); 4] = [
    ("trunk", "new-workflow-trunk"),
    ("git-flow", "new-workflow-git-flow"),
    ("github-flow", "new-workflow-github-flow"),
    ("custom", "new-workflow-custom"),
];
