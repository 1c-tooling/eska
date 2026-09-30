//! Global binding discovery is source-local and refuses to guess exports hidden in protected modules.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use roxmltree::{Document, Node};

use super::RenameError;
use crate::project::{
    ProjectType,
    metadata_edit::{EditError, snapshot},
    metadata_rename::{
        bsl::{GlobalBindings, contains_name, module_names},
        inventory::{Inventory, MAX_TEXT_BYTES, read_text},
    },
};

const MD: &str = "http://v8.1c.ru/8.3/MDClasses";

/// The module list is proven from declarations; every extra read must agree with the final inventory hash.
#[derive(Default)]
pub(super) struct Environment {
    pub globals: GlobalBindings,
    modules: BTreeSet<PathBuf>,
    snapshots: BTreeMap<PathBuf, String>,
}

impl Environment {
    /// Common modules and application modules have no implicit form/object attributes to shadow globals.
    pub fn read(
        root: &Path,
        descriptor: &Path,
        kind: ProjectType,
        inventory: &Inventory,
    ) -> Result<Self, RenameError> {
        let mut environment = Self::default();
        // An extension does not contain the base configuration's complete global context.
        if kind != ProjectType::Configuration {
            return Ok(environment);
        }
        let Some(input) = environment.text(root, descriptor, inventory)? else {
            return Ok(environment);
        };
        let Ok(document) = Document::parse(&input) else {
            return Ok(environment);
        };
        let Some(configuration) = document
            .root_element()
            .children()
            .find(|node| node.has_tag_name((MD, "Configuration")))
        else {
            return Ok(environment);
        };
        environment.globals.complete = true;
        for name in [
            "ManagedApplicationModule",
            "OrdinaryApplicationModule",
            "ExternalConnectionModule",
            "SessionModule",
        ] {
            let path = PathBuf::from(format!("Ext/{name}.bsl"));
            environment.modules.insert(path.clone());
            environment.global_code(root, &path, inventory)?;
        }
        for reference in configuration
            .children()
            .filter(|node| node.has_tag_name((MD, "ChildObjects")))
            .flat_map(|node| node.children())
            .filter(|node| node.has_tag_name((MD, "CommonModule")))
        {
            let Some(name) = reference.text() else {
                environment.globals.complete = false;
                continue;
            };
            if crate::project::metadata_rename::validate_name(name).is_err() {
                environment.globals.complete = false;
                continue;
            }
            let descriptor = PathBuf::from("CommonModules").join(format!("{name}.xml"));
            let Some(input) = environment.text(root, &descriptor, inventory)? else {
                environment.globals.complete = false;
                continue;
            };
            let Some(global) = common_module(&input, name) else {
                environment.globals.complete = false;
                continue;
            };
            let module = PathBuf::from("CommonModules")
                .join(name)
                .join("Ext/Module.bsl");
            environment.modules.insert(module.clone());
            if global {
                environment.global_code(root, &module, inventory)?;
            } else if contains_name(&environment.globals.common_modules, name) {
                environment.globals.complete = false;
            } else {
                environment.globals.common_modules.insert(name.to_owned());
            }
        }
        Ok(environment)
    }

    /// Implicit contexts which have not been checked remain explicitly uncertain in preview.
    pub fn known_module(&self, path: &Path) -> bool {
        self.modules.contains(path)
    }

    /// A global declaration changed between discovery and scanning cannot authorize a stale binding.
    pub fn check_snapshots(&self, actual: &BTreeMap<PathBuf, String>) -> Result<(), RenameError> {
        if self
            .snapshots
            .iter()
            .any(|(path, expected)| actual.get(path) != Some(expected))
        {
            return Err(EditError::Conflict.into());
        }
        Ok(())
    }

    /// Read only inventory members and retain their exact source digest for the final consistency check.
    fn text(
        &mut self,
        root: &Path,
        path: &Path,
        inventory: &Inventory,
    ) -> Result<Option<String>, RenameError> {
        if inventory
            .files
            .binary_search_by(|item| item.as_path().cmp(path))
            .is_err()
        {
            return Ok(None);
        }
        let bytes = std::fs::metadata(root.join(path))
            .map_err(|source| RenameError::Io {
                path: path.to_path_buf(),
                source,
            })?
            .len();
        if bytes > MAX_TEXT_BYTES {
            return Ok(None);
        }
        let input =
            read_text(&root.join(path), MAX_TEXT_BYTES).map_err(|source| RenameError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        self.snapshots.insert(path.to_path_buf(), snapshot(&input));
        Ok(Some(input))
    }

    /// Union all module-level names, including non-exported ones, rather than underestimating global visibility.
    fn global_code(
        &mut self,
        root: &Path,
        path: &Path,
        inventory: &Inventory,
    ) -> Result<(), RenameError> {
        if inventory.files.contains(&path.with_extension("bin")) {
            self.globals.complete = false;
        }
        if let Some(input) = self.text(root, path, inventory)? {
            if let Some(names) = module_names(&input) {
                self.globals.shadowed.extend(names);
            } else {
                self.globals.complete = false;
            }
        } else if inventory
            .files
            .binary_search_by(|item| item.as_path().cmp(path))
            .is_ok()
        {
            self.globals.complete = false;
        }
        Ok(())
    }
}

/// Validate descriptor identity before accepting its Global property or exported code as project context.
fn common_module(input: &str, expected: &str) -> Option<bool> {
    let document = Document::parse(input).ok()?;
    let node = document
        .root_element()
        .children()
        .find(|node| node.has_tag_name((MD, "CommonModule")))?;
    let properties = node
        .children()
        .find(|node| node.has_tag_name((MD, "Properties")))?;
    if property(properties, "Name")? != expected {
        return None;
    }
    match property(properties, "Global")? {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

/// Duplicate fields are ambiguous even if the first occurrence looks plausible.
fn property<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    let mut nodes = node.children().filter(|node| node.has_tag_name((MD, name)));
    let value = nodes.next()?.text()?;
    nodes.next().is_none().then_some(value)
}
