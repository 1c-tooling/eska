//! Explicit artifact profiles match native external models while ordinary object adapters stay internal.

use super::*;
use bsl_metadata::MdoType;
use eska::project_core::metadata_parser::{self, PropertiesMode};

/// Real external metadata has an object field, typed section column and stable source identities.
fn xml(kind: &str) -> String {
    format!(
        "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" version=\"2.20\"><{kind} uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>Owner</Name></Properties><ChildObjects><Attribute uuid=\"20000000-0000-4000-8000-000000000001\"><Properties><Name>Enabled</Name><Type><v8:Type>xs:boolean</v8:Type></Type></Properties></Attribute><TabularSection uuid=\"30000000-0000-4000-8000-000000000001\"><Properties><Name>Rows</Name></Properties><ChildObjects><Attribute uuid=\"40000000-0000-4000-8000-000000000001\"><Properties><Name>Value</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Attribute></ChildObjects></TabularSection></ChildObjects></{kind}></MetaDataObject>"
    )
}

/// EPF/ERF fields and identities exactly match the pinned native parser with the actual external kind.
#[test]
fn explicit_external_profiles_match_native_models_and_do_not_change_internal_mapping() {
    for (kind, internal) in [
        (ExternalObjectKind::DataProcessor, MdoType::DataProcessor),
        (ExternalObjectKind::Report, MdoType::Report),
    ] {
        let text = xml(kind.element_name());
        let descriptor = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
        let current = external_object(&descriptor, kind).unwrap();
        let native = bsl_metadata::parse_metadata_object_from_texts(kind.mdo_type(), &text, None)
            .expect("native external model");
        assert_eq!(current, native);
        assert_eq!(current.mdo_type, kind.mdo_type());
        assert_eq!(current.attributes[0].name, "Enabled");
        assert_eq!(current.tabular_sections[0].name(), "Rows");
        assert_eq!(
            metadata_object(&descriptor, Vec::new()).unwrap().mdo_type,
            internal
        );
    }
}

/// The explicit discriminator cannot rescue a wrong family, unrequested fields or invalid source UUID.
#[test]
fn external_profiles_refuse_wrong_incomplete_and_invalid_descriptors() {
    for kind in [
        ExternalObjectKind::DataProcessor,
        ExternalObjectKind::Report,
    ] {
        let wrong = if kind == ExternalObjectKind::Report {
            "ExternalDataProcessor"
        } else {
            "ExternalReport"
        };
        let descriptor = metadata_parser::parse(&xml(wrong), None, PropertiesMode::All).unwrap();
        assert!(matches!(
            external_object(&descriptor, kind).unwrap_err().issue,
            AdapterIssue::WrongKind { .. }
        ));
        let descriptor =
            metadata_parser::parse(&xml(kind.element_name()), None, PropertiesMode::Summary)
                .unwrap();
        assert_eq!(
            external_object(&descriptor, kind).unwrap_err().issue,
            AdapterIssue::PropertiesNotLoaded
        );
        let text =
            xml(kind.element_name()).replace("10000000-0000-4000-8000-000000000001", "invalid");
        let descriptor = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
        assert_eq!(
            external_object(&descriptor, kind).unwrap_err().issue,
            AdapterIssue::InvalidUuid
        );
    }
}
