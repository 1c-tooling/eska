//! Identity construction keeps the native serialization shape and initializes the same empty indexes.

use super::*;

/// Exact supplied identity survives deterministic construction, collection enrollment and serialization.
#[test]
fn canonical_configuration_identity_is_exact_and_serialization_is_deterministic() {
    let uuid = Uuid::from_u128(0x10000000000040008000000000000001);
    let mut current = Configuration::with_identity(uuid, "ТекущаяКонфигурация");
    let equal = Configuration::with_identity(uuid, "ТекущаяКонфигурация");
    assert_eq!(current, equal);
    assert_eq!(current.uuid(), &uuid);
    assert_eq!(current.name(), "ТекущаяКонфигурация");
    assert_eq!(serde_json::to_value(&current).unwrap(), serde_json::to_value(equal).unwrap());
    assert!(current.metadata_objects().is_empty());
    let kind = crate::MdoType::Catalog;
    current.add_metadata_object(crate::MetadataObject::new(kind, "Товары"));
    assert!(current.find_metadata_object(kind, "Товары").is_some());
    current.set_use_managed_form_in_ordinary_application(true);
    assert!(current.use_managed_form_in_ordinary_application());
    let value = serde_json::to_value(&current).unwrap();
    assert_eq!(value["uuid"], uuid.to_string());
    assert_eq!(value["name"], "ТекущаяКонфигурация");
    let mut expected = serde_json::to_value(Configuration::new("Native")).unwrap();
    let expected = expected.as_object_mut().unwrap();
    let actual = value.as_object().unwrap();
    assert_eq!(actual.keys().collect::<Vec<_>>(), expected.keys().collect::<Vec<_>>());
}
