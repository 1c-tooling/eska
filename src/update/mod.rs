//! Installation-aware CLI updates; ordinary project and IDE operations never call this module.

mod cargo;
mod installer;
mod releases;

use semver::Version;
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

/// Stable outcomes shared by human output and extension clients.
#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    UpToDate,
    UpdateAvailable,
    Updated,
    UnsupportedInstallation,
}

/// A completed inspection or update, without localized presentation.
#[derive(Debug)]
pub struct Outcome {
    pub status: Status,
    pub installed: String,
    pub available: Option<String>,
    pub method: &'static str,
    pub executable: PathBuf,
}

/// Stable operation failures; subprocess output is sent to stderr during installation.
#[derive(Clone, Copy, Debug)]
pub enum Error {
    Io,
    Network,
    CargoQuery,
    CargoInstall,
    Installer,
    Busy,
    InvalidVersion,
    Verification,
    Runtime,
}

impl Error {
    /// Keep machine codes independent of host locale and dependency error messages.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Io => "io",
            Self::Network => "network",
            Self::CargoQuery => "cargo-query",
            Self::CargoInstall => "cargo-install",
            Self::Installer => "installer",
            Self::Busy => "busy",
            Self::InvalidVersion => "invalid-version",
            Self::Verification => "verification",
            Self::Runtime => "runtime",
        }
    }
}

/// The selected channel owns exactly one executable and never falls back after a failed update.
enum Installation {
    Cargo(cargo::Installation),
    Installer(Box<axoupdater::AxoUpdater>),
    Unknown,
}

/// Inspect the running executable, query its channel, and optionally update to a stable version.
pub fn run(check: bool, target: Option<&str>) -> Result<Outcome, Error> {
    let requested = target.map(stable_version).transpose()?;
    let executable = std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(|_| Error::Io)?;
    let current = Version::parse(env!("CARGO_PKG_VERSION")).map_err(|_| Error::InvalidVersion)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| Error::Runtime)?;
    runtime.block_on(async {
        let client = reqwest::Client::builder()
            .user_agent(concat!("eska/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| Error::Network)?;
        let mut installation = detect(&executable, &client)?;
        let method = match &installation {
            Installation::Cargo(_) => "cargo",
            Installation::Installer(_) => "installer",
            Installation::Unknown => "unknown",
        };
        let mut outcome = Outcome {
            status: Status::UnsupportedInstallation,
            installed: current.to_string(),
            available: None,
            method,
            executable: executable.clone(),
        };
        let version = match &mut installation {
            Installation::Cargo(_) => releases::cargo_version(&client, requested.as_ref()).await?,
            Installation::Installer(updater) => {
                installer::version(updater, requested.as_ref(), &current).await?
            }
            Installation::Unknown => return Ok(outcome),
        };
        outcome.available = Some(version.to_string());
        // Neither an older release nor an explicit target can silently downgrade the installation.
        if version <= current {
            outcome.status = Status::UpToDate;
            return Ok(outcome);
        }
        outcome.status = Status::UpdateAvailable;
        if check {
            return Ok(outcome);
        }
        let root = match &installation {
            Installation::Cargo(cargo) => cargo.root.clone(),
            Installation::Installer(updater) => updater
                .install_prefix_root()
                .map_err(|_| Error::Installer)?
                .into_std_path_buf(),
            Installation::Unknown => return Ok(outcome),
        };
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join(".eska-update.lock"))
            .map_err(|_| Error::Io)?;
        lock.try_lock().map_err(|_| Error::Busy)?;
        // Another window may have replaced the executable since this process started.
        if executable_version(&executable)? != current {
            return Err(Error::Busy);
        }
        match &mut installation {
            Installation::Cargo(cargo) => cargo.install(&version)?,
            Installation::Installer(updater) => installer::install(updater, &version).await?,
            Installation::Unknown => return Ok(outcome),
        }
        if executable_version(&executable)? != version {
            return Err(Error::Verification);
        }
        outcome.status = Status::Updated;
        Ok(outcome)
    })
}

/// Prefer Cargo ownership, then a matching official receipt, never an unrelated binary's receipt.
fn detect(executable: &Path, client: &reqwest::Client) -> Result<Installation, Error> {
    if let Some(cargo) = cargo::detect(executable)? {
        return Ok(Installation::Cargo(cargo));
    }
    Ok(
        installer::detect(executable, client)?.map_or(Installation::Unknown, |value| {
            Installation::Installer(Box::new(value))
        }),
    )
}

/// Only release versions are accepted as update targets; prerelease/dev builds are explicit custom paths.
fn stable_version(value: &str) -> Result<Version, Error> {
    let version = Version::parse(value).map_err(|_| Error::InvalidVersion)?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err(Error::InvalidVersion);
    }
    Ok(version)
}

/// Verify the actual replaced file rather than trusting the updater's exit status alone.
fn executable_version(executable: &Path) -> Result<Version, Error> {
    let output = Command::new(executable)
        .arg("--version")
        .output()
        .map_err(|_| Error::Verification)?;
    if !output.status.success() {
        return Err(Error::Verification);
    }
    let text = std::str::from_utf8(&output.stdout).map_err(|_| Error::Verification)?;
    Version::parse(
        text.trim()
            .strip_prefix("eska ")
            .ok_or(Error::Verification)?,
    )
    .map_err(|_| Error::Verification)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_targets_reject_prereleases_and_command_fragments() {
        for value in [
            "1.2.3-rc.1",
            "1.2.3+dev",
            "latest",
            "--force",
            "1.2.3; echo test",
        ] {
            assert!(stable_version(value).is_err());
        }
        assert_eq!(stable_version("0.11.0").unwrap().to_string(), "0.11.0");
    }
}
