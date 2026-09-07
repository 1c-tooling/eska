//! Shared human presentation of project paths and semantic changes.

use crate::{
    cli::localization::Localizer,
    project::{metadata::MetadataPath, semantic::SemanticEventKind},
};
use gix::bstr::{BStr, ByteSlice};

/// Render a logical metadata identity in Configurator notation.
pub(super) fn render_metadata_path(path: &MetadataPath, localizer: &Localizer) -> String {
    path.parts
        .iter()
        .map(|part| {
            let kind = metadata_kind(part.kind, localizer);
            part.name
                .as_ref()
                .map_or_else(|| kind.clone(), |name| format!("{kind}.{name}"))
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// Resolve the localized Configurator name of a stable metadata kind.
pub(super) fn metadata_kind(kind: &str, localizer: &Localizer) -> String {
    localizer.text(&format!("diff-metadata-{kind}"))
}

/// Return the top-level metadata kind encoded in a stable semantic object ID.
pub(super) fn semantic_object_group(id: &str) -> &str {
    id.split([':', '/']).next().unwrap_or(id)
}

/// Render every hierarchical `ObjectId` segment in localized Configurator notation.
pub(super) fn render_semantic_object(id: &str, localizer: &Localizer) -> String {
    id.split('/')
        .map(|segment| {
            segment.split_once(':').map_or_else(
                || metadata_kind(segment, localizer),
                |(kind, name)| {
                    format!(
                        "{}.{}",
                        metadata_kind(kind, localizer),
                        unescape_object_name(name)
                    )
                },
            )
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// Select a localized label for one semantic event kind.
pub(super) const fn semantic_event_key(kind: SemanticEventKind) -> &'static str {
    match kind {
        SemanticEventKind::ObjectAdded => "diff-semantic-object-added",
        SemanticEventKind::ObjectRemoved => "diff-semantic-object-removed",
        SemanticEventKind::ObjectChanged => "diff-semantic-object-changed",
        SemanticEventKind::ModuleChanged => "diff-semantic-module-changed",
        SemanticEventKind::MethodAdded => "diff-semantic-method-added",
        SemanticEventKind::MethodRemoved => "diff-semantic-method-removed",
        SemanticEventKind::MethodChanged => "diff-semantic-method-changed",
        SemanticEventKind::FunctionAdded => "diff-semantic-function-added",
        SemanticEventKind::FunctionRemoved => "diff-semantic-function-removed",
        SemanticEventKind::FunctionChanged => "diff-semantic-function-changed",
        SemanticEventKind::FormChanged => "diff-semantic-form-changed",
        SemanticEventKind::MetadataAttributeChanged => "diff-semantic-metadata-attribute-changed",
    }
}

/// Quote paths only when control characters, quotes or arbitrary bytes require escaping.
pub(super) fn display_path(path: &BStr) -> String {
    if let Ok(path) = path.to_str()
        && path
            .chars()
            .all(|character| !character.is_control() && character != '\\' && character != '"')
    {
        return path.to_owned();
    }

    let mut escaped = String::with_capacity(path.len() + 2);
    escaped.push('"');
    for byte in path.as_bytes() {
        match byte {
            b'\\' => escaped.push_str("\\\\"),
            b'"' => escaped.push_str("\\\""),
            b'\n' => escaped.push_str("\\n"),
            b'\r' => escaped.push_str("\\r"),
            b'\t' => escaped.push_str("\\t"),
            0x20..=0x7e => escaped.push(char::from(*byte)),
            _ => {
                use std::fmt::Write as _;
                write!(escaped, "\\x{byte:02X}").expect("writing to String cannot fail");
            }
        }
    }
    escaped.push('"');
    escaped
}

/// Decode only the three separators escaped by the stable `ObjectId` contract.
fn unescape_object_name(name: &str) -> String {
    name.replace("%2F", "/")
        .replace("%3A", ":")
        .replace("%25", "%")
}

#[cfg(test)]
mod tests {
    use super::{display_path, render_semantic_object};
    use crate::cli::localization::{Locale, Localizer};
    use gix::bstr::ByteSlice;

    /// Keep arbitrary Git path bytes readable and unambiguous in human output.
    #[test]
    fn path_presentation_escapes_control_characters() {
        assert_eq!(
            display_path(b"src/line\nname".as_bstr()),
            "\"src/line\\nname\""
        );
    }

    /// Render stable hierarchical IDs with localized Configurator type names.
    #[test]
    fn semantic_object_hierarchy_is_localized() {
        for (locale, expected) in [
            (
                Locale::RuRu,
                "Справочник.Контрагенты.Реквизит.Код/Артикул:1%",
            ),
            (Locale::EnUs, "Catalog.Контрагенты.Attribute.Код/Артикул:1%"),
        ] {
            let localizer = Localizer::try_new(locale).expect("valid locale");
            assert_eq!(
                render_semantic_object(
                    "catalog:Контрагенты/attribute:Код%2FАртикул%3A1%25",
                    &localizer
                ),
                expected
            );
        }
    }
}
