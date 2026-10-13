//! Content-based platform dispatch; a failed probe is never a publishable source tree.

use std::{
    ffi::{OsStr, OsString},
    fs,
    path::Path,
};

use super::{ArtifactError, Identity, identity::inspect_identity, io_error};
use crate::project::{
    ProjectType,
    build::{Ibcmd, ProcessStream},
};

/// Retry a native suffix only when a complete export proves that the probe identity was false.
/// The platform may silently read an empty configuration under the wrong suffix. Each retry
/// owns a fresh database; an input, launch, cancellation or ordinary platform failure is final.
pub(super) fn export_sources<F>(
    tool: &Ibcmd,
    pid: &Path,
    data: &Path,
    snapshot: &Path,
    destination: &Path,
    output: &mut F,
) -> Result<(), ArtifactError>
where
    F: FnMut(ProcessStream, &[u8]),
{
    let original = snapshot
        .extension()
        .and_then(OsStr::to_str)
        .ok_or_else(|| ArtifactError::UnsupportedFile(snapshot.to_owned()))?;
    let suffixes = std::iter::once(original).chain(
        ["cf", "cfe", "epf", "erf"]
            .into_iter()
            .filter(|suffix| *suffix != original),
    );
    let mut current = snapshot.to_owned();
    for (index, suffix) in suffixes.enumerate() {
        let candidate = snapshot.with_extension(suffix);
        if candidate != current {
            fs::rename(&current, &candidate).map_err(|error| io_error(&current, error))?;
        }
        let database = if index == 0 {
            data.to_owned()
        } else {
            let path = data.with_file_name(format!("infobase-{index}"));
            run(
                tool,
                pid,
                "create-infobase",
                vec!["infobase".into(), "create".into(), option("--data=", &path)],
                output,
            )?;
            path
        };
        if attempt_sources(tool, pid, &database, &candidate, destination, output)? {
            return Ok(());
        }
        fs::remove_dir_all(destination).map_err(|error| io_error(destination, error))?;
        let recovered = data.with_file_name("registered-sources");
        fs::remove_dir_all(&recovered).map_err(|error| io_error(&recovered, error))?;
        current = candidate;
    }
    Err(ArtifactError::InvalidDescriptor(destination.to_owned()))
}

/// A failed file export is evidence for dispatch, never a publishable source tree.
fn attempt_sources<F>(
    tool: &Ibcmd,
    pid: &Path,
    data: &Path,
    snapshot: &Path,
    destination: &Path,
    output: &mut F,
) -> Result<bool, ArtifactError>
where
    F: FnMut(ProcessStream, &[u8]),
{
    let mut buffered = Vec::new();
    let probe = run(
        tool,
        pid,
        "export",
        vec![
            "infobase".into(),
            "config".into(),
            "export".into(),
            option("--data=", data),
            option("--file=", snapshot),
            destination.as_os_str().to_owned(),
        ],
        &mut |stream, bytes| buffered.push((stream, bytes.to_vec())),
    );
    // Cancellation and launch failures must not trigger another platform operation.
    if probe.is_err() && !matches!(&probe, Err(ArtifactError::Platform { .. })) {
        for (stream, bytes) in buffered {
            output(stream, &bytes);
        }
        return probe.map(|()| true);
    }
    let identity = if destination.is_dir() {
        match inspect_identity(destination) {
            Ok(identity) => identity,
            Err(error) => {
                // Failed exports may leave truncated XML; retain the actual native diagnostic.
                for (stream, bytes) in buffered {
                    output(stream, &bytes);
                }
                return Err(error);
            }
        }
    } else {
        None
    };
    if let Some(identity) = identity.filter(|identity| {
        matches!(
            identity.project_type,
            ProjectType::Configuration | ProjectType::Extension
        )
    }) {
        return export_registered(tool, pid, data, snapshot, destination, &identity, output);
    }
    for (stream, bytes) in buffered {
        output(stream, &bytes);
    }
    probe.map(|()| true)
}

/// Load the proven root into owned storage and verify a fresh, complete export.
fn export_registered<F>(
    tool: &Ibcmd,
    pid: &Path,
    data: &Path,
    snapshot: &Path,
    destination: &Path,
    identity: &Identity,
    output: &mut F,
) -> Result<bool, ArtifactError>
where
    F: FnMut(ProcessStream, &[u8]),
{
    // The platform's loader uses the suffix even though file export reveals the actual type.
    // Normalize only the exclusively owned snapshot after its content has been identified.
    let registered = data.with_file_name(if identity.project_type == ProjectType::Extension {
        "registered.cfe"
    } else {
        "registered.cf"
    });
    fs::rename(snapshot, &registered).map_err(|error| io_error(snapshot, error))?;
    let mut load = vec![
        "infobase".into(),
        "config".into(),
        "load".into(),
        option("--data=", data),
    ];
    let mut export = vec![
        "infobase".into(),
        "config".into(),
        "export".into(),
        option("--data=", data),
    ];
    if identity.project_type == ProjectType::Extension {
        let extension = OsString::from(format!("--extension={}", identity.name));
        load.push(extension.clone());
        export.push(extension);
    }
    load.push(registered.as_os_str().to_owned());
    run(tool, pid, "load-configuration", load, output)?;
    let recovered = data.with_file_name("registered-sources");
    export.push(recovered.as_os_str().to_owned());
    run(tool, pid, "export", export, output)?;
    let actual = inspect_identity(&recovered)?
        .ok_or_else(|| ArtifactError::MissingDescriptor(recovered.clone()))?;
    if &actual != identity {
        fs::rename(&registered, snapshot).map_err(|error| io_error(&registered, error))?;
        return Ok(false);
    }
    let dump_info = recovered.join("ConfigDumpInfo.xml");
    if !fs::symlink_metadata(&dump_info).is_ok_and(|metadata| metadata.is_file()) {
        return Err(ArtifactError::MissingDescriptor(dump_info));
    }
    // Neither the partially exported descriptors nor their resources are published.
    fs::remove_dir_all(destination).map_err(|error| io_error(destination, error))?;
    fs::rename(&recovered, destination).map_err(|error| io_error(&recovered, error))?;
    Ok(true)
}

/// Construct one native argument without lossy path conversion or shell interpolation.
pub(super) fn option(prefix: &str, path: &Path) -> OsString {
    let mut value = OsString::from(prefix);
    value.push(path);
    value
}

/// Preserve native diagnostics and stage information for the presentation layer.
pub(super) fn run<F>(
    tool: &Ibcmd,
    pid: &Path,
    stage: &'static str,
    arguments: Vec<OsString>,
    output: &mut F,
) -> Result<(), ArtifactError>
where
    F: FnMut(ProcessStream, &[u8]),
{
    let result = tool
        .run_interruptible(arguments, pid, output)
        .map_err(ArtifactError::Run)?;
    if !result.status.success() {
        return Err(ArtifactError::Platform {
            stage,
            output: format!(
                "{}{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            ),
        });
    }
    Ok(())
}
