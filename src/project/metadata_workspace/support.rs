//! A support snapshot does not expand the presentation tree or write any project source.

mod rules;
pub(super) use rules::RuleCache;

use super::ProjectSession;
use crate::project::{
    configurator::ConfiguratorSchema,
    metadata_model::{MetadataKind, ObjectId},
    metadata_parser::{self, PropertiesMode},
    support::{Reason, State, Support},
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::PathBuf,
};

/// Object permission stays independent of the physical file's combined permission.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectSupport {
    pub object_id: ObjectId,
    pub uuid: String,
    pub state: State,
    pub reason: Reason,
}

/// File-level explanation does not replace the linked objects' individual rules.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileReason {
    MixedObjects,
    SupportRule,
    Unavailable,
    NoSupportRestriction,
}

/// Every path is supplied by the existing Designer resolver, never a file-extension glob.
#[derive(Clone, Debug)]
pub struct FileSupport {
    pub path: PathBuf,
    pub objects: Vec<ObjectId>,
    pub read_only: bool,
    pub reason: FileReason,
    pub mixed: bool,
    pub unknown: bool,
}

/// Generation-scoped policy and ownership, with explicit diagnostics on partial reads.
#[derive(Clone, Debug)]
pub struct SupportSnapshot {
    pub next_offset: Option<usize>,
    pub objects: Vec<ObjectSupport>,
    pub files: Vec<FileSupport>,
    pub diagnostics: Vec<String>,
    pub suppliers: Vec<crate::project::support::Supplier>,
}

/// Each request loads at most 32 descriptors and yields to other IDE requests.
#[derive(Debug)]
pub(super) struct SupportCache {
    generation: u64,
    pending: VecDeque<ObjectId>,
    visited: BTreeSet<ObjectId>,
    policy: Option<std::sync::Arc<Support>>,
    diagnostics: Vec<String>,
    pages: Vec<std::sync::Arc<SupportSnapshot>>,
    restrict_unknown: bool,
}

impl ProjectSession {
    /// Report whether a metadata event leaves cached support and ownership valid.
    #[must_use]
    pub fn support_is_current(&self) -> bool {
        self.support_cache
            .as_ref()
            .is_some_and(|cache| cache.generation == self.generation)
    }

    /// Existing BSL content edits cannot change UUID ownership or vendor support rules.
    pub(super) fn support_unchanged(&self, paths: &[PathBuf]) -> bool {
        self.support_cache.as_ref().is_some_and(|cache| {
            !paths.is_empty()
                && paths.iter().all(|path| {
                    path.extension().is_some_and(|extension| extension == "bsl")
                        && self.project().source().join(path).is_file()
                        && cache
                            .pages
                            .iter()
                            .any(|page| page.files.iter().any(|file| &file.path == path))
                })
        })
    }

    /// Reuse policy pages only after an event proven unrelated to ownership or support.
    pub(super) const fn retain_support_generation(&mut self) {
        if let Some(cache) = &mut self.support_cache {
            cache.generation = self.generation;
        }
    }

    /// Return a cached page or perform one bounded build step in the current generation.
    pub fn support_page(&mut self, offset: usize) -> Option<std::sync::Arc<SupportSnapshot>> {
        let root_uuid = self
            .objects
            .get(self.source.root().id())
            .and_then(|object| object.uuid.clone());
        let mut cache = self
            .support_cache
            .take()
            .filter(|cache| cache.generation == self.generation)
            .unwrap_or_else(|| {
                let (policy, diagnostics) = self.support_rules.read(self.source.project().source());
                SupportCache {
                    generation: self.generation,
                    pending: VecDeque::from([self.source.root().id().clone()]),
                    visited: BTreeSet::new(),
                    restrict_unknown: policy.is_none()
                        && (self.support_seen.is_some() && self.support_seen == root_uuid
                            || diagnostics.iter().any(|value| !value.contains("NotFound"))),
                    policy,
                    diagnostics,
                    pages: Vec::new(),
                }
            });
        if offset == cache.pages.len() && (offset == 0 || !cache.pending.is_empty()) {
            let page = self.read_support_page(&mut cache);
            cache.pages.push(std::sync::Arc::new(page));
        }
        if cache
            .policy
            .as_ref()
            .is_some_and(|policy| !policy.suppliers.is_empty())
        {
            self.support_seen = root_uuid;
        }
        let result = cache.pages.get(offset).map(std::sync::Arc::clone);
        self.support_cache = Some(cache);
        result
    }

