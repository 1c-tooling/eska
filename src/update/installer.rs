//! Official cargo-dist receipt validation and version-pinned updates.

use super::{Error, stable_version};
use axoupdater::{AxoUpdater, ReleaseSourceType, UpdateRequest};
use semver::Version;
use std::path::Path;

/// Accept only the official ESKA repository and a receipt for the executable being run.
pub(super) fn detect(
    executable: &Path,
    client: &reqwest::Client,
) -> Result<Option<AxoUpdater>, Error> {
    if executable.file_name()
        != Some(std::ffi::OsStr::new(if cfg!(windows) {
            "eska.exe"
        } else {
            "eska"
        }))
    {
        return Ok(None);
    }
    let mut updater = AxoUpdater::new_for("eska");
    if updater.load_receipt().is_err() {
        return Ok(None);
    }
    if !updater.source.as_ref().is_some_and(|source| {
        source.owner == "1c-tooling"
            && source.name == "eska"
            && source.app_name == "eska"
            && source.release_type == ReleaseSourceType::GitHub
    }) || !updater
        .check_receipt_is_for_this_executable()
        .unwrap_or(false)
    {
        return Ok(None);
    }
    updater
        .set_client(client.clone())
        .disable_installer_stdout()
        .enable_installer_stderr();
    updater
        .set_current_version(
            env!("CARGO_PKG_VERSION")
                .parse()
                .map_err(|_| Error::InvalidVersion)?,
        )
        .map_err(|_| Error::Installer)?;
    Ok(Some(updater))
}

/// Query a stable release; the current executable's version wins over stale receipt version text.
pub(super) async fn version(
    updater: &mut AxoUpdater,
    target: Option<&Version>,
    current: &Version,
) -> Result<Version, Error> {
    if let Some(target) = target {
        updater.configure_version_specifier(UpdateRequest::SpecificTag(format!("v{target}")));
    }
    let result = updater
        .query_new_version()
        .await
        .map_err(|_| Error::Network)?;
    result.map_or_else(
        || Ok(current.clone()),
        |version| stable_version(&version.to_string()),
    )
}

/// Pin the inspected release so a concurrent publication cannot change the version being installed.
pub(super) async fn install(updater: &mut AxoUpdater, version: &Version) -> Result<(), Error> {
    updater.configure_version_specifier(UpdateRequest::SpecificTag(format!("v{version}")));
    updater
        .run()
        .await
        .map_err(|_| Error::Installer)?
        .ok_or(Error::Installer)?;
    Ok(())
}
