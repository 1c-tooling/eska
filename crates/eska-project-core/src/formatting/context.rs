//! Manifest-aware scope and effective styles are shared by CLI and editor requests.

use super::{Config, Effective, Error, Language, Layer, Patch};
use crate::{
    config::{ManifestConfig, WorkspaceConfig},
    project::{
        Project,
        discovery::{self, DiscoveryContext},
        metadata_workspace::{MetadataWorkspace, SourceAccessRequest},
        selection::{self, SelectionIntent},
        source_lock::SourceLease,
    },
};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

struct PolicyLayer {
    root: PathBuf,
    config: Config,
    manifest: String,
}

/// A context retains checked manifests, without starting an analyzer or a complete metadata index.
pub struct Context {
    root: PathBuf,
    projects: Vec<Project>,
    layers: Vec<PolicyLayer>,
    metadata: BTreeMap<PathBuf, MetadataWorkspace>,
}

impl Context {
    /// Resolve explicit members through ordinary ESKA discovery; absent manifests permit standalone files.
    ///
    /// # Errors
    /// Rejects malformed existing manifests, ambiguous selection and unsafe source paths.
    pub fn open(start: &Path, members: &[String], workspace: bool) -> Result<Self, Error> {
        let start = fs::canonicalize(start)
            .map_err(|_| Error::new("source_io", start.to_string_lossy()))?;
        let start = if start.is_file() {
            start
                .parent()
                .ok_or_else(|| Error::new("source_io", "path"))?
        } else {
            &start
        };
        if !has_manifest(start)? {
            if !members.is_empty() || workspace {
                return Err(Error::new("invalid_scope", "member"));
            }
            return Ok(Self {
                root: start.to_owned(),
                projects: Vec::new(),
                layers: Vec::new(),
                metadata: BTreeMap::new(),
            });
        }
        let context = discovery::discover_context(start)
            .map_err(|_| Error::new("invalid_config", "eska.toml"))?;
        let selected = selection::select_projects(
            &context,
            members,
            workspace,
            SelectionIntent::MultipleMutation,
        )
        .map_err(|_| Error::new("invalid_scope", "member"))?;
        let projects = selected
            .projects()
            .iter()
            .map(|selected| selected.project().clone())
            .collect::<Vec<_>>();
        let mut layers = Vec::new();
        let root = match &context {
            DiscoveryContext::Standalone(project) => project.root().to_owned(),
            DiscoveryContext::Workspace { workspace, .. } => {
                let config = WorkspaceConfig::load(&workspace.root().join("eska.toml"))
                    .map_err(|_| Error::new("invalid_config", "eska.toml"))?;
                layers.push(layer(workspace.root(), config.formatting_settings())?);
                workspace.root().to_owned()
            }
        };
        for project in &projects {
            layers.push(layer(
                project.root(),
                project.configuration().formatting_settings(),
            )?);
        }
        Ok(Self {
            root,
            projects,
            layers,
            metadata: BTreeMap::new(),
        })
    }

