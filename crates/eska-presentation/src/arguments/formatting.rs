//! Pure formatter option metadata is identical for the facade and standalone component.

use super::check::Format;
use crate::localization::Localizer;
use clap::Args;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct FmtArgs {
    pub scope: Option<PathBuf>,
    #[arg(long)]
    pub member: Vec<String>,
    #[arg(long, conflicts_with = "member")]
    pub workspace: bool,
    #[command(flatten)]
    preview: Preview,
    #[arg(long)]
    pub style: Option<String>,
    #[arg(long, conflicts_with = "query_index")]
    pub query_at: Option<usize>,
    #[arg(long)]
    pub query_index: Option<usize>,
    #[command(flatten)]
    migration: Migration,
    #[arg(long,value_enum,default_value_t=Format::Human)]
    pub format: Format,
    #[arg(short,long,action=clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Debug, Args)]
struct Preview {
    #[arg(long,conflicts_with_all=["diff","dump_config","migrate_config","apply_migration"])]
    check: bool,
    #[arg(long,conflicts_with_all=["dump_config","migrate_config","apply_migration"])]
    diff: bool,
    #[arg(long,conflicts_with_all=["migrate_config","apply_migration"])]
    dump_config: bool,
}

#[derive(Debug, Args)]
struct Migration {
    #[arg(long,conflicts_with_all=["scope","member","workspace","style","query_at","query_index","apply_migration"])]
    migrate_config: bool,
    #[arg(long,conflicts_with_all=["scope","member","workspace","style","query_at","query_index"])]
    apply_migration: bool,
}

impl FmtArgs {
    /// CI check mode never publishes source edits.
    #[must_use]
    pub const fn check(&self) -> bool {
        self.preview.check
    }
    /// Diff mode renders an inspectable plan without publication.
    #[must_use]
    pub const fn diff(&self) -> bool {
        self.preview.diff
    }
    /// Effective policy inspection is independent of source syntax and write permissions.
    #[must_use]
    pub const fn dump_config(&self) -> bool {
        self.preview.dump_config
    }
    /// Migration preview has a separate manifest-only scope.
    #[must_use]
    pub const fn migrate_config(&self) -> bool {
        self.migration.migrate_config
    }
    /// Only this explicit mode may publish the migration and its backup.
    #[must_use]
    pub const fn apply_migration(&self) -> bool {
        self.migration.apply_migration
    }
}

/// No project discovery or executable lookup is needed to render either entry point's help.
#[must_use]
pub fn localize(mut command: clap::Command, localizer: &Localizer) -> clap::Command {
    command = command.about(localizer.text("fmt-about"));
    for (name, key) in [
        ("scope", "fmt-scope"),
        ("member", "fmt-member"),
        ("workspace", "fmt-workspace"),
        ("check", "fmt-check"),
        ("diff", "fmt-diff"),
        ("dump_config", "fmt-dump-config"),
        ("style", "fmt-style"),
        ("query_at", "fmt-query-at"),
        ("query_index", "fmt-query-index"),
        ("migrate_config", "fmt-migrate-config"),
        ("apply_migration", "fmt-apply-migration"),
        ("format", "version-format-help"),
        ("help", "cli-help"),
    ] {
        command = command.mut_arg(name, |argument| argument.help(localizer.text(key)));
    }
    command
}
