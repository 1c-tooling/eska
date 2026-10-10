//! A custom location request returns standard UTF-16 Location data without lazy filesystem reads.

use eska::project_core::{
    metadata_model::PropertyKey,
    source_format::{NativeUuid, SourcePath},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{AnalysisSession, MetadataLocationError};

use super::{documents, dto, protocol};

pub const METHOD: &str = "eska/metadata/locate";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Params {
    pub snapshot: protocol::Snapshot,
    source: SourcePath,
    uuid: NativeUuid,
    property: Option<Key>,
}

/// Qualified names are strict wire data; arbitrary native properties share one lookup rule.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Key {
    namespace: Option<String>,
    name: String,
}

/// Coordinates belong to the current parsed bytes, even if the disk file changed or disappeared.
pub fn locate(session: &AnalysisSession, id: &str, params: &Params) -> Result<Value, Value> {
    let key = params.property.as_ref().map(|key| PropertyKey {
        namespace: key.namespace.clone(),
        name: key.name.clone(),
    });
    let target = session
        .metadata_location(
            &params.source,
            session.revision(),
            &params.uuid,
            key.as_ref(),
        )
        .map_err(|error| failure(session, id, params, &error))?;
    let uri = url::Url::from_file_path(session.root().join(target.source.as_path()))
        .map_err(|()| protocol::error_value(-32603, "path_encoding_unsupported", Value::Null))?;
    Ok(
        json!({"snapshot":dto::snapshot(session,id),"source":target.source,"source_sha256":target.source_sha256,
        "origin":dto::origin(target.origin),"uuid":target.uuid,"native_path":target.native_path,
        "byte_range":target.byte_range,"location":{"uri":uri.as_str(),"range":target.range},
        "resources":session.resources()}),
    )
}

/// Core failures remain structured and locale-independent without exposing model or raw text.
fn failure(
    session: &AnalysisSession,
    id: &str,
    params: &Params,
    error: &MetadataLocationError,
) -> Value {
    let (code, kind) = match error {
        MetadataLocationError::Request(error) => return documents::request_error(error),
        MetadataLocationError::Pending => (-32000, "metadata_input_pending"),
        MetadataLocationError::Unavailable(_) => (-32000, "metadata_input_unavailable"),
        MetadataLocationError::ObjectNotFound => (-32602, "metadata_object_not_found"),
        MetadataLocationError::AmbiguousObject => (-32000, "metadata_object_ambiguous"),
        MetadataLocationError::PropertiesNotLoaded => (-32000, "metadata_properties_not_loaded"),
        MetadataLocationError::PropertyNotFound => (-32602, "metadata_property_not_found"),
        MetadataLocationError::AmbiguousProperty => (-32000, "metadata_property_ambiguous"),
        MetadataLocationError::InconsistentCoordinates(_) => {
            (-32603, "metadata_coordinates_inconsistent")
        }
    };
    let mut details = json!({"snapshot":dto::snapshot(session,id),"source":params.source});
    if let MetadataLocationError::Unavailable(failure) = error {
        details["failure"] = dto::failure_value(failure);
    }
    protocol::error_value(code, kind, details)
}
