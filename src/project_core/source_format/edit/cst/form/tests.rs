//! Splice regressions isolate collection transport and comment-loss failures from domain validation.

use super::*;

/// Decimal UI IDs are scalar strings, even when layout collections use flow style.
fn id(value: &str) -> FormId {
    FormId::parse(value).unwrap()
}

/// Existing slot membership is the sole mutable layout value in these concrete CST inputs.
fn layouts() -> (FormLayout, FormLayout) {
    let old = FormLayout {
        roots: vec![id("1"), id("2")],
        children: [(id("1"), vec![id("3")]), (id("2"), vec![])].into(),
    };
    let next = FormLayout {
        roots: vec![id("2"), id("1")],
        children: [(id("1"), vec![]), (id("2"), vec![id("3")])].into(),
    };
    (old, next)
}

/// Block/flow collections and CRLF must decode to the same requested graph, retaining untouched neighbors.
#[test]
fn collection_membership_preserves_empty_shape_styles_and_transport() {
    let (old, next) = layouts();
    for source in [
        "form:\n  layout:\n    roots: ['1', '2'] # roots\n    children: {'1': ['3'], '2': []}\n  marker: unchanged # neighbor\n",
        "form:\n  layout:\n    roots:\n      - '1' # position one\n      - '2' # position two\n    children:\n      '1':\n        - '3'\n      '2': []\n  marker: unchanged # neighbor\n",
    ] {
        for eol in ["\n", "\r\n"] {
            let source = source.replace('\n', eol);
            let updated = form_layout(&source, &old, &next).unwrap();
            let value = yaml::decode(&updated).unwrap();
            let layout: FormLayout =
                serde_json::from_value(value["form"]["layout"].clone()).unwrap();
            assert_eq!(layout, next);
            assert!(updated.ends_with(&format!("  marker: unchanged # neighbor{eol}")));
            assert_eq!(
                super::super::comments(&source),
                super::super::comments(&updated)
            );
            if eol == "\r\n" {
                assert!(!updated.replace("\r\n", "").contains('\n'));
            }
        }
    }
}

/// Removing annotated membership cannot silently delete the comment when an emptied collection is emitted.
#[test]
fn collection_edits_refuse_comment_loss_and_mixed_changed_transport() {
    let (old, next) = layouts();
    for source in [
        "form:\n  layout:\n    roots: ['1', '2']\n    children:\n      '1':\n        - '3' # annotated child\n      '2': []\n",
        "form:\n  layout:\n    roots:\r\n      - '1'\r\n      - '2'\n    children: {'1': ['3'], '2': []}\n",
    ] {
        assert_eq!(
            form_layout(source, &old, &next).unwrap_err().code,
            "source_edit_value_unsupported"
        );
    }
}

/// Current quoting and literal string transport are preserved wherever the selected new text permits them.
#[test]
fn localized_scalar_splices_preserve_flow_quotes_and_literal_neighbors() {
    let old = FormPropertyValue::Localized(crate::project_core::source_format::FormLocalizedText {
        text: [
            ("ru".into(), "old".into()),
            ("en".into(), "neighbor".into()),
        ]
        .into(),
        formatted: None,
    });
    let new = FormPropertyValue::Localized(crate::project_core::source_format::FormLocalizedText {
        text: [
            ("ru".into(), "new 😀".into()),
            ("en".into(), "neighbor".into()),
        ]
        .into(),
        formatted: None,
    });
    for (source, expected) in [
        (
            "form: {properties: {Title: {text: {ru: 'old', en: 'neighbor'}}}} # keep\n",
            "form: {properties: {Title: {text: {ru: 'new 😀', en: 'neighbor'}}}} # keep\n",
        ),
        (
            "form:\n  properties:\n    Title:\n      text:\n        ru: |-\n          old\n        en: 'neighbor' # keep\n",
            "form:\n  properties:\n    Title:\n      text:\n        ru: |-\n          new 😀\n        en: 'neighbor' # keep\n",
        ),
    ] {
        let updated = form_property(source, None, FormPropertyName::Title, &old, &new).unwrap();
        assert_eq!(updated, expected);
        assert_eq!(
            yaml::decode(&updated).unwrap()["form"]["properties"]["Title"],
            serde_json::to_value(&new).unwrap()
        );
    }
}
