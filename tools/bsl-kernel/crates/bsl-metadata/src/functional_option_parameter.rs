//! Functional-option parameters describe admitted metadata targets independently of runtime values.

use crate::{MdoType, ObjectBelonging};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A parameter selects a catalog element or supplies one information-register dimension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FunctionalOptionParameterUse {
    Catalog { name: String },
    InformationRegisterDimension { register: String, dimension: String },
}

impl FunctionalOptionParameterUse {
    /// The current owner provides the parameter's possible value type.
    pub fn owner(&self) -> (MdoType, &str) {
        match self {
            Self::Catalog { name } => (MdoType::Catalog, name),
            Self::InformationRegisterDimension { register, .. } => {
                (MdoType::InformationRegister, register)
            }
        }
    }

    /// Native owner/member names are case-insensitive, while the target category remains exact.
    pub fn same_target(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Catalog { name }, Self::Catalog { name: other }) => {
                stdx::case::eq_ignore_case(name, other)
            }
            (
                Self::InformationRegisterDimension { register, dimension },
                Self::InformationRegisterDimension {
                    register: other_register,
                    dimension: other_dimension,
                },
            ) => {
                stdx::case::eq_ignore_case(register, other_register)
                    && stdx::case::eq_ignore_case(dimension, other_dimension)
            }
            _ => false,
        }
    }

    /// Count only the target names owned by this declaration.
    fn estimated_heap_size(&self) -> usize {
        match self {
            Self::Catalog { name } => name.capacity(),
            Self::InformationRegisterDimension { register, dimension } => {
                register.capacity() + dimension.capacity()
            }
        }
    }
}

/// Names and Use are current source data, not the interface's currently set parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionalOptionsParameter {
    uuid: Uuid,
    name: String,
    uses: Option<Vec<FunctionalOptionParameterUse>>,
    belonging: ObjectBelonging,
    extends: Option<Uuid>,
}

impl FunctionalOptionsParameter {
    /// Absent borrowed Use stays absent until the original identity has been proven.
    pub fn from_parts(
        uuid: Uuid,
        name: String,
        uses: Option<Vec<FunctionalOptionParameterUse>>,
        belonging: ObjectBelonging,
        extends: Option<Uuid>,
    ) -> Self {
        Self { uuid, name, uses, belonging, extends }
    }

    /// Source UUID is independent of physical filenames.
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    /// The configured name is the potential structure key in the platform getter.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Preserve the admitted target list without evaluating a parameter's runtime value.
    pub fn uses(&self) -> Option<&[FunctionalOptionParameterUse]> {
        self.uses.as_deref()
    }

    /// Borrowed parameters require an existing original.
    pub fn object_belonging(&self) -> ObjectBelonging {
        self.belonging
    }

    /// This profile admits inherited or identical Use with the exact original name and UUID.
    pub fn adopts(&self, base: &Self) -> bool {
        self.belonging == ObjectBelonging::Adopted
            && stdx::case::eq_ignore_case(&self.name, &base.name)
            && self.extends == Some(base.extends.unwrap_or(base.uuid))
            && self.uses.as_ref().is_none_or(|uses| {
                base.uses().is_some_and(|current| {
                    uses.len() == current.len()
                        && uses
                            .iter()
                            .all(|target| current.iter().any(|other| target.same_target(other)))
                })
            })
    }

    /// An admitted contribution changes identity bookkeeping without replacing the proven targets.
    pub fn apply_extension_overlay(&mut self, overlay: &Self) {
        self.uuid = overlay.uuid;
        self.belonging = overlay.belonging;
        self.extends = overlay.extends;
    }

    /// Canonical storage counts names and target allocations once.
    pub fn estimated_heap_size(&self) -> usize {
        self.name.capacity()
            + self.uses.as_ref().map_or(0, |uses| {
                uses.capacity() * std::mem::size_of::<FunctionalOptionParameterUse>()
                    + uses
                        .iter()
                        .map(FunctionalOptionParameterUse::estimated_heap_size)
                        .sum::<usize>()
            })
    }
}
