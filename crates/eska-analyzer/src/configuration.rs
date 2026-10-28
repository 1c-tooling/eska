//! Deterministic derived configuration, ownership checks and one active context per workspace root.

use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use eska_project_core::application::{self, AnalysisContext, AnalyzerPolicy, SourceRole};
use eska_project_core::project::discovery::{self, DiscoveryContext};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    Error,
    process::{Analyzer, SUPPORTED_VERSION},
    storage::{self, ConfigLock, Temporary},
};

mod formatting;

pub const GENERATOR_VERSION: u32 = 1;
pub const FILE_NAME: &str = "bsl-analyzer.toml";
const HEADER: &str = "# eska-generated ";

/// A generated config carries only reproducibility and ownership facts, not editor state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Ownership {
    pub schema_version: u32,
    pub generator_version: u32,
    pub analyzer_version: String,
    pub context: Option<String>,
    pub input_fingerprint: String,
    pub body_sha256: String,
}

/// Freshness is a read-only result; a foreign config is never adopted implicitly.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Current,
    Missing,
    Stale,
    MigrationRequired,
    ManagedConflict,
    ContextConflict,
}

/// A bounded snapshot binds preview, validation and publication to the same inputs.
#[derive(Debug)]
pub struct Plan {
    pub context: AnalysisContext,
    pub ownership: Ownership,
    pub contents: Vec<u8>,
    pub previous: Option<Vec<u8>>,
    pub status: Status,
    pub token: String,
    pub(crate) manifest: Vec<u8>,
}

impl Plan {
    /// Return the one ordinary config path discovered by the published LSP server.
    #[must_use]
    pub fn path(&self) -> PathBuf {
        self.context.root().join(FILE_NAME)
    }
}

/// Keep the shared OS lock alive for the lifetime of an analyzer client or CI run.
pub struct Lease {
    _lock: ConfigLock,
}

/// Resolve checked context and policy without writing any file or starting any process.
///
/// # Errors
/// Rejects invalid source roots, unreadable inputs or nonportable analyzer paths.
pub fn inspect(start: &Path, application: Option<&str>) -> Result<Plan, Error> {
    let context = application::resolve(start, application)
        .map_err(|error| Error::Context(Box::new(error)))?;
    let manifest = read(context.root(), &context.root().join("eska.toml"))?;
    build(context, manifest, None)
}

/// Rebuild a proposed policy while all source, topology and platform inputs stay unchanged.
pub(crate) fn build(
    context: AnalysisContext,
    manifest: Vec<u8>,
    policy: Option<&AnalyzerPolicy>,
) -> Result<Plan, Error> {
    let root = context.root();
    let body = generate(
        &context,
        policy.unwrap_or_else(|| context.analyzer_policy()),
        &manifest,
    )?;
    let mut hash = Sha256::new();
    hash.update(GENERATOR_VERSION.to_le_bytes());
    hash.update(SUPPORTED_VERSION.as_bytes());
    digest_input(&mut hash, Path::new("eska.toml"), &manifest)?;
    for source in context.sources() {
        let member_manifest = source.project().root().join("eska.toml");
        if !source.is_reference() && member_manifest != root.join("eska.toml") {
            digest_input(
                &mut hash,
                relative(root, &member_manifest)?,
                &read(root, &member_manifest)?,
            )?;
        }
        let descriptor = source.project().source().join(source.descriptor());
        let source_root = if source.is_reference() {
            source.project().source()
        } else {
            root
        };
        digest_input(
            &mut hash,
            context_path(root, &descriptor, source.is_reference())?,
            &read(source_root, &descriptor)?,
        )?;
    }
    hash.update(body.as_bytes());
    let ownership = Ownership {
        schema_version: 1,
        generator_version: GENERATOR_VERSION,
        analyzer_version: SUPPORTED_VERSION.into(),
        context: context.name().map(|name| name.as_str().into()),
        input_fingerprint: hex(&hash.finalize()),
        body_sha256: sha256(body.as_bytes()),
    };
    let header = serde_json::to_string(&ownership).map_err(|_| Error::InvalidPolicy {
        field: "ownership".into(),
    })?;
    let contents = format!("{HEADER}{header}\n{body}").into_bytes();
    let path = root.join(FILE_NAME);
    let previous = storage::read_optional(&path)?;
    let status = status(previous.as_deref(), &ownership, &contents);
    let mut token = Sha256::new();
    token.update(&contents);
    token.update(previous.as_deref().unwrap_or_default());
    let token = hex(&token.finalize());
    Ok(Plan {
        context,
        ownership,
        contents,
        previous,
        status,
        token,
        manifest,
    })
}