    /// Predefined XML has its own UUIDs and shares the owner's file-level restriction.
    fn support_predefined(
        &self,
        parsed: &mut metadata_parser::ParsedDescriptor,
        files: &mut BTreeMap<PathBuf, Vec<ObjectId>>,
        diagnostics: &mut Vec<String>,
    ) -> BTreeSet<PathBuf> {
        let mut additions = Vec::new();
        let mut unreadable = BTreeSet::new();
        for object in &parsed.objects {
            let owner = &object.metadata;
            if !ConfiguratorSchema::collections(owner.kind())
                .contains(&MetadataKind::PredefinedItem)
            {
                continue;
            }
            let path = match self.source.predefined_path(owner.id()) {
                Ok(Some(path)) => path,
                Ok(None) => continue,
                Err(_) => {
                    diagnostics.push(format!("source_unavailable:{}", owner.id()));
                    continue;
                }
            };
            files
                .entry(path.clone())
                .or_default()
                .push(owner.id().clone());
            let predefined = self
                .source
                .read_xml(&path)
                .ok()
                .flatten()
                .and_then(|input| {
                    metadata_parser::parse_predefined(&input, owner.id(), PropertiesMode::Summary)
                        .ok()
                });
            if let Some(predefined) = predefined {
                additions.extend(predefined.objects);
            } else {
                diagnostics.push(format!("descriptor_unavailable:{}:predefined", owner.id()));
                unreadable.insert(path);
            }
        }
        parsed.objects.extend(additions);
        unreadable
    }

    /// Read through existing bounded XML loaders and containment-checked source mappings.
    fn read_support_page(&mut self, cache: &mut SupportCache) -> SupportSnapshot {
        let mut snapshot = SupportSnapshot {
            next_offset: None,
            objects: Vec::new(),
            files: Vec::new(),
            diagnostics: if cache.pages.is_empty() {
                cache.diagnostics.clone()
            } else {
                Vec::new()
            },
            suppliers: Vec::new(),
        };
        let support = &cache.policy;
        if cache.pages.is_empty()
            && let Some(support) = support
        {
            snapshot.suppliers.clone_from(&support.suppliers);
        }
        let mut files: BTreeMap<PathBuf, Vec<ObjectId>> = BTreeMap::new();
        let mut unreadable = BTreeSet::new();
        for _ in 0..32 {
            let Some(id) = cache.pending.pop_front() else {
                break;
            };
            if !cache.visited.insert(id.clone()) {
                continue;
            }
            let parsed = self.cached_support_descriptor(&id).map_or_else(
                || metadata_parser::load(&self.source, &id, PropertiesMode::Summary),
                |parsed| Ok((*parsed).clone()),
            );
            let Ok(mut parsed) = parsed else {
                snapshot
                    .diagnostics
                    .push(format!("descriptor_unavailable:{id}"));
                continue;
            };
            unreadable.extend(self.support_predefined(
                &mut parsed,
                &mut files,
                &mut snapshot.diagnostics,
            ));
            for reference in parsed.references {
                cache.pending.push_back(reference.id);
            }
            for object in parsed.objects {
                let metadata = object.metadata;
                let (state, reason) = support
                    .as_ref()
                    .map_or((State::Unknown, Reason::Unavailable), |data| {
                        data.object(metadata.uuid())
                    });
                snapshot.objects.push(ObjectSupport {
                    object_id: metadata.id().clone(),
                    uuid: metadata.uuid().to_owned(),
                    state,
                    reason,
                });
                // Inline objects share a descriptor already checked for their owner.
                if let Ok((owner, candidates)) = self.source.descriptor_candidates(metadata.id())
                    && &owner != metadata.id()
                    && let Some(path) = candidates.iter().find(|path| files.contains_key(*path))
                {
                    files
                        .get_mut(path)
                        .into_iter()
                        .for_each(|owners| owners.push(metadata.id().clone()));
                    continue;
                }
                match self.source.sources(metadata.id()) {
                    Ok(sources) => {
                        for source in sources {
                            files
                                .entry(source.path)
                                .or_default()
                                .push(metadata.id().clone());
                        }
                    }
                    Err(_) => snapshot
                        .diagnostics
                        .push(format!("source_unavailable:{}", metadata.id())),
                }
            }
        }
        snapshot.files = file_policies(
            files,
            &snapshot.objects,
            &unreadable,
            cache.restrict_unknown,
        );
        snapshot.next_offset = (!cache.pending.is_empty()).then_some(cache.pages.len() + 1);
        snapshot
    }
}

/// A shared physical file takes the strongest policy of its independently classified objects.
fn file_policies(
    files: BTreeMap<PathBuf, Vec<ObjectId>>,
    objects: &[ObjectSupport],
    unreadable: &BTreeSet<PathBuf>,
    restrict_unknown: bool,
) -> Vec<FileSupport> {
    let policies: BTreeMap<_, _> = objects
        .iter()
        .map(|object| (object.object_id.clone(), object.state))
        .collect();
    files
        .into_iter()
        .map(|(path, mut objects)| {
            objects.sort();
            objects.dedup();
            let locked = objects
                .iter()
                .any(|id| policies.get(id) == Some(&State::Locked));
            let unknown = unreadable.contains(&path)
                || objects
                    .iter()
                    .any(|id| policies.get(id) == Some(&State::Unknown));
            let mixed = locked
                && objects.iter().any(|id| {
                    matches!(
                        policies.get(id),
                        Some(State::Unrestricted | State::EditableWithSupport)
                    )
                });
            FileSupport {
                reason: if unknown {
                    FileReason::Unavailable
                } else if mixed {
                    FileReason::MixedObjects
                } else if locked {
                    FileReason::SupportRule
                } else {
                    FileReason::NoSupportRestriction
                },
                read_only: locked || unreadable.contains(&path) || (unknown && restrict_unknown),
                path,
                objects,
                mixed,
                unknown,
            }
        })
        .collect()
}
