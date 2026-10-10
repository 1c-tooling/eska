//! Display capabilities never change kernel candidate order or caller authority.

use crate::{CallParameter, CallSignature, CallSignatures};

use super::*;

/// Parameter options are read from their own standard nested properties, independently of hover.
#[test]
fn signature_capabilities_follow_nested_flags_and_documentation_preference_order() {
    let plain = Capabilities::negotiate(&Value::Null);
    assert_eq!(plain.documentation, Format::Plaintext);
    assert!(
        !plain.parameters.label_offsets
            && !plain.parameters.active_per_signature
            && !plain.parameters.no_active
    );
    assert!(plain.provider().get("retriggerCharacters").is_none());
    let options = Capabilities::negotiate(&json!({"capabilities":{"textDocument":{
        "hover":{"contentFormat":["plaintext"]},"signatureHelp":{"contextSupport":true,
            "signatureInformation":{"documentationFormat":["unknown","markdown","plaintext"],
                "parameterInformation":{"labelOffsetSupport":true},"activeParameterSupport":true,
                "noActiveParameterSupport":true}}}}}));
    assert_eq!(options.documentation, Format::Markdown);
    assert!(
        options.parameters.label_offsets
            && options.parameters.active_per_signature
            && options.parameters.no_active
    );
    assert_eq!(options.provider()["retriggerCharacters"], json!([")"]));
    let malformed =
        Capabilities::negotiate(&json!({"capabilities":{"textDocument":{"signatureHelp":{
        "contextSupport":"true","signatureInformation":{"documentationFormat":"markdown",
            "parameterInformation":{"labelOffsetSupport":"true"},"activeParameterSupport":[],
            "noActiveParameterSupport":1}}}}}));
    assert_eq!(malformed.documentation, Format::Plaintext);
    assert!(
        !malformed.parameters.label_offsets
            && !malformed.parameters.active_per_signature
            && !malformed.parameters.no_active
    );
}

/// A default containing non-BMP text changes later UTF-16 offsets without changing the parameter name.
#[test]
fn parameter_labels_use_generated_signature_utf16_and_safe_authored_documentation() {
    let first = "[Арг = \"😀\"]";
    let second = "Арг";
    let label = format!("Функция Арг({first}, {second})");
    let start = label.find('(').unwrap() + 1;
    let end = start + first.len();
    let second_start = end + 2;
    let signature = CallSignature {
        label: label.clone(),
        documentation: Some("**doc** [x](command:run)".into()),
        parameters: vec![
            CallParameter {
                label: first.into(),
                documentation: Some("<tag>".into()),
                byte_range: u32::try_from(start).unwrap()..u32::try_from(end).unwrap(),
            },
            CallParameter {
                label: second.into(),
                documentation: None,
                byte_range: u32::try_from(second_start).unwrap()
                    ..u32::try_from(second_start + second.len()).unwrap(),
            },
        ],
    };
    let help = CallSignatures {
        signatures: vec![signature],
        active_signature: Some(0),
        active_parameter: Some(1),
    };
    let plain = Capabilities::negotiate(&Value::Null);
    let rendered = presentation::render(help.clone(), plain).unwrap();
    assert_eq!(rendered["signatures"][0]["parameters"][0]["label"], first);
    assert_eq!(
        rendered["signatures"][0]["documentation"]["value"],
        "**doc** [x](command:run)"
    );
    assert!(rendered["signatures"][0].get("activeParameter").is_none());
    let mut offsets = plain;
    offsets.parameters.label_offsets = true;
    offsets.parameters.active_per_signature = true;
    offsets.documentation = Format::Markdown;
    let rendered = presentation::render(help, offsets).unwrap();
    let expected_start = label[..second_start].encode_utf16().count();
    assert_ne!(expected_start, second_start);
    assert_eq!(
        rendered["signatures"][0]["parameters"][1]["label"],
        json!([
            expected_start,
            expected_start + second.encode_utf16().count()
        ])
    );
    assert_eq!(rendered["signatures"][0]["activeParameter"], 1);
    assert_eq!(rendered["activeParameter"], 1);
    assert_eq!(
        rendered["signatures"][0]["parameters"][0]["documentation"]["value"],
        "\\<tag\\>"
    );
    assert_eq!(
        rendered["signatures"][0]["documentation"]["value"],
        "\\*\\*doc\\*\\* \\[x\\]\\(command\\:run\\)"
    );
}

/// Unsupported null selection is omitted, and only the selected signature gets a per-item field.
#[test]
fn no_active_parameter_is_null_only_for_negotiated_clients() {
    let signature = CallSignature {
        label: "Вызов()".into(),
        documentation: None,
        parameters: vec![],
    };
    let help = CallSignatures {
        signatures: vec![signature.clone(), signature],
        active_signature: Some(1),
        active_parameter: None,
    };
    let legacy = Capabilities::negotiate(&Value::Null);
    let plain = presentation::render(help.clone(), legacy).unwrap();
    assert!(plain.get("activeParameter").is_none());
    let mut supported = legacy;
    supported.parameters.no_active = true;
    supported.parameters.active_per_signature = true;
    let result = presentation::render(help, supported).unwrap();
    assert_eq!(result["activeParameter"], Value::Null);
    assert_eq!(
        result["signatures"][1].get("activeParameter"),
        Some(&Value::Null)
    );
    assert!(result["signatures"][0].get("activeParameter").is_none());
    assert_eq!(result["activeSignature"], 1);
}
