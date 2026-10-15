//! Sequences own their declared dimensions and document identities independently of registers.

use crate::{AttributeType, MdoType, ObjectBelonging};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A dimension keeps its original identity when a borrowed declaration omits its type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SequenceDimension {
    pub uuid: Uuid,
    pub name: String,
    pub attribute_type: Option<AttributeType>,
    pub belonging: ObjectBelonging,
    pub extends: Option<Uuid>,
}

impl SequenceDimension {
    /// Equal names alone cannot replace a base dimension or inherit its type.
    fn adopts(&self, base: &Self) -> bool {
        self.belonging == ObjectBelonging::Adopted
            && stdx::case::eq_ignore_case(&self.name, &base.name)
            && self.extends == Some(base.extends.unwrap_or(base.uuid))
    }

    /// Count source-owned strings and the optional native type description.
    fn estimated_heap_size(&self) -> usize {
        self.name.capacity()
            + self.attribute_type.as_ref().map_or(0, AttributeType::estimated_heap_size)
    }
}

/// A sequence has generated manager, record-set and record types, without an MDO/register surrogate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sequence {
    uuid: Uuid,
    name: String,
    dimensions: Vec<SequenceDimension>,
    documents: Option<Vec<String>>,
    register_records: Option<Vec<(MdoType, String)>>,
    belonging: ObjectBelonging,
    extends: Option<Uuid>,
}

impl Sequence {
    /// The producer supplies complete own declarations and preserves absent borrowed collections.
    pub fn from_parts(
        uuid: Uuid,
        name: String,
        dimensions: Vec<SequenceDimension>,
        documents: Option<Vec<String>>,
        register_records: Option<Vec<(MdoType, String)>>,
        belonging: ObjectBelonging,
        extends: Option<Uuid>,
    ) -> Self {
        Self { uuid, name, dimensions, documents, register_records, belonging, extends }
    }

    /// Source identity never depends on the filename or generated type identifier.
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    /// The configured spelling names all three generated sequence types.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Declaration order and physical dimension identities remain stable.
    pub fn dimensions(&self) -> &[SequenceDimension] {
        &self.dimensions
    }

    /// An absent borrowed list inherits; an own empty list remains explicitly empty.
    pub fn documents(&self) -> Option<&[String]> {
        self.documents.as_deref()
    }

    /// These register links describe boundary dependencies rather than sequence record fields.
    pub fn register_records(&self) -> Option<&[(MdoType, String)]> {
        self.register_records.as_deref()
    }

    /// A borrowed owner must prove the original declaration before contributing any fields.
    pub fn object_belonging(&self) -> ObjectBelonging {
        self.belonging
    }

    /// Every borrowed dimension proves its original UUID; new dimensions cannot collide by name or UUID.
    pub fn adopts(&self, base: &Self) -> bool {
        self.belonging == ObjectBelonging::Adopted
            && stdx::case::eq_ignore_case(&self.name, &base.name)
            && self.extends == Some(base.extends.unwrap_or(base.uuid))
            && self.dimensions.iter().all(|dimension| {
                if let Some(original) = base
                    .dimensions
                    .iter()
                    .find(|original| stdx::case::eq_ignore_case(&dimension.name, &original.name))
                {
                    dimension.adopts(original)
                } else {
                    dimension.belonging == ObjectBelonging::Own
                        && dimension.extends.is_none()
                        && !base.dimensions.iter().any(|original| original.uuid == dimension.uuid)
                }
            })
    }

    /// A proven overlay keeps untouched dimensions and extends the declared document/register links.
    pub fn apply_extension_overlay(&mut self, overlay: &Self) {
        for dimension in &overlay.dimensions {
            if let Some(original) = self
                .dimensions
                .iter_mut()
                .find(|original| stdx::case::eq_ignore_case(&dimension.name, &original.name))
            {
                if dimension.attribute_type.is_some() {
                    original.attribute_type = dimension.attribute_type.clone();
                }
                original.uuid = dimension.uuid;
                original.belonging = dimension.belonging;
                original.extends = dimension.extends;
            } else {
                self.dimensions.push(dimension.clone());
            }
        }
        if let Some(documents) = &overlay.documents {
            let current = self.documents.get_or_insert_with(Vec::new);
            for document in documents {
                if !current.iter().any(|name| stdx::case::eq_ignore_case(name, document)) {
                    current.push(document.clone());
                }
            }
        }
        if let Some(registers) = &overlay.register_records {
            let current = self.register_records.get_or_insert_with(Vec::new);
            for (kind, name) in registers {
                if !current.iter().any(|(other_kind, other_name)| {
                    kind == other_kind && stdx::case::eq_ignore_case(name, other_name)
                }) {
                    current.push((*kind, name.clone()));
                }
            }
        }
        self.uuid = overlay.uuid;
        self.belonging = overlay.belonging;
        self.extends = overlay.extends;
    }

    /// Current source storage includes dimension types and all declared relation names.
    pub fn estimated_heap_size(&self) -> usize {
        self.name.capacity()
            + self.dimensions.capacity() * std::mem::size_of::<SequenceDimension>()
            + self.dimensions.iter().map(SequenceDimension::estimated_heap_size).sum::<usize>()
            + self.documents.as_ref().map_or(0, |values| {
                values.capacity() * std::mem::size_of::<String>()
                    + values.iter().map(String::capacity).sum::<usize>()
            })
            + self.register_records.as_ref().map_or(0, |values| {
                values.capacity() * std::mem::size_of::<(MdoType, String)>()
                    + values.iter().map(|(_, name)| name.capacity()).sum::<usize>()
            })
    }
}
