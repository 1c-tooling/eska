//! Owned fixtures prove authoring locality, accepted authority and real publication conflicts.

use std::{fs, path::PathBuf};

use super::*;
use crate::{
    project_core::source_format::{
        FormLocalizedText, accept_source, export_designer, files::Inventory, import_designer_forms,
        prepare_protected,
    },
    test_support::TestDir,
};

const OWNER: &str = "Catalogs/Sample/Forms/Main/form.yaml";
const DIALOG: &str = "Catalogs/Sample/Forms/Main/Ext/Form.xml";
const XML: &str = "\u{feff}<Form xmlns=\"http://v8.1c.ru/8.3/xcf/logform\" xmlns:v8=\"http://v8.1c.ru/8.1/data/core\" version=\"2.20\">\r\n<AutoCommandBar name=\"\" id=\"-1\"/><Title><v8:item><v8:lang>ru</v8:lang><v8:content>Форма</v8:content></v8:item><v8:item><v8:lang>en</v8:lang><v8:content>Form</v8:content></v8:item></Title><ChildItems><UsualGroup name=\"First\" id=\"1\"><ChildItems><InputField name=\"Value\" id=\"3\"><Title formatted=\"true\"><v8:item><v8:lang>ru</v8:lang><v8:content>Поле</v8:content></v8:item></Title></InputField></ChildItems></UsualGroup><UsualGroup name=\"Second\" id=\"2\"><ChildItems/></UsualGroup></ChildItems><ReadOnly>true</ReadOnly>\r\n</Form>\r\n";

/// UUIDs address metadata; decimal UI IDs never become accepted-base authority.
fn uuid(prefix: char) -> NativeUuid {
    NativeUuid::parse(&format!("{prefix}0000000-0000-4000-8000-000000000001")).unwrap()
}

/// Real copied native metadata and synthetic CF bytes remain inside this test's unique playground tree.
fn fixture(directory: &TestDir, frozen: &[NativeUuid], locked: bool) -> (PathBuf, AcceptedBase) {
    let native = directory.0.join("native");
    let original =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eska-format/designer");
    fs::create_dir(&native).unwrap();
    let inventory = Inventory::read(&original).unwrap();
    for path in inventory.directories {
        fs::create_dir_all(native.join(path.as_path())).unwrap();
    }
    for path in inventory.files.keys() {
        fs::copy(original.join(path.as_path()), native.join(path.as_path())).unwrap();
    }
    fs::write(native.join(DIALOG), XML).unwrap();
    let policy = native.join("Ext/ParentConfigurations.bin");
    if locked {
        let text = fs::read_to_string(&policy).unwrap().replace(
            "\"SampleConfiguration\",3,",
            &format!("\"SampleConfiguration\",4,0,0,{0},{0},", uuid('7').as_str()),
        );
        fs::write(policy, text).unwrap();
        fs::create_dir_all(native.join("Ext/ParentConfigurations")).unwrap();
        fs::write(
            native.join("Ext/ParentConfigurations/SampleConfiguration.cf"),
            b"opaque test CF",
        )
        .unwrap();
    } else {
        fs::remove_file(policy).unwrap();
    }
    let root = directory.0.join("source");
    import_designer_forms(&native, &root).unwrap();
    let accepted = directory.0.join("accepted");
    let reference = accept_source(&root, &accepted, frozen).unwrap();
    (root, AcceptedBase::open(&accepted, &reference).unwrap())
}

/// Root booleans use their existing native slot rather than a manufactured default.
fn flag(value: bool) -> FormEdit {
    FormEdit::Property {
        element: None,
        property: FormPropertyName::ReadOnly,
        value: FormPropertyValue::Bool(value),
    }
}

/// Read exact current owner bytes for preview and publication assertions.
fn text(root: &Path) -> String {
    fs::read_to_string(root.join(OWNER)).unwrap()
}

/// Logical no-op keeps spelling, comments, CRLF and mtime; preview itself has no write authority.
#[test]
fn equal_form_edit_preserves_exact_bytes_and_saved_mtime() {
    let directory = TestDir::new();
    let (root, base) = fixture(&directory, &[], false);
    let input = format!("# keep owner\n{}", text(&root))
        .replace("\"ReadOnly\": true", "\"ReadOnly\": TRUE # keep spelling")
        .replace('\n', "\r\n");
    fs::write(root.join(OWNER), &input).unwrap();
    let before = fs::metadata(root.join(OWNER)).unwrap().modified().unwrap();
    let preview = prepare_form_edit_text(&base, &uuid('7'), flag(true), &input).unwrap();
    assert!(preview.is_empty());
    assert_eq!(preview.updated(), input);
    assert_eq!(preview.path().as_str(), OWNER);
    assert_eq!(preview.baseline(), base.reference());
    let saved = prepare_form_edit(&root, &base, &uuid('7'), flag(true)).unwrap();
    assert!(saved.is_empty());
    saved.publish(&base).unwrap();
    assert_eq!(text(&root), input);
    assert_eq!(
        fs::metadata(root.join(OWNER)).unwrap().modified().unwrap(),
        before
    );
}

