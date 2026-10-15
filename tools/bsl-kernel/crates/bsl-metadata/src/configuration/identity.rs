//! A canonical configuration header retains its supplied source UUID without generating another one.

#[cfg(test)]
mod tests;

use super::{Configuration, FxHashMap, HashMap, Uuid};

impl Configuration {
    /// Construct an actual CF/CFE identity with empty collections supplied separately by its producer.
    ///
    /// External artifacts are metadata objects and must not use this configuration constructor.
    pub fn with_identity(uuid: Uuid, name: impl Into<String>) -> Self {
        Self {
            uuid,
            name: name.into(),
            common_modules: Vec::new(),
            metadata_objects: Vec::new(),
            registers: Vec::new(),
            event_subscriptions: Vec::new(),
            defined_types: Vec::new(),
            session_parameters: Vec::new(),
            scheduled_jobs: Vec::new(),
            roles: Vec::new(),
            subsystems: Vec::new(),
            uri_to_module: HashMap::new(),
            uri_lower_to_common_module: HashMap::new(),
            name_to_common_module: FxHashMap::default(),
            name_to_register: FxHashMap::default(),
            name_to_event_subscription: FxHashMap::default(),
            name_to_defined_type: FxHashMap::default(),
            name_to_scheduled_job: FxHashMap::default(),
            name_to_role: FxHashMap::default(),
            name_to_http_service: FxHashMap::default(),
            name_to_web_service: FxHashMap::default(),
            name_to_integration_service: FxHashMap::default(),
            metadata_objects_by_key: FxHashMap::default(),
            recorders_by_register: HashMap::new(),
            use_managed_form_in_ordinary_application: false,
            use_ordinary_form_in_managed_application: false,
            http_services: Vec::new(),
            web_services: Vec::new(),
            integration_services: Vec::new(),
        }
    }
}