    /// Configuration migration targets the owning workspace/root manifest, never a derived analyzer file.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve the same workspace/member layers for a native buffer, query owner or disk file.
    ///
    /// # Errors
    /// Returns invalid policies, unsupported overrides and conflicting legacy options.
    pub fn policy(
        &self,
        path: &Path,
        language: Language,
        operation: &Patch,
    ) -> Result<Effective, Error> {
        let selected = self
            .layers
            .iter()
            .filter(|layer| path.starts_with(&layer.root))
            .map(|layer| {
                Layer::new(
                    &layer.root,
                    if layer.root == self.root {
                        "root"
                    } else {
                        "member"
                    },
                    &layer.config,
                    &layer.manifest,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        super::resolve(&selected, path, language, operation)
    }

    /// Select only owned source roots by default; an explicit object is its ordinary source directory.
    ///
    /// # Errors
    /// Rejects symlink traversal, out-of-selection paths and oversized directory inventories.
    pub fn files(
        &self,
        start: &Path,
        scope: Option<&Path>,
        xml: bool,
    ) -> Result<Vec<PathBuf>, Error> {
        let targets = scope.map_or_else(
            || {
                if self.projects.is_empty() {
                    vec![start.to_owned()]
                } else {
                    self.projects
                        .iter()
                        .map(|project| project.source().to_owned())
                        .collect()
                }
            },
            |scope| vec![start.join(scope)],
        );
        let mut files = Vec::new();
        let mut visited = 0;
        for target in targets {
            if !self.projects.is_empty()
                && !self
                    .projects
                    .iter()
                    .any(|project| target.starts_with(project.root()))
            {
                return Err(Error::new("invalid_scope", target.to_string_lossy()));
            }
            collect(&target, xml, &mut files, 0, &mut visited)?;
        }
        files.sort();
        files.dedup();
        if xml && files.len() != 1 {
            return Err(Error::new("invalid_scope", "query"));
        }
        Ok(files)
    }

    /// Recheck targeted support and active source owners without inventorying all metadata.
    ///
    /// # Errors
    /// Returns read-only, unresolved source, filesystem and shared-owner failures.
    pub fn check_file(&mut self, path: &Path) -> Result<(), Error> {
        let metadata = fs::symlink_metadata(path)
            .map_err(|_| Error::new("source_io", path.to_string_lossy()))?;
        if !metadata.is_file() || metadata.is_symlink() {
            return Err(Error::new("unsafe_path", path.to_string_lossy()));
        }
        if metadata.permissions().readonly() {
            return Err(Error::new("source_read_only", path.to_string_lossy()));
        }
        SourceLease::check(self.lock_root(path), path).map_err(|error| lock_error(&error, path))?;
        self.check_support(path, None)
    }

    /// A publisher holding the source lease rechecks support without probing its own exclusive OS lock.
    pub(super) fn check_support(
        &mut self,
        path: &Path,
        permit: Option<&SourceLease>,
    ) -> Result<(), Error> {
        let own =
            permit.is_some_and(|permit| permit.owns(self.lock_root(path), path).unwrap_or(false));
        if path
            .extension()
            .is_some_and(|extension| extension == "sdbl")
        {
            return Ok(());
        }
        if let Some(project) = self
            .projects
            .iter()
            .find(|project| path.starts_with(project.source()))
        {
            let root = project.root().to_owned();
            if !self.metadata.contains_key(&root) {
                let workspace = MetadataWorkspace::open(&root, &[], false)
                    .map_err(|_| Error::new("unsupported_source", path.to_string_lossy()))?;
                self.metadata.insert(root.clone(), workspace);
            }
            let workspace = self
                .metadata
                .get_mut(&root)
                .ok_or_else(|| Error::new("unsupported_source", "project"))?;
            let scope = workspace
                .projects()
                .first()
                .ok_or_else(|| Error::new("unsupported_source", "project"))?
                .scope()
                .clone();
            let project = workspace
                .project_mut(&scope)
                .map_err(|_| Error::new("unsupported_source", "project"))?;
            let relative = path
                .strip_prefix(project.project().source())
                .map_err(|_| Error::new("unsupported_source", "path"))?;
            let mut protection = project
                .source_protection(
                    &[SourceAccessRequest {
                        path: relative.to_owned(),
                        user_read_only: false,
                    }],
                    true,
                )
                .map_err(|_| Error::new("unsupported_source", path.to_string_lossy()))?;
            if own {
                for protection in &mut protection {
                    protection.shared_read_only = false;
                }
            }
            if protection
                .first()
                .is_none_or(|protection| !protection.writable())
            {
                return Err(Error::new("source_read_only", path.to_string_lossy()));
            }
        } else if !self.projects.is_empty() {
            return Err(Error::new("unsupported_source", path.to_string_lossy()));
        }
        Ok(())
    }

    /// Canonical project roots keep CLI leases compatible with query sessions from another process.
    #[must_use]
    pub fn lock_root<'a>(&'a self, path: &'a Path) -> &'a Path {
        self.projects
            .iter()
            .find(|project| path.starts_with(project.root()))
            .map_or_else(
                || {
                    if self.projects.is_empty() {
                        path.parent().unwrap_or(&self.root)
                    } else {
                        &self.root
                    }
                },
                Project::root,
            )
    }
}

/// Broken links and permission failures count as existing invalid configuration, never as absent defaults.
fn has_manifest(start: &Path) -> Result<bool, Error> {
    for root in start.ancestors() {
        match fs::symlink_metadata(root.join("eska.toml")) {
            Ok(_) => return Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(Error::new("source_io", "eska.toml")),
        }
    }
    Ok(false)
}

/// Retain sparse original settings instead of serializing inherited defaults into member overrides.
fn layer(root: &Path, config: &Config) -> Result<PolicyLayer, Error> {
    let path = root.join("eska.toml");
    ManifestConfig::load(&path)
        .map_err(|_| Error::new("invalid_config", path.to_string_lossy()))?;
    let manifest =
        fs::read_to_string(&path).map_err(|_| Error::new("source_io", path.to_string_lossy()))?;
    Ok(PolicyLayer {
        root: root.to_owned(),
        config: config.clone(),
        manifest,
    })
}

/// Directory traversal is bounded and never follows symlinks into another project or generated tree.
fn collect(
    path: &Path,
    xml: bool,
    output: &mut Vec<PathBuf>,
    depth: usize,
    visited: &mut usize,
) -> Result<(), Error> {
    *visited = visited.saturating_add(1);
    if depth > 128 || *visited > 100_000 {
        return Err(Error::new("scope_too_large", "scope"));
    }
    let metadata =
        fs::symlink_metadata(path).map_err(|_| Error::new("source_io", path.to_string_lossy()))?;
    if metadata.is_symlink() {
        return Err(Error::new("unsafe_path", path.to_string_lossy()));
    }
    if metadata.is_file() {
        if path.extension().is_some_and(|extension| {
            extension == "bsl" || extension == "sdbl" || xml && extension == "xml"
        }) {
            output.push(
                fs::canonicalize(path)
                    .map_err(|_| Error::new("source_io", path.to_string_lossy()))?,
            );
        }
    } else if metadata.is_dir() {
        for entry in
            fs::read_dir(path).map_err(|_| Error::new("source_io", path.to_string_lossy()))?
        {
            let entry = entry.map_err(|_| Error::new("source_io", path.to_string_lossy()))?;
            if [".git", ".eska", "target", "build", "node_modules"]
                .iter()
                .any(|name| entry.file_name() == *name)
            {
                continue;
            }
            collect(&entry.path(), xml, output, depth + 1, visited)?;
        }
    }
    Ok(())
}

/// Lock contention remains distinct from unavailable coordination storage.
pub(crate) fn lock_error(error: &io::Error, path: &Path) -> Error {
    Error::new(
        if error.kind() == io::ErrorKind::WouldBlock {
            "source_busy"
        } else {
            "source_lock_unavailable"
        },
        path.to_string_lossy(),
    )
}