/// Validate with the selected published executable, then atomically publish only a changed owned file.
///
/// # Errors
/// Preserves previous contents on conflicts, changed inputs, failed validation, cancellation or timeout.
pub fn prepare(plan: &Plan, analyzer: &Analyzer<'_>, switch_context: bool) -> Result<Plan, Error> {
    match plan.status {
        Status::MigrationRequired => return Err(Error::MigrationRequired { path: plan.path() }),
        Status::ManagedConflict => return Err(Error::ManagedConflict { path: plan.path() }),
        Status::ContextConflict if !switch_context => return Err(context_conflict(plan)),
        _ => {}
    }
    if plan.status == Status::Current {
        let _lease = lease(plan)?;
        analyzer.validate(plan.context.root())?;
        analyzer.check_config(plan.context.root(), &plan.path())?;
        ensure_snapshot(plan)?;
        return inspect(plan.context.root(), plan.ownership.context.as_deref());
    }
    let _lock = ConfigLock::exclusive(plan.context.root())?;
    ensure_snapshot(plan)?;
    analyzer.validate(plan.context.root())?;
    let candidate = Temporary::new(plan.context.root(), &plan.contents)?;
    analyzer.check_config(plan.context.root(), candidate.path())?;
    ensure_snapshot(plan)?;
    candidate.publish(&plan.path())?;
    inspect(plan.context.root(), plan.ownership.context.as_deref())
}

/// Acquire a shared lease only for exactly the config which was prepared for this context.
///
/// # Errors
/// Rejects stale plans and writer contention before a client can use a different config.
pub fn lease(plan: &Plan) -> Result<Lease, Error> {
    let lock = ConfigLock::shared(plan.context.root())?;
    if plan.status != Status::Current {
        return Err(Error::StalePreview);
    }
    ensure_snapshot(plan)?;
    Ok(Lease { _lock: lock })
}

/// Inspect a current configuration once under its shared lease, without accepting an earlier preview.
///
/// # Errors
/// Rejects invalid contexts, writer contention, changed ownership roots and noncurrent configurations.
pub fn inspect_leased(start: &Path, application: Option<&str>) -> Result<(Plan, Lease), Error> {
    // Manifest discovery locates the lock without parsing source XML. The complete
    // inspection happens only after acquiring it; callers still compare its token
    // with their prepared configuration before starting the language process.
    let discovered = discovery::discover_context(start).map_err(|failure| {
        Error::Context(Box::new(application::ContextError::Discovery(Box::new(
            failure,
        ))))
    })?;
    let root = match discovered {
        DiscoveryContext::Standalone(project) => project.root().to_owned(),
        DiscoveryContext::Workspace { workspace, .. } => workspace.root().to_owned(),
    };
    let lock = ConfigLock::shared(&root)?;
    let plan = inspect(start, application)?;
    if plan.context.root() != root || plan.status != Status::Current {
        return Err(Error::StalePreview);
    }
    storage::unchanged(&plan.path(), plan.previous.as_deref())?;
    Ok((plan, Lease { _lock: lock }))
}

/// Detect manifest/root edits even if the generated file itself has not changed.
pub(crate) fn ensure_snapshot(plan: &Plan) -> Result<(), Error> {
    let current = inspect(plan.context.root(), plan.ownership.context.as_deref())?;
    if current.token != plan.token {
        return Err(Error::StalePreview);
    }
    storage::unchanged(&plan.path(), plan.previous.as_deref())
}

/// The structured source set disables upstream's extension and external auto-discovery explicitly.
pub(crate) fn generate(
    context: &AnalysisContext,
    policy: &AnalyzerPolicy,
    manifest: &[u8],
) -> Result<String, Error> {
    let mut table: toml::Table =
        toml::from_str(policy.as_toml()).map_err(|error| Error::InvalidToml {
            path: context.root().join("eska.toml"),
            detail: error.to_string(),
        })?;
    if let Some(version) = context.target_platform_version() {
        table.insert("target_platform_version".into(), version.as_str().into());
    }
    let mut source = match table.remove("source") {
        None => toml::Table::new(),
        Some(toml::Value::Table(table)) => table,
        Some(_) => {
            return Err(Error::InvalidPolicy {
                field: "source".into(),
            });
        }
    };
    let mut extensions = Vec::new();
    let mut externals = Vec::new();
    for item in context.sources() {
        let path = portable(context_path(
            context.root(),
            item.project().source(),
            item.is_reference(),
        )?)?;
        match item.role() {
            SourceRole::Configuration => {
                source.insert("root".into(), path.into());
                if let Some(mode) = item.compatibility_mode() {
                    table.insert("compatibility_mode".into(), mode.into());
                }
            }
            SourceRole::Extension => {
                let name = item
                    .member()
                    .map_or("standalone-extension", |name| name.as_str());
                let mut extension = toml::Table::new();
                extension.insert("name".into(), name.into());
                extension.insert("path".into(), path.into());
                if let Some(scope) = item.extension_scope() {
                    extension.insert("scope".into(), scope.as_str().into());
                }
                let depends = item
                    .member()
                    .and_then(|member| context.extension_dependencies().get(member))
                    .map_or_else(Vec::new, |names| {
                        names
                            .iter()
                            .map(|name| toml::Value::String(name.as_str().into()))
                            .collect()
                    });
                extension.insert("dependsOn".into(), toml::Value::Array(depends));
                extensions.push(toml::Value::Table(extension));
            }
            SourceRole::ExternalObject => {
                let name = item
                    .member()
                    .map_or("standalone-external", |name| name.as_str());
                let mut external = toml::Table::new();
                external.insert("name".into(), name.into());
                external.insert("path".into(), path.into());
                externals.push(toml::Value::Table(external));
            }
        }
    }
    source.insert("extensions".into(), toml::Value::Array(extensions));
    source.insert("externals".into(), toml::Value::Array(externals));
    table.insert("source".into(), toml::Value::Table(source));
    formatting::apply(context, manifest, &mut table)?;
    toml::to_string_pretty(&table).map_err(|_| Error::InvalidPolicy {
        field: "serialization".into(),
    })
}

