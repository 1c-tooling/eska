//! Explicit protocol DTO conversion; internal serde representations are not the wire format.

use crate::cli::localization::{Locale, Localizer};
use crate::project::{
    configurator::{ChildrenState, TreeLabel, TreeNode},
    designer_source::{SourceLocation, SourceRole},
    metadata_model::{
        CollectionKind, MetadataKind, MetadataProperty, MetadataValue, NodeId, ValueIssue,
    },
    metadata_parser::{Diagnostic, LocatedProperty, ParseIssue},
    metadata_workspace::{
        ObjectSummary,
        search::{IndexProgress, IndexState, MatchRank, SearchHit},
    },
};
use serde_json::{Value, json};

pub(super) struct Labels {
    ru: Localizer,
    en: Localizer,
}
impl Labels {
    /// Load both embedded locales once, independently of the client's preferred locale.
    pub(super) fn new() -> Result<Self, crate::cli::localization::LocalizationError> {
        Ok(Self {
            ru: Localizer::try_new(Locale::RuRu)?,
            en: Localizer::try_new(Locale::EnUs)?,
        })
    }
    /// Project one node without serializing its potentially large children list.
    pub(super) fn node(&self, value: &TreeNode) -> Value {
        let label = match &value.label {
            TreeLabel::Name(text) => json!({"kind":"name","text":text}),
            TreeLabel::Key(key) => {
                json!({"kind":"key","key":key,"translations":{"ru-RU":self.ru.text(key),"en-US":self.en.text(key)}})
            }
        };
        let state = match value.state {
            ChildrenState::Empty => "empty",
            ChildrenState::NonEmpty => "non_empty",
            ChildrenState::Unloaded => "unloaded",
            ChildrenState::Error => "error",
        };
        json!({"id":node_id(&value.id),"metadataKind":value.metadata_kind.map(MetadataKind::as_str),"parent":value.parent.as_ref().map(node_id),"label":label,"state":state,"expandedByDefault":value.expanded_by_default,"rootSection":value.root_section,"diagnostics":value.diagnostics.iter().map(diagnostic).collect::<Vec<_>>()})
    }
}

/// Node identities are discriminated by role, never by a localized label or source path.
pub(super) fn node_id(value: &NodeId) -> Value {
    match value {
        NodeId::Object(id) => json!({"kind":"object","objectId":id.as_str()}),
        NodeId::Module { owner, role } => {
            json!({"kind":"module","owner":owner.as_str(),"role":role.as_str()})
        }
        NodeId::Collection {
            owner,
            kind: collection,
        } => {
            let collection = match collection {
                CollectionKind::Metadata(kind) => {
                    json!({"kind":"metadata","metadataKind":kind.as_str()})
                }
                CollectionKind::Common => json!({"kind":"common"}),
                CollectionKind::Modules => json!({"kind":"modules"}),
                CollectionKind::Unsupported => json!({"kind":"unsupported"}),
            };
            json!({"kind":"collection","owner":owner.as_str(),"collection":collection})
        }
    }
}

/// Reuse the established reversible native path encoding.
pub(super) fn path(value: &std::path::Path) -> Value {
    let (value, encoding) = crate::cli::encoding::json_path(value.as_os_str());
    json!({"value":value,"encoding":encoding})
}

/// Unknown reference UUIDs remain null until that descriptor is loaded.
pub(super) fn object(value: &ObjectSummary) -> Value {
    json!({"objectId":value.id.as_str(),"metadataKind":value.kind.as_str(),"name":value.name,"parent":value.parent.as_ref().map(crate::project::metadata_model::ObjectId::as_str),"uuid":value.uuid,"synonyms":value.synonyms})
}

/// Parser diagnostics preserve structural context without including XML source text.
pub(super) fn diagnostic(value: &Diagnostic) -> Value {
    let (code, details) = match &value.issue {
        ParseIssue::UnsupportedRoot => ("unsupported_root", json!({})),
        ParseIssue::UnknownKind { namespace, name } => {
            ("unknown_kind", json!({"namespace":namespace,"name":name}))
        }
        ParseIssue::InvalidObject => ("invalid_object", json!({})),
        ParseIssue::DuplicateIdentity(id) => {
            ("duplicate_identity", json!({"objectId":id.as_str()}))
        }
        ParseIssue::UnsupportedValue => ("unsupported_value", json!({})),
    };
    json!({"code":code,"range":{"start":value.range.start,"end":value.range.end},"details":details})
}

/// Preserve property order, namespace-aware keys and UTF-8 byte ranges.
pub(super) fn property(value: &LocatedProperty) -> Value {
    let mut result = field(&value.property);
    result["range"] = json!({"start":value.range.start,"end":value.range.end});
    result
}

/// Nested record fields retain repeats and qualifiers rather than becoming a JSON map.
fn field(value: &MetadataProperty) -> Value {
    let content = match &value.value {
        MetadataValue::Text(text) => json!({"kind":"text","text":text}),
        MetadataValue::Localized(items) => json!({"kind":"localized","items":items}),
        MetadataValue::Record(fields) => {
            json!({"kind":"record","fields":fields.iter().map(field).collect::<Vec<_>>()})
        }
        MetadataValue::Unsupported(issue) => {
            json!({"kind":"unsupported","issue":match issue { ValueIssue::MixedContent => "mixed_content", ValueIssue::InvalidLocalizedText => "invalid_localized_text" }})
        }
    };
    json!({"key":value.key,"qualifiers":value.qualifiers.iter().map(|(key,value)|json!({"key":key,"value":value})).collect::<Vec<_>>(),"value":content})
}

/// Module roles and inline logical addresses stay separate from physical paths.
pub(super) fn source(value: &SourceLocation) -> Value {
    let role = match value.role {
        SourceRole::Descriptor => json!({"kind":"descriptor"}),
        SourceRole::Payload => json!({"kind":"payload"}),
        SourceRole::Module(role) => json!({"kind":"module","role":role.as_str()}),
    };
    json!({"path":path(&value.path),"role":role,"inline":value.inline.iter().map(|part| json!({"metadataKind":part.kind.as_str(),"name":part.name})).collect::<Vec<_>>()})
}

/// Explicit state mapping prevents internal Rust variant names from leaking into clients.
pub(super) fn progress(value: &IndexProgress) -> Value {
    let state = match value.state {
        IndexState::NotStarted => "not_started",
        IndexState::Building => "building",
        IndexState::Cancelled => "cancelled",
        IndexState::Ready => "ready",
        IndexState::Incomplete => "incomplete",
    };
    json!({"state":state,"indexedObjects":value.indexed_objects,"pendingDescriptors":value.pending_descriptors,"failedDescriptors":value.failed_descriptors})
}

/// Scope and generation are carried once in the enclosing response.
pub(super) fn hit(value: &SearchHit) -> Value {
    let rank = match value.rank {
        MatchRank::ExactName => "exact_name",
        MatchRank::ExactSynonym => "exact_synonym",
        MatchRank::PrefixName => "prefix_name",
        MatchRank::PrefixSynonym => "prefix_synonym",
        MatchRank::SubstringName => "substring_name",
        MatchRank::SubstringSynonym => "substring_synonym",
    };
    json!({"objectId":value.object.as_str(),"node":node_id(&value.node),"metadataKind":value.kind.as_str(),"name":value.name,"synonyms":value.synonyms,"ancestry":value.ancestry.iter().map(node_id).collect::<Vec<_>>(),"rank":rank})
}
