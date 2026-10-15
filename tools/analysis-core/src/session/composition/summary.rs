//! Composition coverage exposes current contributions without treating registration as readiness.

use std::path::PathBuf;

use crate::{AnalysisSession, composition::RootId};

/// Every root retains its own input state; only topology-visible roots define selected completeness.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct ContributionSummary {
    pub root: RootId,
    pub source_root: PathBuf,
    pub name: String,
    pub label: Option<String>,
    pub visible: bool,
    #[serde(flatten)]
    pub inputs: ContributionInputs,
    pub retained_source_bytes: u64,
}

/// Source readability and supported models remain independently observable for each contribution.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct ContributionInputs {
    pub metadata_inputs_complete: bool,
    pub metadata_models_complete: bool,
    pub module_inputs_complete: bool,
}

/// An admitted topology and a current-ready semantic context are deliberately separate facts.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct CompositionSummary {
    pub source_sha256: String,
    pub selected: RootId,
    pub roots: Vec<ContributionSummary>,
    pub current_complete: bool,
}

impl AnalysisSession {
    /// Report exact source identities and visible readiness without loading a contribution.
    #[must_use]
    pub fn composition_summary(&self) -> Option<CompositionSummary> {
        let source_sha256 = self.composition_sha256.clone()?;
        let visible = self.visible_root_ids(self.selected_root_id());
        let snapshot = self.database.workspace_configs_snapshot();
        let roots = self
            .root_ids()
            .filter_map(|id| {
                let root = self.root_inputs(id).ok()?;
                let reader = root.inputs.source_reader();
                Some(ContributionSummary {
                    root: id,
                    source_root: reader.root().to_owned(),
                    name: reader.configuration().declarations.first()?.name.clone(),
                    label: snapshot.paths.get(id.index() as usize)?.0.clone(),
                    visible: visible.contains(&id),
                    inputs: ContributionInputs {
                        metadata_inputs_complete: root.inputs.summary().metadata_input_complete,
                        metadata_models_complete: root.incomplete_metadata == 0,
                        module_inputs_complete: root.incomplete_modules == 0,
                    },
                    retained_source_bytes: root
                        .owners
                        .values()
                        .map(|owner| owner.source_bytes)
                        .sum::<u64>()
                        + root
                            .modules
                            .values()
                            .map(|module| module.model.bytes())
                            .sum::<u64>()
                        + root
                            .metadata_resources
                            .values()
                            .map(|resource| resource.input.bytes())
                            .sum::<u64>()
                        + root
                            .query_inputs
                            .values()
                            .map(crate::QueryTextInput::bytes)
                            .sum::<u64>(),
                })
            })
            .collect();
        Some(CompositionSummary {
            source_sha256,
            selected: self.selected_root_id(),
            roots,
            current_complete: self.require_current_composition().is_ok(),
        })
    }
}
