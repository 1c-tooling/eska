//! Machine-local configuration stored outside project repositories.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use directories::BaseDirs;
use serde::Deserialize;

const CONFIG_DIRECTORY_ENV: &str = "ESKA_CONFIG_DIR";
const DEFAULT_CONFIG: &str = "[build]\nrunner = \"auto\"\n";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RunnerKind {
    #[default]
    Auto,
    Host,
    Distrobox,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GlobalBuildConfig {
    pub runner: RunnerKind,
    pub container: Option<String>,
    pub platform_arch: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GlobalConfig {
    pub build: GlobalBuildConfig,
}

impl GlobalConfig {
    /// Load the machine config, or return portable defaults when it does not exist.
    pub fn load(path: &Path) -> Result<Self, GlobalConfigError> {
        let input = match fs::read_to_string(path) {
            Ok(input) => input,
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => return Err(io_error(path, source)),
        };
        Self::parse(path, &input)
    }

    /// Parse and validate the strict machine-local TOML schema.
    fn parse(path: &Path, input: &str) -> Result<Self, GlobalConfigError> {
        let raw: RawGlobalConfig =
            toml::from_str(input).map_err(|source| GlobalConfigError::Invalid {
                path: path.to_owned(),
                source,
            })?;
        let runner = match raw.build.runner {
            RawRunnerKind::Auto => RunnerKind::Auto,
            RawRunnerKind::Host => RunnerKind::Host,
            RawRunnerKind::Distrobox => RunnerKind::Distrobox,
        };
        if runner == RunnerKind::Distrobox && raw.build.container.is_none() {
            return Err(GlobalConfigError::DistroboxContainerMissing {
                path: path.to_owned(),
            });
        }
        if runner == RunnerKind::Host && raw.build.container.is_some() {
            return Err(GlobalConfigError::HostContainerUnexpected {
                path: path.to_owned(),
            });
        }
        Ok(Self {
            build: GlobalBuildConfig {
                runner,
                container: raw.build.container,
                platform_arch: raw.build.platform_arch,
            },
        })
    }
}

/// Load the machine config from its resolved path.
pub fn load() -> Result<(PathBuf, GlobalConfig), GlobalConfigError> {
    let path = config_path()?;
    let config = GlobalConfig::load(&path)?;
    Ok((path, config))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGlobalConfig {
    #[serde(default)]
    build: RawGlobalBuildConfig,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGlobalBuildConfig {
    #[serde(default)]
    runner: RawRunnerKind,
    container: Option<String>,
    platform_arch: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum RawRunnerKind {
    #[default]
    Auto,
    Host,
    Distrobox,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InitOutcome {
    Created(PathBuf),
    Existing(PathBuf),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditOutcome {
    Unchanged(PathBuf),
    Changed { path: PathBuf, backup: PathBuf },
}

#[derive(Debug)]
pub enum GlobalConfigError {
    LocationUnavailable,
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Invalid {
        path: PathBuf,
        source: toml::de::Error,
    },
    DistroboxContainerMissing {
        path: PathBuf,
    },
    HostContainerUnexpected {
        path: PathBuf,
    },
    Editor {
        source: io::Error,
    },
    EditorFailed,
    Replace {
        path: PathBuf,
        source: io::Error,
    },
}

/// Resolve the standard per-user config location on the current operating system.
pub fn config_path() -> Result<PathBuf, GlobalConfigError> {
    if let Some(directory) = std::env::var_os(CONFIG_DIRECTORY_ENV) {
        return Ok(PathBuf::from(directory).join("config.toml"));
    }
    BaseDirs::new()
        .map(|directories| directories.config_dir().join("eska").join("config.toml"))
        .ok_or(GlobalConfigError::LocationUnavailable)
}

/// Create the default config without overwriting an existing file.
pub fn init_at(path: &Path) -> Result<InitOutcome, GlobalConfigError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or(GlobalConfigError::LocationUnavailable)?;
    fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
            return Ok(InitOutcome::Existing(path.to_owned()));
        }
        Err(source) => return Err(io_error(path, source)),
    };
    if let Err(source) = file
        .write_all(DEFAULT_CONFIG.as_bytes())
        .and_then(|()| file.sync_all())
    {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(io_error(path, source));
    }
    Ok(InitOutcome::Created(path.to_owned()))
}

/// Edit a temporary copy, validate it and atomically retain the previous config as backup.
pub fn edit_at<F>(path: &Path, edit: F) -> Result<EditOutcome, GlobalConfigError>
where
    F: FnOnce(&Path) -> Result<(), GlobalConfigError>,
{
    init_at(path)?;
    let original = fs::read(path).map_err(|source| io_error(path, source))?;
    let (temporary, mut temporary_file) = create_unique_sibling(path, "edit")?;
    if let Err(source) = temporary_file
        .write_all(&original)
        .and_then(|()| temporary_file.sync_all())
    {
        drop(temporary_file);
        let _ = fs::remove_file(&temporary);
        return Err(io_error(&temporary, source));
    }
    drop(temporary_file);
    if let Ok(metadata) = fs::metadata(path) {
        fs::set_permissions(&temporary, metadata.permissions())
            .map_err(|source| io_error(&temporary, source))?;
    }
    if let Err(error) = edit(&temporary) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    let edited = match fs::read(&temporary) {
        Ok(edited) => edited,
        Err(source) => {
            let _ = fs::remove_file(&temporary);
            return Err(io_error(&temporary, source));
        }
    };
    if edited == original {
        fs::remove_file(&temporary).map_err(|source| io_error(&temporary, source))?;
        return Ok(EditOutcome::Unchanged(path.to_owned()));
    }
    let text = match std::str::from_utf8(&edited) {
        Ok(text) => text,
        Err(source) => {
            let _ = fs::remove_file(&temporary);
            return Err(GlobalConfigError::Io {
                path: temporary,
                source: io::Error::new(io::ErrorKind::InvalidData, source),
            });
        }
    };
    if let Err(error) = GlobalConfig::parse(path, text) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    let backup = unique_backup(path)?;
    fs::rename(path, &backup).map_err(|source| io_error(path, source))?;
    if let Err(source) = fs::rename(&temporary, path) {
        let _ = fs::rename(&backup, path);
        let _ = fs::remove_file(&temporary);
        return Err(GlobalConfigError::Replace {
            path: path.to_owned(),
            source,
        });
    }
    Ok(EditOutcome::Changed {
        path: path.to_owned(),
        backup,
    })
}

/// Create a unique sibling file and keep the reservation open for its caller.
fn create_unique_sibling(path: &Path, purpose: &str) -> Result<(PathBuf, File), GlobalConfigError> {
    for sequence in 0..1000_u16 {
        let candidate = path.with_extension(format!("toml.{purpose}-{sequence}"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {}
            Err(source) => return Err(io_error(&candidate, source)),
        }
    }
    Err(io_error(
        path,
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "no unique config path available",
        ),
    ))
}

/// Choose a timestamped backup path while preserving every earlier backup.
fn unique_backup(path: &Path) -> Result<PathBuf, GlobalConfigError> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    for sequence in 0..1000_u16 {
        let suffix = if sequence == 0 {
            format!("backup-{timestamp}")
        } else {
            format!("backup-{timestamp}-{sequence}")
        };
        let candidate = path.with_extension(format!("toml.{suffix}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(io_error(
        path,
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "no unique backup path available",
        ),
    ))
}

fn io_error(path: &Path, source: io::Error) -> GlobalConfigError {
    GlobalConfigError::Io {
        path: path.to_owned(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create an isolated config path for one transactional test.
    fn test_path(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "eska-global-config-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        root.join("config.toml")
    }

    #[test]
    fn init_creates_default_once_without_overwrite() {
        let path = test_path("init");
        assert!(matches!(init_at(&path), Ok(InitOutcome::Created(_))));
        fs::write(&path, "[build]\nrunner = \"host\"\n").expect("replace fixture");
        assert!(matches!(init_at(&path), Ok(InitOutcome::Existing(_))));
        assert!(
            matches!(GlobalConfig::load(&path), Ok(config) if config.build.runner == RunnerKind::Host)
        );
        fs::remove_dir_all(path.parent().expect("parent")).expect("cleanup");
    }

    #[test]
    fn changed_edit_keeps_previous_config_as_backup() {
        let path = test_path("backup");
        init_at(&path).expect("initialize");
        let outcome = edit_at(&path, |temporary| {
            fs::write(temporary, "[build]\nrunner = \"host\"\n")
                .map_err(|source| io_error(temporary, source))
        })
        .expect("edit config");
        let EditOutcome::Changed { backup, .. } = outcome else {
            panic!("expected changed config");
        };
        assert_eq!(fs::read_to_string(&backup).expect("backup"), DEFAULT_CONFIG);
        assert!(
            matches!(GlobalConfig::load(&path), Ok(config) if config.build.runner == RunnerKind::Host)
        );
        fs::remove_dir_all(path.parent().expect("parent")).expect("cleanup");
    }

    #[test]
    fn invalid_edit_preserves_original_without_temporary_file() {
        let path = test_path("invalid");
        init_at(&path).expect("initialize");
        assert!(matches!(
            edit_at(&path, |temporary| {
                fs::write(temporary, "not toml").map_err(|source| io_error(temporary, source))
            }),
            Err(GlobalConfigError::Invalid { .. })
        ));
        assert_eq!(fs::read_to_string(&path).expect("original"), DEFAULT_CONFIG);
        assert_eq!(
            fs::read_dir(path.parent().expect("parent"))
                .expect("directory")
                .count(),
            1
        );
        fs::remove_dir_all(path.parent().expect("parent")).expect("cleanup");
    }
}
