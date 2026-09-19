//! Cargo ownership and channel-preserving installation.

use super::Error;
use semver::Version;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Debug, Deserialize)]
struct Metadata {
    installs: BTreeMap<String, serde_json::Value>,
}
#[derive(Debug, Deserialize)]
struct Options {
    bins: Vec<String>,
    features: Vec<String>,
    all_features: bool,
    no_default_features: bool,
    profile: String,
    target: String,
}

pub(super) struct Installation {
    pub root: PathBuf,
    options: Options,
}

/// Query Cargo for the actual executable's install root, including non-default `--root` installs.
pub(super) fn detect(executable: &Path) -> Result<Option<Installation>, Error> {
    if executable.file_name()
        != Some(std::ffi::OsStr::new(if cfg!(windows) {
            "eska.exe"
        } else {
            "eska"
        }))
    {
        return Ok(None);
    }
    let Some(bin) = executable
        .parent()
        .filter(|path| path.file_name().is_some_and(|name| name == "bin"))
    else {
        return Ok(None);
    };
    let Some(root) = bin.parent() else {
        return Ok(None);
    };
    let metadata = match fs::read(root.join(".crates2.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(Error::CargoQuery),
    };
    let Some(options) = options(&metadata, env!("CARGO_PKG_VERSION"))? else {
        return Ok(None);
    };
    let output = Command::new("cargo")
        .args(["install", "--list", "--root"])
        .arg(root)
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| Error::CargoQuery)?;
    if !output.status.success() {
        return Err(Error::CargoQuery);
    }
    let expected = format!("eska v{}:", env!("CARGO_PKG_VERSION"));
    if !String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| line == expected)
    {
        return Err(Error::CargoQuery);
    }
    Ok(Some(Installation {
        root: root.to_owned(),
        options,
    }))
}

/// Reject Git/path/foreign registry installs instead of converting them into crates.io installs.
fn options(bytes: &[u8], version: &str) -> Result<Option<Options>, Error> {
    let mut metadata: Metadata = serde_json::from_slice(bytes).map_err(|_| Error::CargoQuery)?;
    let key = format!("eska {version} (registry+https://github.com/rust-lang/crates.io-index)");
    let entry = metadata
        .installs
        .remove(&key)
        .map(serde_json::from_value::<Options>)
        .transpose()
        .map_err(|_| Error::CargoQuery)?;
    Ok(entry.filter(|entry| {
        entry
            .bins
            .iter()
            .any(|name| name == "eska" || name == "eska.exe")
    }))
}

impl Installation {
    /// Keep the installation root, target, profile and feature selection used by Cargo.
    fn arguments(&self, version: &Version) -> Vec<OsString> {
        let mut args: Vec<OsString> = [
            "install",
            "eska",
            "--locked",
            "--registry",
            "crates-io",
            "--version",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        args.extend([
            version.to_string().into(),
            "--root".into(),
            self.root.as_os_str().to_owned(),
            "--profile".into(),
            self.options.profile.clone().into(),
            "--target".into(),
            self.options.target.clone().into(),
        ]);
        if self.options.all_features {
            args.push("--all-features".into());
        }
        if self.options.no_default_features {
            args.push("--no-default-features".into());
        }
        if !self.options.features.is_empty() {
            args.extend(["--features".into(), self.options.features.join(",").into()]);
        }
        args
    }

    /// Cargo receives explicit arguments and owns compilation and metadata; installer fallback is forbidden.
    pub fn install(&self, version: &Version) -> Result<(), Error> {
        let mut command = Command::new("cargo");
        command
            .args(self.arguments(version))
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::from(std::io::stderr()))
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        let previous = park_executable()?;
        let success = command.status().is_ok_and(|status| status.success());
        #[cfg(windows)]
        restore_or_remove(previous, success)?;
        if success {
            Ok(())
        } else {
            Err(Error::CargoInstall)
        }
    }
}

#[cfg(windows)]
/// Windows cannot replace a running executable; park it until Cargo has installed the replacement.
fn park_executable() -> Result<(PathBuf, PathBuf), Error> {
    let current = std::env::current_exe().map_err(|_| Error::Io)?;
    let previous = current.with_file_name(format!("eska.previous-{}.exe", std::process::id()));
    if previous.exists() {
        return Err(Error::Busy);
    }
    fs::rename(&current, &previous).map_err(|_| Error::Io)?;
    Ok((current, previous))
}

#[cfg(windows)]
/// Restore a failed update or schedule deletion after the running executable exits.
fn restore_or_remove((current, previous): (PathBuf, PathBuf), success: bool) -> Result<(), Error> {
    if success {
        self_replace::self_delete_at(previous).map_err(|_| Error::Io)
    } else {
        if current.exists() {
            fs::remove_file(&current).map_err(|_| Error::Io)?;
        }
        fs::rename(previous, current).map_err(|_| Error::Io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cargo_options_preserve_nondefault_installation_and_reject_other_sources() {
        let bytes = br#"{"installs":{"eska 0.11.0 (registry+https://github.com/rust-lang/crates.io-index)":{"bins":["eska"],"features":["one","two"],"all_features":false,"no_default_features":true,"profile":"release","target":"x86_64-unknown-linux-gnu"}}}"#;
        let mut mixed: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        mixed["installs"]["unrelated old package"] = serde_json::json!({"bins":["other"]});
        assert!(
            options(&serde_json::to_vec(&mixed).unwrap(), "0.11.0")
                .unwrap()
                .is_some()
        );
        let settings = options(bytes, "0.11.0").unwrap().unwrap();
        let cargo = Installation {
            root: PathBuf::from("root with spaces"),
            options: settings,
        };
        let args = cargo.arguments(&Version::new(0, 11, 1));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--root", "root with spaces"])
        );
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--features", "one,two"])
        );
        assert!(args.contains(&"--no-default-features".into()));
        assert!(!args.contains(&"--force".into()));
        assert!(options(bytes, "0.10.0").unwrap().is_none());
        let other = String::from_utf8(bytes.to_vec()).unwrap().replace(
            "registry+https://github.com/rust-lang/crates.io-index",
            "git+https://example.org/fork",
        );
        assert!(options(other.as_bytes(), "0.11.0").unwrap().is_none());
    }
}
