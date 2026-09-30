//! Resolve the renamed declaration and its physical companions before scanning references.

use std::{
    collections::BTreeMap,
    ops::Range,
    path::{Path, PathBuf},
};

use super::{ProjectSession, RenameError, WorkspaceError};
use crate::project::{
    metadata_edit::{EditError, snapshot},
    metadata_model::{MetadataKind, ObjectId},
    metadata_parser::{self, PropertiesMode},
    metadata_rename::{
        ReferenceRename, RenameFile, RenameMove, inventory::Inventory, same_name, validate_name,
    },
};

const MD: &str = "http://v8.1c.ru/8.3/MDClasses";
const XR: &str = "http://v8.1c.ru/8.3/xcf/readable";

pub(super) struct RenameContext {
    pub old_name: String,
    pub new_id: ObjectId,
    pub uuid: String,
    references: ReferenceRename,
    code: crate::project::metadata_rename::bsl::BslRename,
    declarations: BTreeMap<PathBuf, (String, Vec<Range<usize>>)>,
    move_candidates: Vec<RenameMove>,
}

impl ProjectSession {
    /// Derive logical ancestry and declaration coordinates from current source bytes.
    pub(super) fn rename_context(
        &mut self,
        id: &ObjectId,
        new_name: &str,
    ) -> Result<RenameContext, RenameError> {
        validate_name(new_name).map_err(RenameError::Name)?;
        let object = self.reveal_declared_object(id)?;
        if object.kind == MetadataKind::PredefinedItem {
            return Err(EditError::UnsupportedValue.into());
        }
        let new_id = ObjectId::from_parts(object.parent.as_ref(), object.kind.as_str(), new_name);
        if self.objects.values().any(|other| {
            other.id != *id
                && other.parent == object.parent
                && other.kind == object.kind
                && same_name(&other.name, new_name)
        }) {
            return Err(RenameError::Collision(new_id));
        }
        let location = self
            .source
            .object_descriptor(id)
            .map_err(WorkspaceError::Source)?
            .ok_or(EditError::UnsupportedValue)?;
        let input = self
            .source
            .read_xml(&location.path)
            .map_err(WorkspaceError::Source)?
            .ok_or(EditError::UnsupportedValue)?;
        let mut owner = id.clone();
        for _ in &location.inline {
            owner = owner.parent().ok_or(EditError::UnsupportedValue)?;
        }
        let parsed = metadata_parser::parse(&input, owner.parent(), PropertiesMode::Summary)
            .map_err(|_| EditError::InvalidXml)?;
        let target = parsed
            .objects
            .iter()
            .find(|object| object.metadata.id() == id)
            .ok_or(EditError::Conflict)?;
        let document = roxmltree::Document::parse(&input).map_err(|_| EditError::InvalidXml)?;
        let node = document
            .descendants()
            .find(|node| node.is_element() && node.range() == target.range)
            .ok_or(EditError::Conflict)?;
        let properties = node
            .children()
            .find(|node| node.has_tag_name((MD, "Properties")))
            .ok_or(EditError::UnsupportedValue)?;
        if properties.children().any(|node| {
            node.has_tag_name((MD, "ObjectBelonging")) && node.text() == Some("Adopted")
        }) {
            return Err(EditError::UnsupportedValue.into());
        }
        let name = properties
            .children()
            .find(|node| node.has_tag_name((MD, "Name")))
            .ok_or(EditError::UnsupportedValue)?;
        let generated: Vec<_> = node
            .descendants()
            .filter(|node| node.has_tag_name((XR, "GeneratedType")))
            .filter_map(|node| node.attribute("name").map(str::to_owned))
            .collect();
        let ancestry = self.rename_ancestry(id, node.tag_name().name())?;
        let mut context = RenameContext {
            old_name: object.name.clone(),
            new_id,
            uuid: target.metadata.uuid().to_owned(),
            references: ReferenceRename::new(&ancestry, new_name, &generated)
                .map_err(RenameError::Name)?,
            code: crate::project::metadata_rename::bsl::BslRename::new(&ancestry, new_name),
            declarations: BTreeMap::from([(
                location.path.clone(),
                (snapshot(&input), vec![name.range()]),
            )]),
            move_candidates: Vec::new(),
        };
        if location.inline.is_empty() && id != self.source.root().id() {
            context.descriptor_moves(&location.path, new_name);
            let parent = object
                .parent
                .as_ref()
                .unwrap_or_else(|| self.source.root().id());
            context.add_declaration(self, parent, id)?;
        }
        Ok(context)
    }
    /// External roots retain their Designer alias in every descendant reference.
    fn rename_ancestry(
        &self,
        id: &ObjectId,
        tag: &str,
    ) -> Result<Vec<(String, String)>, RenameError> {
        let parts =
            crate::project::designer_source::identity_parts(id).map_err(WorkspaceError::Source)?;
        let ancestry: Vec<_> = parts
            .iter()
            .enumerate()
            .map(|(index, part)| {
                let tag = if index == 0
                    && matches!(
                        self.project().configuration().project_type(),
                        crate::project::ProjectType::Processing
                            | crate::project::ProjectType::Report
                    ) {
                    match self.project().configuration().project_type() {
                        crate::project::ProjectType::Processing => "ExternalDataProcessor",
                        _ => "ExternalReport",
                    }
                } else if index + 1 == parts.len() {
                    tag
                } else {
                    part.kind.designer_tag()
                };
                (tag.to_owned(), part.name.clone())
            })
            .collect();
        Ok(ancestry)
    }
}