/// Localized values retain neighboring languages, scalar style, comments and untouched native stencils.
#[test]
fn form_property_previews_replace_only_selected_existing_scalars() {
    let directory = TestDir::new();
    let (root, base) = fixture(&directory, &[], false);
    let saved = text(&root);
    for eol in ["\n", "\r\n"] {
        let input = text(&root)
            .replace("\"ru\": \"Форма\"", "\"ru\": 'Форма' # title comment")
            .replace('\n', eol);
        let mut value = ObjectSource::from_yaml(&input)
            .unwrap()
            .form
            .unwrap()
            .properties[&FormPropertyName::Title]
            .clone();
        let FormPropertyValue::Localized(title) = &mut value else {
            panic!("localized fixture")
        };
        title.text.insert("ru".into(), "Новая 😀 форма".into());
        let edit = FormEdit::Property {
            element: None,
            property: FormPropertyName::Title,
            value,
        };
        let preview = prepare_form_edit_text(&base, &uuid('7'), edit, &input).unwrap();
        assert_eq!(
            preview.updated(),
            input.replace("'Форма'", "'Новая 😀 форма'")
        );
        assert_eq!(text(&root), saved);
        let changed =
            prepare_form_edit_text(&base, &uuid('7'), flag(false), preview.updated()).unwrap();
        assert_eq!(
            changed.updated(),
            preview
                .updated()
                .replace("\"ReadOnly\": true", "\"ReadOnly\": false")
        );
    }
    let input = text(&root);
    let mut value = ObjectSource::from_yaml(&input)
        .unwrap()
        .form
        .unwrap()
        .elements[&FormId::parse("3").unwrap()]
        .properties[&FormPropertyName::Title]
        .clone();
    let FormPropertyValue::Localized(title) = &mut value else {
        panic!("localized fixture")
    };
    title.formatted = Some(false);
    let edit = FormEdit::Property {
        element: Some(FormId::parse("3").unwrap()),
        property: FormPropertyName::Title,
        value,
    };
    let preview = prepare_form_edit_text(&base, &uuid('7'), edit, &input).unwrap();
    assert_eq!(
        preview.updated(),
        input.replace("\"formatted\": true", "\"formatted\": false")
    );
}

