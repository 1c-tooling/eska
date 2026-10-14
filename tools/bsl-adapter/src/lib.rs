//! Canonical metadata adapters depend on shared logical properties, never exported XML or an IDE.

mod common_module;
mod defined_type;
mod event_subscription;
mod form;
mod http_service;
mod identity;
mod integration_service;
mod object;
mod properties;
mod register;
mod role;
mod scheduled_job;
mod simple_object;
mod structural_descriptor;
mod subsystem;
mod type_description;
mod web_service;

pub use common_module::common_module;
pub use defined_type::defined_type;
pub use event_subscription::event_subscription;
pub use form::form;
pub use http_service::http_service;
pub use integration_service::integration_service;
pub use object::metadata_object;
pub use register::register;
pub use role::role;
pub use scheduled_job::scheduled_job;
pub use simple_object::{constant_object, enum_object};
pub use structural_descriptor::structural_descriptor;
pub use subsystem::subsystem;
pub use type_description::attribute_type;
pub use web_service::web_service;

use std::ops::Range;

use eska::project_core::MetadataKind;
use eska::project_core::metadata_model::PropertyKey;

/// Coordinates belong to the exact XML or YAML snapshot supplied by the caller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterError {
    pub issue: AdapterIssue,
    pub range: Range<usize>,
}

/// Failure is explicit rather than a guessed default value or a silently incomplete model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdapterIssue {
    PropertiesNotLoaded,
    WrongKind { actual: MetadataKind },
    InvalidUuid,
    DuplicateProperty { name: String },
    InvalidProperty { name: String },
    UnsupportedOwnership { value: String },
    MissingProperty { name: String },
    UnsupportedType { key: PropertyKey },
    UnsupportedTypeField { key: PropertyKey },
    UnresolvedTypeName { text: String },
    MissingRoot,
    IncompleteDescriptor,
    UnsupportedStandardAttribute { name: String },
    UnsupportedPropertyValue { name: String, value: String },
}
