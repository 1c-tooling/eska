//! CLI source selection consumes explicit project-owned topology without native XML discovery.

use std::{collections::BTreeSet, path::PathBuf};

use eska::{
    project_core::{eska_source::EskaSource, source_format::PROJECT_SOURCE_IMPORT_RECEIPT},
    tool_cli::{self, Failure},
};
use project_model::{ExtensionDecl, ProjectConfig};

/// Source ownership stays available for navigation, policy and union guards until host publication.
pub struct Sources {
    selected: EskaSource,
    selected_workspace: Option<PathBuf>,
    others: Vec<(EskaSource, Option<PathBuf>)>,
    configured: bool,
}

impl Sources {
    /// Open only configured ESKA source directories, resolving all paths against the selected project.
    pub fn open(
        selected: EskaSource,
        workspace: Option<PathBuf>,
        config: Option<&ProjectConfig>,
    ) -> Result<Self, Failure> {
        let configured = config.is_some_and(|config| {
            config.configuration_root.is_some()
                || config
                    .extensions
                    .as_ref()
                    .is_some_and(|extensions| !extensions.is_empty())
        });
        let mut result = Self {
            selected,
            selected_workspace: workspace,
            others: Vec::new(),
            configured,
        };
        if !configured {
            return Ok(result);
        }
        let config = config.ok_or_else(|| Failure::new("analysis_config_invalid"))?;
        let anchor = result.selected.project().root();
        let mut paths = vec![
            config
                .resolve_configuration_path(anchor)
                .map_err(|_| Failure::new("analysis_config_invalid"))?
                .unwrap_or_else(|| result.selected.reader().root().to_owned()),
        ];
        for extension in config.extensions.iter().flatten() {
            let ExtensionDecl::Structured(extension) = extension else {
                return Err(Failure::new("analysis_scope_unsupported"));
            };
            if extension.path.is_empty() || extension.path.contains('*') {
                return Err(Failure::new("analysis_scope_unsupported"));
            }
            paths.push(anchor.join(&extension.path));
        }
        let mut seen = BTreeSet::new();
        for path in paths {
            let canonical = std::fs::canonicalize(&path)
                .map_err(|_| Failure::new("analysis_config_invalid").at(path.to_string_lossy()))?;
            if !seen.insert(canonical.clone()) {
                return Err(Failure::new("analysis_config_invalid").at(path.to_string_lossy()));
            }
            if canonical == result.selected.reader().root() {
                continue;
            }
            let selected = tool_cli::selected(&canonical, None)?;
            let workspace = selected.workspace_root;
            let source = EskaSource::open(selected.project, selected.scope)?;
            if source.reader().root() != canonical {
                return Err(Failure::new("analysis_scope_unsupported").at(path.to_string_lossy()));
            }
            result.others.push((source, workspace));
        }
        result.require_idle()?;
        Ok(result)
    }

    /// The selected project continues to own public paths, metadata navigation and editing authority.
    pub const fn selected(&self) -> &EskaSource {
        &self.selected
    }

    /// Configuration scope requires this explicit admission rather than a standalone CFE promotion.
    pub const fn configured(&self) -> bool {
        self.configured
    }

    /// Every admitted project's controls participate, even when a sibling is not semantically visible.
    pub fn iter(&self) -> impl Iterator<Item = &EskaSource> {
        self.controls().map(|(source, _)| source)
    }

    /// Workspace controls come from actual project selection, never source-directory ancestry.
    pub fn controls(&self) -> impl Iterator<Item = (&EskaSource, Option<&std::path::Path>)> {
        std::iter::once((&self.selected, self.selected_workspace.as_deref())).chain(
            self.others
                .iter()
                .map(|(source, workspace)| (source, workspace.as_deref())),
        )
    }

    /// Retain guard paths through a checker run after readers move into the resident host.
    pub fn project_roots(&self) -> Vec<PathBuf> {
        self.iter()
            .map(|source| source.project().root().to_owned())
            .collect()
    }

    /// No receipt from any admitted root may be ignored while publishing a new host.
    pub fn require_idle(&self) -> Result<(), Failure> {
        require_idle(&self.project_roots())
    }

    /// A single-source caller cannot accidentally discard a configured dependency set.
    pub fn into_single(self) -> Result<eska::project_core::source_format::SourceReader, Failure> {
        if self.configured {
            return Err(Failure::new("analysis_scope_unsupported"));
        }
        Ok(self.selected.into_reader())
    }

    /// Transfer every verified reader together only after validating the complete topology.
    pub fn into_composition(
        self,
        config: &ProjectConfig,
    ) -> Result<super::AdmittedComposition, Failure> {
        let anchor = self.selected.project().root().to_owned();
        let selected = self.selected.reader().root().to_owned();
        let readers = std::iter::once(self.selected)
            .chain(self.others.into_iter().map(|(source, _)| source))
            .map(EskaSource::into_reader)
            .collect();
        super::admit(&anchor, config, readers, &selected).map_err(admission_failure)
    }
}

/// A late import blocks checker publication even when it belongs to a later dependency root.
pub fn require_idle(roots: &[PathBuf]) -> Result<(), Failure> {
    for root in roots {
        let receipt = root.join(PROJECT_SOURCE_IMPORT_RECEIPT);
        match std::fs::symlink_metadata(&receipt) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) | Ok(_) => {
                return Err(Failure::new("source_import_incomplete").at(receipt.to_string_lossy()));
            }
        }
    }
    Ok(())
}

/// Native topology errors never silently reduce the set to a standalone local context.
fn admission_failure(error: super::AdmissionError) -> Failure {
    if let super::AdmissionError::Source(error) = error {
        return error.into();
    }
    Failure::new("analysis_config_invalid")
}
