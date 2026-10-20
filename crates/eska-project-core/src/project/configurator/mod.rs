//! Schema-driven, read-only projections of a single loaded metadata descriptor.

mod order;
mod schema;
mod tree;

pub use order::{METADATA_ORDER, metadata_group_rank};
pub use schema::ConfiguratorSchema;
pub use tree::{
    ChildrenState, ConfiguratorTree, ModuleAvailability, TreeError, TreeLabel, TreeNode,
    TreeOptions,
};
