//! Single-member import retains shell-relative paths while the child owns all native/source writes.

use super::super::{Format, ImportArgs};
use crate::{
    cli::localization::Localizer,
    project::{build::PlatformVersion, selection::SelectedProject},
};
use std::{
    ffi::{OsStr, OsString},
    io,
    path::{self, Path},
};

/// Convert only shell-scoped input paths to absolute arguments, retaining OS-native path bytes.
pub(super) fn arguments(
    args: &ImportArgs,
    selected: SelectedProject<'_>,
    base: &Path,
    version: Option<&PlatformVersion>,
    localizer: &Localizer,
) -> io::Result<Vec<OsString>> {
    let mut values = vec![
        "--lang".into(),
        localizer.locale().as_str().into(),
        "--project-dir".into(),
        selected.project().root().as_os_str().to_owned(),
        "--format".into(),
        match args.format {
            Format::Human => "human",
            Format::Json => "json",
        }
        .into(),
        "import".into(),
        path::absolute(base.join(&args.file))?.into_os_string(),
    ];
    option(
        &mut values,
        "--project",
        selected.name().map(|name| OsStr::new(name.as_str())),
    );
    option(
        &mut values,
        "--accepted-base-sha256",
        args.source.accepted_base_sha256.as_deref().map(OsStr::new),
    );
    option(
        &mut values,
        "--ancestor-sha256",
        args.source.ancestor_sha256.as_deref().map(OsStr::new),
    );
    option(
        &mut values,
        "--expected-plan-sha256",
        args.source.expected_plan_sha256.as_deref().map(OsStr::new),
    );
    for (name, value) in [
        ("--ancestor", args.source.ancestor.as_deref()),
        ("--receipt", args.source.receipt.as_deref()),
    ] {
        if let Some(value) = value {
            values.push(name.into());
            values.push(path::absolute(base.join(value))?.into_os_string());
        }
    }
    option(
        &mut values,
        "--platform-version",
        version.map(|version| OsStr::new(version.as_str())),
    );
    option(
        &mut values,
        "--ibcmd",
        args.platform.ibcmd.as_deref().map(Path::as_os_str),
    );
    option(
        &mut values,
        "--platform-arch",
        args.platform.platform_arch.as_deref().map(OsStr::new),
    );
    option(
        &mut values,
        "--distrobox",
        args.platform.distrobox.as_deref().map(OsStr::new),
    );
    if args.source.write {
        values.push("--write".into());
    }
    // Preview is the child's default. --dry-run does not alter its result schema; --force is
    // refused before this boundary because it cannot bypass ESKA conflicts or support policy.
    Ok(values)
}

/// Keep flags separate from values instead of formatting a command line or parsing a shell.
fn option(values: &mut Vec<OsString>, name: &'static str, value: Option<&OsStr>) {
    if let Some(value) = value {
        values.push(name.into());
        values.push(value.to_owned());
    }
}
