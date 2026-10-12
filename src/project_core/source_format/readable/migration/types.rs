//! Concrete type field bindings avoid moving whole sequences through CST reindentation factories.

use super::{Binding, Step, add, append};
use crate::project_core::source_format::{TypeData, TypeLayout, TypePart};

/// Ordinary type leaves retain authored styles while wrappers keep their own annotation targets.
pub(super) fn data(bindings: &mut Vec<Binding>, value: &TypeData, before: &[Step], after: &[Step]) {
    add(bindings, before.to_vec(), after.to_vec(), false);
    let old = append(before, [Step::Key("alternatives")]);
    let new = append(after, [Step::Key("alternatives")]);
    add(bindings, old.clone(), new.clone(), false);
    for index in 0..value.alternatives.len() {
        let old = append(&old, [Step::Index(index)]);
        let new = append(&new, [Step::Index(index)]);
        add(bindings, old.clone(), new.clone(), false);
        fields(bindings, &old, &new, &["kind", "name"], true);
    }
    if let Some(value) = &value.string {
        group(
            bindings,
            before,
            after,
            "string",
            &[
                ("length", value.length.is_some()),
                ("allowed_length", value.allowed_length.is_some()),
            ],
        );
    }
    if let Some(value) = &value.number {
        group(
            bindings,
            before,
            after,
            "number",
            &[
                ("digits", value.digits.is_some()),
                ("fraction_digits", value.fraction_digits.is_some()),
                ("allowed_sign", value.allowed_sign.is_some()),
            ],
        );
    }
    if let Some(value) = &value.date {
        group(
            bindings,
            before,
            after,
            "date",
            &[("fractions", value.fractions.is_some())],
        );
    }
}

/// Native slot comments follow their exact fields; canonical slots have no duplicate editable values.
pub(super) fn layout(
    bindings: &mut Vec<Binding>,
    value: &TypeLayout,
    before: &[Step],
    after: &[Step],
) {
    add(bindings, before.to_vec(), after.to_vec(), false);
    fields(bindings, before, after, &["prefix"], true);
    for (key, length) in [
        ("string_fields", value.string_fields.len()),
        ("number_fields", value.number_fields.len()),
        ("date_fields", value.date_fields.len()),
    ] {
        let old = append(before, [Step::Key(key)]);
        let new = append(after, [Step::Key(key)]);
        add(bindings, old.clone(), new.clone(), false);
        for index in 0..length {
            add(
                bindings,
                append(&old, [Step::Index(index)]),
                append(&new, [Step::Index(index)]),
                false,
            );
        }
    }
    let old = append(before, [Step::Key("parts")]);
    let new = append(after, [Step::Key("parts")]);
    add(bindings, old.clone(), new.clone(), false);
    for (index, part) in value.parts.iter().enumerate() {
        let old = append(&old, [Step::Index(index)]);
        let new = append(&new, [Step::Index(index)]);
        add(bindings, old.clone(), new.clone(), false);
        fields(bindings, &old, &new, &["kind"], true);
        if matches!(part, TypePart::Alternative(_)) {
            fields(bindings, &old, &new, &["index"], true);
        }
    }
}

/// Missing qualifier fields stay absent and cannot manufacture an annotation target or default.
fn group(
    bindings: &mut Vec<Binding>,
    before: &[Step],
    after: &[Step],
    key: &'static str,
    present: &[(&'static str, bool)],
) {
    let old = append(before, [Step::Key(key)]);
    let new = append(after, [Step::Key(key)]);
    add(bindings, old.clone(), new.clone(), false);
    for &(key, present) in present {
        if present {
            fields(bindings, &old, &new, &[key], true);
        }
    }
}

/// Mapping leaf copies preserve quoting without the sequence factory's last-entry comment loss.
fn fields(
    bindings: &mut Vec<Binding>,
    before: &[Step],
    after: &[Step],
    keys: &[&'static str],
    copy: bool,
) {
    for key in keys {
        add(
            bindings,
            append(before, [Step::Key(key)]),
            append(after, [Step::Key(key)]),
            copy,
        );
    }
}
