//! Current filter criteria retain the configured search type and the exact object-field membership.

use crate::{AttributeType, MdoType, ObjectBelonging};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The full field path is retained even though Find returns references to its enclosing objects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterCriterionTarget {
    pub kind: MdoType,
    pub name: String,
    pub path: String,
}

/// A borrowed criterion can add content while inheriting the proven original search type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilterCriterion {
    uuid: Uuid,
    name: String,
    search_type: Option<AttributeType>,
    content: Option<Vec<FilterCriterionTarget>>,
    belonging: ObjectBelonging,
    extends: Option<Uuid>,
}

impl FilterCriterion {
    /// Keep absence distinct from an explicit empty declaration until native composition.
    pub fn from_parts(
        uuid: Uuid,
        name: String,
        search_type: Option<AttributeType>,
        content: Option<Vec<FilterCriterionTarget>>,
        belonging: ObjectBelonging,
        extends: Option<Uuid>,
    ) -> Self {
        Self { uuid, name, search_type, content, belonging, extends }
    }

    /// Return the descriptor's own UUID without deriving it from its physical path.
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    /// Keep original spelling for BSL completion and generated native manager identity.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The configured search type controls Find's input rather than the type of its returned references.
    pub fn search_type(&self) -> Option<&AttributeType> {
        self.search_type.as_ref()
    }

    /// Return the complete current membership, preserving distinct fields of the same object.
    pub fn content(&self) -> Option<&[FilterCriterionTarget]> {
        self.content.as_deref()
    }

    /// A missing original cannot be replaced by an own or same-name declaration.
    pub fn object_belonging(&self) -> ObjectBelonging {
        self.belonging
    }

    /// This slice accepts only UUID-proven content additions with an inherited or identical search type.
    pub fn adopts(&self, base: &Self) -> bool {
        self.belonging == ObjectBelonging::Adopted
            && stdx::case::eq_ignore_case(&self.name, &base.name)
            && self.extends == Some(base.extends.unwrap_or(base.uuid))
            && self.search_type.as_ref().is_none_or(|value| Some(value) == base.search_type())
    }

    /// A verified borrowed contribution adds supplied members without removing the original content.
    pub fn apply_extension_overlay(&mut self, overlay: &Self) {
        self.uuid = overlay.uuid;
        self.belonging = overlay.belonging;
        self.extends = overlay.extends;
        if let Some(content) = &overlay.content {
            let previous = self.content.get_or_insert_with(Vec::new);
            for target in content {
                if !previous
                    .iter()
                    .any(|value| stdx::case::eq_ignore_case(&value.path, &target.path))
                {
                    previous.push(target.clone());
                }
            }
        }
    }

    /// Count owned source strings and type payloads once at the canonical input.
    pub fn estimated_heap_size(&self) -> usize {
        self.name.capacity()
            + self.search_type.as_ref().map_or(0, AttributeType::estimated_heap_size)
            + self.content.as_ref().map_or(0, |content| {
                content.capacity() * std::mem::size_of::<FilterCriterionTarget>()
                    + content
                        .iter()
                        .map(|value| value.name.capacity() + value.path.capacity())
                        .sum::<usize>()
            })
    }
}