impl RenameContext {
    /// A standalone descriptor and its optional payload directory share the metadata filename.
    fn descriptor_moves(&mut self, path: &Path, new_name: &str) {
        self.move_candidates.push(RenameMove {
            from: path.to_path_buf(),
            to: path.with_file_name(format!("{new_name}.xml")),
            directory: false,
        });
        self.move_candidates.push(RenameMove {
            from: path.with_extension(""),
            to: path.with_file_name(new_name),
            directory: true,
        });
    }

    /// A separate descriptor is listed by its containing object, including the configuration root.
    fn add_declaration(
        &mut self,
        project: &ProjectSession,
        parent: &ObjectId,
        target: &ObjectId,
    ) -> Result<(), RenameError> {
        let location = project
            .source
            .object_descriptor(parent)
            .map_err(WorkspaceError::Source)?
            .ok_or(EditError::UnsupportedValue)?;
        let input = project
            .source
            .read_xml(&location.path)
            .map_err(WorkspaceError::Source)?
            .ok_or(EditError::UnsupportedValue)?;
        let parsed = metadata_parser::parse(&input, parent.parent(), PropertiesMode::Summary)
            .map_err(|_| EditError::InvalidXml)?;
        let reference = parsed
            .references
            .iter()
            .find(|reference| &reference.id == target)
            .ok_or(EditError::Conflict)?;
        self.declarations.insert(
            location.path,
            (snapshot(&input), vec![reference.range.clone()]),
        );
        Ok(())
    }

    /// Validate collisions for descriptors and payload directories before returning planned moves.
    pub fn moves(&self, inventory: &Inventory) -> Result<Vec<RenameMove>, RenameError> {
        let mut moves = Vec::new();
        for candidate in &self.move_candidates {
            if candidate.from == candidate.to {
                continue;
            }
            let exists = if candidate.directory {
                inventory.directories.contains(&candidate.from)
            } else {
                inventory.files.contains(&candidate.from)
            };
            if !exists {
                continue;
            }
            if inventory
                .files
                .iter()
                .chain(&inventory.directories)
                .any(|path| {
                    path != &candidate.from
                        && same_name(&path.to_string_lossy(), &candidate.to.to_string_lossy())
                })
            {
                return Err(RenameError::Collision(self.new_id.clone()));
            }
            moves.push(RenameMove {
                from: candidate.from.clone(),
                to: candidate.to.clone(),
                directory: candidate.directory,
            });
        }
        Ok(moves)
    }

    /// Analyze one immutable text snapshot, retaining BSL matches for semantic review.
    pub fn analyze(
        &self,
        path: &Path,
        kind: &str,
        input: &str,
        hash: &str,
        environment: &super::bsl::Environment,
    ) -> Result<RenameFile, RenameError> {
        let mut file = RenameFile {
            path: path.to_path_buf(),
            snapshot: hash.to_owned(),
            replacements: Vec::new(),
            uncertain: Vec::new(),
        };
        if !self.references.may_mention(input) && !self.declarations.contains_key(path) {
            return Ok(file);
        }
        if kind == "xml" {
            let declarations = self
                .declarations
                .get(path)
                .map_or(&[][..], |(_, ranges)| ranges.as_slice());
            let analysis = self.references.analyze_xml(input, declarations)?;
            file.replacements = analysis.replacements;
            file.uncertain = analysis.uncertain;
        } else {
            let analysis =
                self.code
                    .analyze(input, &environment.globals, environment.known_module(path));
            file.replacements = analysis.replacements;
            file.uncertain = analysis.uncertain;
        }
        Ok(file)
    }

    /// A source change between resolution and inventory cannot produce a coherent preview.
    pub fn check_snapshots(&self, actual: &BTreeMap<PathBuf, String>) -> Result<(), RenameError> {
        if self
            .declarations
            .iter()
            .any(|(path, (expected, _))| actual.get(path) != Some(expected))
        {
            return Err(EditError::Conflict.into());
        }
        Ok(())
    }
}