/// Reorder and move change existing collection values while definitions/properties stay at their original bytes.
#[test]
fn layout_edit_moves_into_empty_slot_and_only_changes_native_dialog() {
    let directory = TestDir::new();
    let (root, base) = fixture(&directory, &[], false);
    let input = text(&root);
    let before = ObjectSource::from_yaml(&input).unwrap();
    let mut layout = before.form.as_ref().unwrap().layout.clone();
    layout.roots.reverse();
    layout
        .children
        .get_mut(&FormId::parse("1").unwrap())
        .unwrap()
        .clear();
    layout
        .children
        .get_mut(&FormId::parse("2").unwrap())
        .unwrap()
        .push(FormId::parse("3").unwrap());
    let plan =
        prepare_form_edit(&root, &base, &uuid('7'), FormEdit::Layout(layout.clone())).unwrap();
    let after = ObjectSource::from_yaml(plan.updated()).unwrap();
    assert_eq!(after.document, before.document);
    assert_eq!(
        after.form.as_ref().unwrap().elements,
        before.form.as_ref().unwrap().elements
    );
    assert_eq!(
        after.form.as_ref().unwrap().properties,
        before.form.as_ref().unwrap().properties
    );
    assert_eq!(after.form.unwrap().layout, layout);
    let control = directory.0.join("control");
    export_designer(&root, &control).unwrap();
    plan.publish(&base).unwrap();
    let prepared = prepare_protected(&root, &base, &directory.0.join("generations"), true).unwrap();
    prepared.validate().unwrap();
    let old = Inventory::read(&control).unwrap();
    let new = Inventory::read(prepared.xml()).unwrap();
    let changes = new
        .files
        .iter()
        .filter(|(path, entry)| old.files.get(*path) != Some(*entry))
        .map(|(path, _)| path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(changes, [DIALOG]);
    assert!(
        prepare_form_edit(&root, &base, &uuid('7'), FormEdit::Layout(layout))
            .unwrap()
            .is_empty()
    );
}

/// Native restrictions and inherited ESKA freeze refuse previews before any publication authority is created.
#[test]
fn form_editor_uses_accepted_metadata_policy_and_ancestor_freeze() {
    for (frozen, locked, code) in [
        (vec![uuid('7')], false, "source_edit_frozen"),
        (vec![uuid('2')], false, "source_edit_frozen"),
        (vec![uuid('1')], false, "source_edit_frozen"),
        (vec![], true, "source_edit_native_restricted"),
    ] {
        let directory = TestDir::new();
        let (root, base) = fixture(&directory, &frozen, locked);
        let before = Inventory::read(&root).unwrap();
        assert_eq!(
            prepare_form_edit_text(&base, &uuid('7'), flag(false), &text(&root))
                .unwrap_err()
                .code,
            code
        );
        assert_eq!(
            prepare_form_edit(&root, &base, &uuid('7'), flag(false))
                .unwrap_err()
                .code,
            code
        );
        assert_eq!(Inventory::read(&root).unwrap(), before);
    }
}

/// A stale saved plan refuses replacement and an independent baseline pin cannot be substituted.
#[test]
fn saved_form_plan_rechecks_exact_current_bytes_and_baseline_pin() {
    let directory = TestDir::new();
    let (root, base) = fixture(&directory, &[], false);
    let plan = prepare_form_edit(&root, &base, &uuid('7'), flag(false)).unwrap();
    let concurrent = format!("{}\n# concurrent write\n", text(&root));
    fs::write(root.join(OWNER), &concurrent).unwrap();
    assert_eq!(
        plan.publish(&base).unwrap_err().code,
        "source_edit_conflict"
    );
    assert_eq!(text(&root), concurrent);
    let other_path = directory.0.join("other-base");
    let reference = accept_source(&root, &other_path, &[uuid('7')]).unwrap();
    let other = AcceptedBase::open(&other_path, &reference).unwrap();
    assert_eq!(
        plan.publish(&other).unwrap_err().code,
        "source_edit_baseline_changed"
    );
    assert_eq!(text(&root), concurrent);
}

/// A valid current owner cannot acquire new languages, slots, types or an ambiguous UI graph from a request.
#[test]
fn unsupported_form_requests_never_write_the_source() {
    let directory = TestDir::new();
    let (root, base) = fixture(&directory, &[], false);
    let input = text(&root);
    let before = Inventory::read(&root).unwrap();
    let mut layout = ObjectSource::from_yaml(&input)
        .unwrap()
        .form
        .unwrap()
        .layout;
    layout.roots.push(layout.roots[0].clone());
    let unknown = FormEdit::Property {
        element: Some(FormId::parse("999").unwrap()),
        property: FormPropertyName::Title,
        value: FormPropertyValue::Bool(false),
    };
    let wrong_type = FormEdit::Property {
        element: None,
        property: FormPropertyName::Title,
        value: FormPropertyValue::Bool(false),
    };
    let new_language = FormEdit::Property {
        element: None,
        property: FormPropertyName::Title,
        value: FormPropertyValue::Localized(FormLocalizedText {
            text: [
                ("ru".into(), "Форма".into()),
                ("en".into(), "Form".into()),
                ("de".into(), "Form".into()),
            ]
            .into(),
            formatted: None,
        }),
    };
    let new_slot = FormEdit::Property {
        element: None,
        property: FormPropertyName::Visible,
        value: FormPropertyValue::Bool(false),
    };
    for edit in [
        FormEdit::Layout(layout),
        unknown,
        wrong_type,
        new_language,
        new_slot,
    ] {
        assert!(prepare_form_edit_text(&base, &uuid('7'), edit.clone(), &input).is_err());
        assert!(prepare_form_edit(&root, &base, &uuid('7'), edit).is_err());
    }
    assert_eq!(
        prepare_form_edit_text(&base, &uuid('2'), flag(false), &input)
            .unwrap_err()
            .code,
        "source_edit_operation_unsupported"
    );
    assert_eq!(
        prepare_form_edit_text(&base, &uuid('8'), flag(false), &input)
            .unwrap_err()
            .code,
        "source_edit_object_unknown"
    );
    let spoofed = input.replace(uuid('7').as_str(), uuid('8').as_str());
    assert_eq!(
        prepare_form_edit_text(&base, &uuid('7'), flag(false), &spoofed)
            .unwrap_err()
            .code,
        "source_edit_structure_changed"
    );
    let substituted = input.replace("\"name\": \"Value\"", "\"name\": \"Different\"");
    assert!(prepare_form_edit_text(&base, &uuid('7'), flag(true), &substituted).is_err());
    assert_eq!(Inventory::read(&root).unwrap(), before);
}
