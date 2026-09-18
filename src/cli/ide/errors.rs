//! Domain errors expose structured reasons, never source contents or debug strings.

use super::{
    dto,
    envelope::{domain, error},
};
use crate::project::{
    designer_source::SourceError,
    discovery::{ContextDiscoveryError, DiscoveryError},
    metadata_parser::{LoadError, ParseError},
    metadata_workspace::WorkspaceError,
};
use serde_json::{Value, json};

/// Opening errors distinguish manifest discovery/selection from source and root failures.
pub(super) fn opening(value: &WorkspaceError) -> Value {
    let reason = match value {
        WorkspaceError::Source(SourceError::Discovery(ContextDiscoveryError::Project(
            DiscoveryError::NotFound { .. },
        ))) => "manifest_missing",
        WorkspaceError::Source(SourceError::Discovery(_)) => "manifest_invalid",
        WorkspaceError::Source(SourceError::Selection(_)) => "selection_invalid",
        WorkspaceError::Source(
            SourceError::InvalidRoot { .. }
            | SourceError::MissingRoot
            | SourceError::AmbiguousRoot { .. },
        )
        | WorkspaceError::Load(_)
        | WorkspaceError::Tree(_) => "root_invalid",
        _ => "source_invalid",
    };
    domain("project_open_failed", json!({"reason":reason}))
}

/// Map each public core error without depending on Display or Debug output.
pub(super) fn workspace(value: &WorkspaceError) -> Value {
    match value {
        WorkspaceError::Source(value) | WorkspaceError::Load(LoadError::Source(value)) => {
            source(value)
        }
        WorkspaceError::Load(LoadError::Parse { path, source }) => parse(path, source),
        WorkspaceError::Load(LoadError::MissingDescriptor(id)) => domain(
            "source_missing",
            json!({"objectId":id.as_str(),"reason":"not_file"}),
        ),
        WorkspaceError::Load(LoadError::ObjectNotFound(id)) | WorkspaceError::UnknownObject(id) => {
            domain("unknown_object", json!({"objectId":id.as_str()}))
        }
        WorkspaceError::UnknownNode(id) => domain("unknown_node", json!({"node":dto::node_id(id)})),
        WorkspaceError::MissingSource(id) => domain(
            "source_missing",
            json!({"node":dto::node_id(id),"reason":"not_file"}),
        ),
        WorkspaceError::InvalidChangedPath(path) => domain(
            "source_invalid",
            json!({"path":dto::path(path),"reason":"outside_source"}),
        ),
        WorkspaceError::SourceChanged(path) => {
            domain("source_changed", json!({"path":dto::path(path)}))
        }
        WorkspaceError::StaleGeneration { expected, actual } => domain(
            "stale_generation",
            json!({"expected":actual.to_string(),"actual":expected.to_string()}),
        ),
        WorkspaceError::GenerationExhausted => domain("generation_exhausted", json!({})),
        WorkspaceError::UnknownProject(_) => domain("unknown_project", json!({})),
        WorkspaceError::Tree(_) | WorkspaceError::BrokenAncestry(_) => error(-32603),
    }
}

/// XML errors identify only the source address and the bounded parser category.
fn parse(path: &std::path::Path, value: &ParseError) -> Value {
    let reason = match value {
        ParseError::TooLarge => "too_large",
        ParseError::TooDeep => "too_deep",
        ParseError::Envelope(_) | ParseError::Xml(_) => "malformed",
        ParseError::Invalid(_) => "invalid_metadata",
    };
    domain(
        "xml_invalid",
        json!({"path":dto::path(path),"reason":reason}),
    )
}

/// Preserve containment, IO, missing-file and identity distinctions at the boundary.
fn source(value: &SourceError) -> Value {
    let (kind, reason, path) = match value {
        SourceError::Io { path, source } => (
            if source.kind() == std::io::ErrorKind::NotFound {
                "source_missing"
            } else {
                "source_invalid"
            },
            "io",
            Some(path),
        ),
        SourceError::OutsideSource { path } => ("source_invalid", "outside_source", Some(path)),
        SourceError::NotFile { path } => ("source_invalid", "not_file", Some(path)),
        SourceError::DescriptorTooLarge { path } => ("source_invalid", "too_large", Some(path)),
        SourceError::InvalidRoot { path, .. } | SourceError::InvalidMetadata { path, .. } => {
            ("source_invalid", "invalid_identity", Some(path))
        }
        SourceError::Parse { path, source } => return parse(path, source),
        SourceError::MissingRoot => ("source_missing", "not_file", None),
        SourceError::AmbiguousRoot { .. } | SourceError::AmbiguousLocation { .. } => {
            ("source_invalid", "ambiguous", None)
        }
        SourceError::InvalidIdentity { .. } => ("source_invalid", "invalid_identity", None),
        SourceError::UnsupportedLocation { .. } | SourceError::UnsupportedModule { .. } => {
            ("source_invalid", "unsupported", None)
        }
        SourceError::Discovery(_) | SourceError::Selection(_) => return error(-32603),
    };
    let mut details = json!({"reason":reason});
    if let Some(path) = path {
        details["path"] = dto::path(path);
    }
    domain(kind, details)
}
