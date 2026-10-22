//! Compare the actual file universe and bytes, including newly created or removed modules.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
};

use sha2::{Digest, Sha256};

use super::Failure;
use crate::{configuration::Plan, process::Analyzer};

#[derive(Eq, PartialEq)]
pub(super) struct Snapshot {
    pub modules: BTreeSet<PathBuf>,
    files: BTreeMap<PathBuf, [u8; 32]>,
}

impl Snapshot {
    /// The generator remains the only owner of composition; policy exclusions prune directories.
    pub fn capture(plan: &Plan, analyzer: &Analyzer<'_>) -> Result<Self, Failure> {
        let policy: toml::Table = toml::from_str(plan.context.analyzer_policy().as_toml())
            .map_err(|_| Failure::PartialPolicy {
                field: "invalid_policy",
            })?;
        for field in ["diff_base", "ignored_authors"] {
            if policy
                .get("analysis")
                .and_then(toml::Value::as_table)
                .and_then(|table| table.get(field))
                .is_some_and(|value| match value {
                    toml::Value::String(value) => !value.is_empty(),
                    toml::Value::Array(value) => !value.is_empty(),
                    _ => true,
                })
            {
                return Err(Failure::PartialPolicy {
                    field: if field == "diff_base" {
                        "analysis.diff_base"
                    } else {
                        "analysis.ignored_authors"
                    },
                });
            }
        }
        let exclusions = exclusions(&policy, plan.context.root())?;
        let mut snapshot = Self {
            modules: BTreeSet::new(),
            files: BTreeMap::new(),
        };
        let mut visited = BTreeSet::new();
        let mut pending: Vec<_> = plan
            .context
            .sources()
            .iter()
            .map(|source| source.project().source().to_path_buf())
            .collect();
        while let Some(path) = pending.pop() {
            if analyzer.is_cancelled() {
                return Err(crate::Error::Cancelled.into());
            }
            if exclusions.iter().any(|excluded| path.starts_with(excluded))
                || !visited.insert(path.clone())
            {
                continue;
            }
            if visited.len() > 1_000_000 {
                return Err(crate::Error::InputTooLarge { path }.into());
            }
            let metadata = fs::symlink_metadata(&path).map_err(|source| Failure::Io {
                path: path.clone(),
                source,
            })?;
            // Upstream follows links. Refuse ambiguous aliases instead of silently omitting their files.
            if metadata.file_type().is_symlink() {
                return Err(Failure::UnsupportedPath { path });
            }
            if metadata.is_dir() {
                for entry in fs::read_dir(&path).map_err(|source| Failure::Io {
                    path: path.clone(),
                    source,
                })? {
                    pending.push(
                        entry
                            .map_err(|source| Failure::Io {
                                path: path.clone(),
                                source,
                            })?
                            .path(),
                    );
                }
            } else if metadata.is_file() {
                let extension = path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .unwrap_or_default();
                let module = extension.eq_ignore_ascii_case("bsl");
                if module || extension.eq_ignore_ascii_case("xml") {
                    if path.to_str().is_none() {
                        return Err(Failure::UnsupportedPath { path });
                    }
                    let digest = fingerprint(&path, analyzer)?;
                    if module {
                        snapshot.modules.insert(path.clone());
                    }
                    snapshot.files.insert(path, digest);
                }
            } else {
                return Err(Failure::UnsupportedPath { path });
            }
        }
        if snapshot.modules.is_empty() {
            return Err(Failure::EmptyInput);
        }
        Ok(snapshot)
    }

    /// Paths are root-relative so moving the same context does not change its content identity.
    pub fn fingerprint(&self, plan: &Plan) -> String {
        let mut hash = Sha256::new();
        hash.update(plan.ownership.input_fingerprint.as_bytes());
        for (path, bytes) in &self.files {
            let path = path
                .strip_prefix(plan.context.root())
                .unwrap_or(path)
                .to_string_lossy();
            hash.update((path.len() as u64).to_le_bytes());
            hash.update(path.as_bytes());
            hash.update(bytes);
        }
        crate::configuration::hex(&hash.finalize())
    }
}

/// Stream source bytes without retaining full configuration text or creating a second index.
fn fingerprint(path: &Path, analyzer: &Analyzer<'_>) -> Result<[u8; 32], Failure> {
    let mut file = File::open(path).map_err(|source| Failure::Io {
        path: path.into(),
        source,
    })?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 16 * 1024];
    loop {
        if analyzer.is_cancelled() {
            return Err(crate::Error::Cancelled.into());
        }
        let count = file.read(&mut buffer).map_err(|source| Failure::Io {
            path: path.into(),
            source,
        })?;
        if count == 0 {
            return Ok(hash.finalize().into());
        }
        hash.update(&buffer[..count]);
    }
}

/// Literal directory exclusions are relative to the generated config, as in upstream 0.3.2.
fn exclusions(policy: &toml::Table, root: &Path) -> Result<Vec<PathBuf>, Failure> {
    let Some(value) = policy
        .get("source")
        .and_then(toml::Value::as_table)
        .and_then(|table| table.get("exclude"))
    else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or(Failure::PartialPolicy {
        field: "source.exclude",
    })?;
    values
        .iter()
        .map(|value| {
            let value =
                value
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .ok_or(Failure::PartialPolicy {
                        field: "source.exclude",
                    })?;
            let mut path = root.to_path_buf();
            for component in Path::new(value).components() {
                match component {
                    Component::Normal(value) => path.push(value),
                    Component::CurDir => {}
                    Component::ParentDir if path != root => {
                        path.pop();
                    }
                    _ => {
                        return Err(Failure::PartialPolicy {
                            field: "source.exclude",
                        });
                    }
                }
            }
            if path.try_exists().map_err(|source| Failure::Io {
                path: path.clone(),
                source,
            })? {
                let canonical = path.canonicalize().map_err(|source| Failure::Io {
                    path: path.clone(),
                    source,
                })?;
                if canonical != path {
                    return Err(Failure::UnsupportedPath { path });
                }
            }
            Ok(path)
        })
        .collect()
}
