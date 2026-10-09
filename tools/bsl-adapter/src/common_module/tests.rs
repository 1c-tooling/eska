use super::*;
use bsl_metadata::traits::MdObject;
use eska::project_core::metadata_parser::{self, PropertiesMode};

/// Native text feeds only the shared project parser, never the adapter itself.
fn xml(properties: &str) -> String {
    format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><CommonModule uuid="20000000-0000-4000-8000-000000000001"><Properties><Name>Tools</Name>{properties}</Properties></CommonModule></MetaDataObject>"#
    )
}

/// The original kernel parser is a reference for flags and optional-value semantics.
#[test]
fn projected_module_matches_native_kernel_flags_and_identity() {
    for fields in [
        "",
        "<Server>true</Server><Global>false</Global><ClientManagedApplication>true</ClientManagedApplication><ClientOrdinaryApplication>false</ClientOrdinaryApplication><ExternalConnection>true</ExternalConnection><ServerCall>true</ServerCall><Privileged>false</Privileged><ReturnValuesReuse>DuringSession</ReturnValuesReuse>",
        "<ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>30000000-0000-4000-8000-000000000001</ExtendedConfigurationObject>",
    ] {
        let text = xml(fields);
        let parsed = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
        let converted = common_module(&parsed.objects[0]).unwrap();
        let native = bsl_metadata::xml_parser::parse_common_module_xml(&text).unwrap();
        assert_eq!(converted, native);
        assert_eq!(
            converted.uuid().to_string(),
            "20000000-0000-4000-8000-000000000001"
        );
    }
}

/// An unset flag inherits, whereas explicit false clears the base's flag.
#[test]
fn missing_flags_are_not_materialized_as_false() {
    let make_base = || {
        CommonModule::builder()
            .uuid("40000000-0000-4000-8000-000000000001".parse().unwrap())
            .name("Tools")
            .server(true)
            .global(true)
            .server_call(true)
            .build()
    };
    let parsed = metadata_parser::parse(&xml(""), None, PropertiesMode::All).unwrap();
    let mut inherited = make_base();
    inherited.apply_extension_overlay(&common_module(&parsed.objects[0]).unwrap());
    assert!(inherited.is_server() && inherited.is_global() && inherited.is_server_call());
    let parsed = metadata_parser::parse(
        &xml("<Server>false</Server><Global>false</Global><ServerCall>false</ServerCall>"),
        None,
        PropertiesMode::All,
    )
    .unwrap();
    let mut cleared = make_base();
    cleared.apply_extension_overlay(&common_module(&parsed.objects[0]).unwrap());
    assert!(!cleared.is_server() && !cleared.is_global() && !cleared.is_server_call());
}

/// A summary cannot provide a complete module's execution context.
#[test]
fn summary_mode_is_rejected() {
    let parsed =
        metadata_parser::parse(&xml("<Server>true</Server>"), None, PropertiesMode::Summary)
            .unwrap();
    assert_eq!(
        common_module(&parsed.objects[0]).unwrap_err().issue,
        AdapterIssue::PropertiesNotLoaded
    );
}

/// Invalid scalar/record data is rejected at its exact original source location.
#[test]
fn invalid_and_duplicate_flags_are_not_guessed() {
    for value in ["TRUE", "1", "", "<Value>true</Value>"] {
        let text = xml(&format!("<Server>{value}</Server>"));
        let parsed = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
        let error = common_module(&parsed.objects[0]).unwrap_err();
        assert_eq!(
            error.issue,
            AdapterIssue::InvalidProperty {
                name: "Server".into()
            }
        );
        assert!(text[error.range].starts_with("<Server>"));
    }
    let text = xml("<Server>true</Server><Server>false</Server>");
    let parsed = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
    let error = common_module(&parsed.objects[0]).unwrap_err();
    assert_eq!(
        error.issue,
        AdapterIssue::DuplicateProperty {
            name: "Server".into()
        }
    );
    assert_eq!(&text[error.range], "<Server>false</Server>");
}

/// Foreign fields with the same local name cannot override a platform flag.
#[test]
fn qualified_property_keys_ignore_prefix_spelling_and_reject_namespace_collisions() {
    let text = xml(
        "<other:Server xmlns:other='urn:other'>false</other:Server><m:Server xmlns:m='http://v8.1c.ru/8.3/MDClasses'>true</m:Server>",
    );
    let parsed = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
    assert!(common_module(&parsed.objects[0]).unwrap().is_server());
}

/// Missing identity never reaches the kernel builder's random-UUID default.
#[test]
fn invalid_uuid_and_unsupported_ownership_are_explicit_failures() {
    let text = xml("").replace("20000000-0000-4000-8000-000000000001", "invalid");
    let parsed = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
    assert_eq!(
        common_module(&parsed.objects[0]).unwrap_err().issue,
        AdapterIssue::InvalidUuid
    );
    for fields in [
        "<ObjectBelonging>Future</ObjectBelonging>",
        "<ExtendedConfigurationObject>invalid</ExtendedConfigurationObject>",
        "<ReturnValuesReuse>Future</ReturnValuesReuse>",
    ] {
        let parsed = metadata_parser::parse(&xml(fields), None, PropertiesMode::All).unwrap();
        assert!(common_module(&parsed.objects[0]).is_err());
    }
}
