use super::*;
use eska::project_core::metadata_parser::{self, ParsedDescriptor, PropertiesMode};

const UUID: &str = "a1000000-0000-4000-8000-000000000001";

/// Real native roots use the shared parser while their UI form defaults remain independent.
fn descriptor(properties: &str, mode: PropertiesMode) -> ParsedDescriptor {
    let text = format!(
        "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><SettingsStorage uuid='{UUID}'><Properties><Name>PreferencesЯ</Name>{properties}</Properties></SettingsStorage></MetaDataObject>"
    );
    metadata_parser::parse(&text, None, mode).unwrap()
}

/// Read the current owner exactly as canonical SDK projection does.
fn converted(properties: &str) -> Result<SettingsStorage, AdapterError> {
    settings_storage(&descriptor(properties, PropertiesMode::All).objects[0])
}

/// A storage owns a manager identity, without requiring nonexistent record fields or runtime flags.
#[test]
fn settings_storage_preserves_native_identity() {
    let value =
        converted("<DefaultSaveForm/><DefaultLoadForm/><AuxiliarySaveForm/><AuxiliaryLoadForm/>")
            .unwrap();
    assert_eq!(value.uuid().to_string(), UUID);
    assert_eq!(value.name(), "PreferencesЯ");
    assert_eq!(value.object_belonging(), ObjectBelonging::Unknown);
    assert!(value.estimated_heap_size() >= value.name().len());
    assert_eq!(converted("").unwrap(), value);
}

/// Borrowed manager modules use exact original UUIDs even after an earlier contribution was merged.
#[test]
fn settings_storage_adoption_requires_original_identity() {
    let mut base = converted("").unwrap();
    let overlay = SettingsStorage::from_parts(
        "a1000000-0000-4000-8000-000000000002".parse().unwrap(),
        "preferencesя".into(),
        ObjectBelonging::Adopted,
        Some(base.uuid()),
    );
    assert!(overlay.adopts(&base));
    base.apply_extension_overlay(&overlay);
    assert_eq!(base.uuid(), overlay.uuid());
    assert_eq!(base.name(), "PreferencesЯ");
    assert_eq!(base.object_belonging(), ObjectBelonging::Adopted);
    let original: bsl_metadata::Uuid = UUID.parse().unwrap();
    for (name, extends, expected) in [
        ("PreferencesЯ", original, true),
        ("Other", original, false),
        ("PreferencesЯ", overlay.uuid(), false),
    ] {
        let contribution = SettingsStorage::from_parts(
            "a1000000-0000-4000-8000-000000000003".parse().unwrap(),
            name.into(),
            ObjectBelonging::Adopted,
            Some(extends),
        );
        assert_eq!(contribution.adopts(&base), expected);
    }
}

/// A borrowed declaration cannot fall back to an own manager when its source identity is malformed.
#[test]
fn settings_storage_rejects_inconsistent_or_duplicate_ownership() {
    for properties in [
        "<ObjectBelonging>Adopted</ObjectBelonging>",
        "<ExtendedConfigurationObject>a1000000-0000-4000-8000-000000000001</ExtendedConfigurationObject>",
        "<ObjectBelonging>Own</ObjectBelonging><ExtendedConfigurationObject>a1000000-0000-4000-8000-000000000001</ExtendedConfigurationObject>",
        "<ObjectBelonging>Unknown</ObjectBelonging>",
        "<ObjectBelonging>Own</ObjectBelonging><ObjectBelonging>Own</ObjectBelonging>",
        "<ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>invalid</ExtendedConfigurationObject>",
    ] {
        assert!(converted(properties).is_err(), "{properties}");
    }
    let borrowed = converted(&format!(
        "<ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>{UUID}</ExtendedConfigurationObject>"
    ))
    .unwrap();
    assert!(borrowed.adopts(&converted("").unwrap()));
    assert_eq!(
        converted("<ObjectBelonging>Own</ObjectBelonging>")
            .unwrap()
            .object_belonging(),
        ObjectBelonging::Own
    );
}

/// Missing property capture, invalid UUIDs and wrong root kinds stay explicit adapter failures.
#[test]
fn settings_storage_rejects_unavailable_or_wrong_identity() {
    let summary = descriptor("", PropertiesMode::Summary);
    assert_eq!(
        settings_storage(&summary.objects[0]).unwrap_err().issue,
        AdapterIssue::PropertiesNotLoaded
    );
    let text = "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><SettingsStorage uuid='invalid'><Properties><Name>PreferencesЯ</Name></Properties></SettingsStorage></MetaDataObject>";
    let parsed = metadata_parser::parse(text, None, PropertiesMode::All).unwrap();
    assert_eq!(
        settings_storage(&parsed.objects[0]).unwrap_err().issue,
        AdapterIssue::InvalidUuid
    );
    let text = format!(
        "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><CommonModule uuid='{UUID}'><Properties><Name>Other</Name></Properties></CommonModule></MetaDataObject>"
    );
    let parsed = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
    assert_eq!(
        settings_storage(&parsed.objects[0]).unwrap_err().issue,
        AdapterIssue::WrongKind {
            actual: MetadataKind::CommonModule,
        }
    );
}