/// Validate ownership before treating stale derived data as replaceable.
fn status(previous: Option<&[u8]>, expected: &Ownership, contents: &[u8]) -> Status {
    let Some(previous) = previous else {
        return Status::Missing;
    };
    let Ok(text) = std::str::from_utf8(previous) else {
        return Status::MigrationRequired;
    };
    let Some((header, body)) = text.split_once('\n') else {
        return Status::MigrationRequired;
    };
    let Some(header) = header.strip_prefix(HEADER) else {
        return Status::MigrationRequired;
    };
    let Ok(ownership) = serde_json::from_str::<Ownership>(header) else {
        return Status::ManagedConflict;
    };
    if ownership.schema_version != 1
        || ownership.generator_version != GENERATOR_VERSION
        || ownership.body_sha256 != sha256(body.as_bytes())
    {
        return Status::ManagedConflict;
    }
    if ownership.context != expected.context {
        return Status::ContextConflict;
    }
    if previous == contents {
        Status::Current
    } else {
        Status::Stale
    }
}

/// Include component boundaries when hashing concatenated input paths and bytes.
fn digest_input(hash: &mut Sha256, path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let path = portable(path)?;
    hash.update((path.len() as u64).to_le_bytes());
    hash.update(path.as_bytes());
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
    Ok(())
}

/// Canonical context paths must stay under the owning root even when a symlink was encountered.
fn relative<'a>(root: &Path, path: &'a Path) -> Result<&'a Path, Error> {
    let value = path
        .strip_prefix(root)
        .map_err(|_| Error::NonPortablePath { path: path.into() })?;
    Ok(if value.as_os_str().is_empty() {
        Path::new(".")
    } else {
        value
    })
}

/// Only explicitly resolved references may use absolute paths outside the owning workspace.
fn context_path<'a>(root: &Path, path: &'a Path, reference: bool) -> Result<&'a Path, Error> {
    if reference && !path.starts_with(root) {
        Ok(path)
    } else {
        relative(root, path)
    }
}

/// Normalize native separators only; Unix backslashes remain literal filename characters.
fn portable(path: &Path) -> Result<String, Error> {
    if path.is_absolute() {
        return path
            .to_str()
            .map(|path| {
                if cfg!(windows) {
                    path.replace('\\', "/")
                } else {
                    path.into()
                }
            })
            .ok_or_else(|| Error::NonPortablePath { path: path.into() });
    }
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => components.push(
                value
                    .to_str()
                    .ok_or_else(|| Error::NonPortablePath { path: path.into() })?,
            ),
            Component::CurDir => {}
            _ => return Err(Error::NonPortablePath { path: path.into() }),
        }
    }
    Ok(if components.is_empty() {
        ".".into()
    } else {
        components.join("/")
    })
}

/// Hash exact bytes, including BOM, line endings and comments.
pub(crate) fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Format bytes without relying on an output-array formatting implementation in the hash crate.
pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [
                char::from(DIGITS[usize::from(byte >> 4)]),
                char::from(DIGITS[usize::from(byte & 15)]),
            ]
        })
        .collect()
}

/// Retain source coordinates for failed snapshot reads.
pub(crate) fn read(root: &Path, path: &Path) -> Result<Vec<u8>, Error> {
    let canonical = fs::canonicalize(path).map_err(|source| Error::Io {
        path: path.into(),
        source,
    })?;
    if !canonical.starts_with(root) {
        return Err(Error::NonPortablePath { path: path.into() });
    }
    storage::read_optional(&canonical)?.ok_or_else(|| Error::Io {
        path: path.into(),
        source: std::io::Error::from(std::io::ErrorKind::NotFound),
    })
}

/// A second context needs an explicit coordinated switch and an exclusive lock.
fn context_conflict(plan: &Plan) -> Error {
    let current = plan
        .previous
        .as_deref()
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .and_then(|text| text.lines().next())
        .and_then(|line| line.strip_prefix(HEADER))
        .and_then(|value| serde_json::from_str::<Ownership>(value).ok())
        .and_then(|ownership| ownership.context);
    Error::ContextConflict {
        current,
        requested: plan.ownership.context.clone(),
    }
}
