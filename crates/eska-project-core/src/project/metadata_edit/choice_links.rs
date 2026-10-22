//! Existing choice links use parameter names rather than metadata object identities.

use roxmltree::{Document, Node};
use unicode_general_category::{GeneralCategory, get_general_category};

use super::{
    EditError,
    schema::{MD, READABLE},
};
use crate::project::metadata_rename::same_name;

/// Recognize the reviewed list wrapper in ordinary and standard attribute properties.
pub(super) fn is_link(node: Node<'_, '_>) -> bool {
    node.has_tag_name((READABLE, "Link"))
        && node.parent().is_some_and(|parent| {
            parent.tag_name().name() == "ChoiceParameterLinks"
                && matches!(parent.tag_name().namespace(), Some(MD | READABLE))
        })
        && node.attributes().len() == 0
        && node.children().filter(Node::is_element).count() == 3
        && ["Name", "DataPath", "ValueChange"].iter().all(|name| {
            node.children()
                .filter(|child| child.has_tag_name((READABLE, *name)))
                .count()
                == 1
        })
}

/// This narrow exception never makes an object's structural `Properties/Name` writable.
pub(super) fn is_name(node: Node<'_, '_>) -> bool {
    node.has_tag_name((READABLE, "Name")) && node.parent().is_some_and(is_link)
}

/// The installed EDT accepts one or two nonempty segments of BMP letters, digits and underscores.
pub(super) fn valid_name(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() <= 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.chars().all(|character| {
                    character == '_'
                        || (character.len_utf16() == 1
                            && matches!(
                                get_general_category(character),
                                GeneralCategory::UppercaseLetter
                                    | GeneralCategory::LowercaseLetter
                                    | GeneralCategory::TitlecaseLetter
                                    | GeneralCategory::ModifierLetter
                                    | GeneralCategory::OtherLetter
                                    | GeneralCategory::DecimalNumber
                            ))
                })
        })
}

/// A scalar patch must not introduce duplicate or ancestor/descendant names in the existing list.
pub(super) fn validate_dependents(
    node: Node<'_, '_>,
    candidate: &Document<'_>,
) -> Result<(), EditError> {
    if !is_name(node) {
        return Ok(());
    }
    let name = candidate
        .descendants()
        .find(|item| is_name(*item) && item.range().start == node.range().start)
        .ok_or(EditError::InvalidXml)?;
    let link = name.parent().ok_or(EditError::InvalidXml)?;
    let list = link.parent().ok_or(EditError::InvalidXml)?;
    let value = name.text().unwrap_or_default();
    for other in list
        .children()
        .filter(|other| is_link(*other) && *other != link)
    {
        let other = other
            .children()
            .find(|item| is_name(*item))
            .and_then(|name| name.text())
            .unwrap_or_default();
        let (prefix, suffix) = value
            .split_once('.')
            .map_or((value, None), |(a, b)| (a, Some(b)));
        let (other_prefix, other_suffix) = other
            .split_once('.')
            .map_or((other, None), |(a, b)| (a, Some(b)));
        if same_name(value, other)
            || (same_name(prefix, other_prefix) && suffix.is_some() != other_suffix.is_some())
        {
            return Err(EditError::InvalidValue);
        }
    }
    Ok(())
}
