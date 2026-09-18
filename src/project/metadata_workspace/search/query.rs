use super::{IndexProgress, IndexState, MatchRank, SearchHit, SearchOptions, SearchResponse};
use crate::project::metadata_model::NodeId;
use crate::project::metadata_workspace::{ProjectSession, WorkspaceError};

impl ProjectSession {
    /// Inspect indexing progress without starting work or reading files.
    #[must_use]
    pub fn search_progress(&self) -> IndexProgress {
        IndexProgress {
            state: self.search_index.state,
            generation: self.generation,
            indexed_objects: self
                .search_index
                .records
                .values()
                .filter(|record| self.search_index.visible(record))
                .count(),
            pending_descriptors: self.search_index.pending.len(),
            failed_descriptors: self.search_index.failures.len(),
        }
    }

    /// Query retained names/synonyms without XML IO; empty input intentionally returns no hits.
    #[must_use]
    pub fn search(&self, text: &str, options: &SearchOptions) -> SearchResponse {
        let query = text.trim().to_lowercase();
        let limit = options.limit.clamp(1, 500);
        let mut matches = Vec::new();
        if !query.is_empty() {
            for (id, record) in &self.search_index.records {
                if !self.search_index.visible(record) {
                    continue;
                }
                let mut rank = matching(&record.folded, &query, false);
                for (language, synonym) in &record.folded_synonyms {
                    if options
                        .synonym_language
                        .as_ref()
                        .is_none_or(|requested| requested == language)
                    {
                        rank = [rank, matching(synonym, &query, true)]
                            .into_iter()
                            .flatten()
                            .min();
                    }
                }
                if let Some(rank) = rank {
                    matches.push((rank, &record.folded, id, record));
                }
            }
        }
        matches.sort_by(|a, b| (&a.0, a.1, a.2).cmp(&(&b.0, b.1, b.2)));
        let truncated = matches.len() > limit;
        let hits = matches
            .into_iter()
            .take(limit)
            .map(|(rank, _, id, record)| SearchHit {
                project: self.scope().clone(),
                generation: self.generation,
                object: id.clone(),
                node: NodeId::Object(id.clone()),
                kind: record.kind,
                name: record.name.clone(),
                synonyms: record.synonyms.clone(),
                ancestry: record.ancestry.clone(),
                rank,
            })
            .collect();
        SearchResponse {
            progress: self.search_progress(),
            hits,
            truncated,
        }
    }

    /// Open only the ancestors of a selected indexed result; unrelated branches stay lazy.
    ///
    /// # Errors
    /// Rejects stale/project-mismatched hits or missing/invalid source branches.
    pub fn reveal_search_hit(&mut self, hit: &SearchHit) -> Result<Vec<NodeId>, WorkspaceError> {
        self.check_generation(hit.generation)?;
        if &hit.project != self.scope() {
            return Err(WorkspaceError::UnknownProject(hit.project.clone()));
        }
        for node in &hit.ancestry {
            if node == &hit.node {
                break;
            }
            if matches!(node, NodeId::Object(_)) {
                self.children(node, crate::project::configurator::TreeOptions::default())?;
            }
        }
        self.ancestry(&hit.node)
    }

    /// Pause between bounded index steps; existing results remain explicitly partial.
    pub fn cancel_search_index(&mut self) {
        if self.search_index.state == IndexState::Building {
            self.search_index.state = IndexState::Cancelled;
        }
    }

    /// Continue retained work after cancellation, including newly invalidated descriptors.
    pub fn resume_search_index(&mut self) {
        if self.search_index.state == IndexState::Cancelled {
            self.search_index.state = IndexState::Building;
            self.search_index.settle();
        }
    }
}

/// Unicode lowercase is used, without stemming, fuzzy matching or conflating ё and е.
fn matching(value: &str, query: &str, synonym: bool) -> Option<MatchRank> {
    if value == query {
        Some(if synonym {
            MatchRank::ExactSynonym
        } else {
            MatchRank::ExactName
        })
    } else if value.starts_with(query) {
        Some(if synonym {
            MatchRank::PrefixSynonym
        } else {
            MatchRank::PrefixName
        })
    } else if value.contains(query) {
        Some(if synonym {
            MatchRank::SubstringSynonym
        } else {
            MatchRank::SubstringName
        })
    } else {
        None
    }
}
