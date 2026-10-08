//! Append completed XML generations; consumers never observe a preparing directory as ready.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use super::super::{
    AcceptedBase, AcceptedRef, Error, SourcePath,
    files::{self, Inventory},
};
use super::{Origin, read_working};

/// Include every input authority and the resulting native bytes in the immutable generation key.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Binding {
    schema_version: u32,
    rules_version: u32,
    profile: String,
    input_revision: String,
    baseline: String,
    native_revision: String,
}

impl Binding {
    /// Cache identity cannot omit a policy revision or use stale materialized XML as input.
    pub(super) fn new(input: &str, base: &AcceptedRef, native: &str) -> Self {
        Self {
            schema_version: super::super::SCHEMA_VERSION,
            rules_version: super::super::RULES_VERSION,
            profile: super::super::PROFILE.into(),
            input_revision: input.into(),
            baseline: base.sha256.clone(),
            native_revision: native.into(),
        }
    }

    /// Serialize only fixed machine-facing fields when deriving the content-addressed name.
    pub(super) fn id(&self) -> Result<String, Error> {
        let bytes = serde_json::to_vec(self).map_err(|_| Error::root("source_schema_invalid"))?;
        Ok(files::hex(&Sha256::digest(bytes)))
    }
}

/// References remain bound to one captured working inventory and one already verified base.
pub(super) struct Plan<'a> {
    pub source: &'a Path,
    pub working: &'a Inventory,
    pub base: &'a AcceptedBase,
    pub binding: &'a Binding,
    pub xml: &'a BTreeMap<SourcePath, String>,
    pub native: &'a Inventory,
    pub origins: &'a BTreeMap<SourcePath, Origin>,
}

impl Plan<'_> {
    /// Stage, verify and rename one generation; failures leave all older complete outputs intact.
    pub(super) fn publish(&self, output: &Path) -> Result<PathBuf, Error> {
        self.validate_resources()?;
        let root = store(self.source, self.base, output)?;
        let id = self.binding.id()?;
        let target = root.join(&id);
        let marker = serde_json::to_vec_pretty(self.binding)
            .map_err(|_| Error::root("source_schema_invalid"))?;
        match fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.file_type().is_dir() => {
                if !fs::symlink_metadata(target.join("generation.json"))
                    .is_ok_and(|metadata| metadata.file_type().is_file())
                {
                    return Err(Error::root("source_generation_corrupt"));
                }
                let previous: Binding =
                    serde_json::from_str(&files::read_text(&target.join("generation.json"))?)
                        .map_err(|_| Error::root("source_generation_corrupt"))?;
                if previous != *self.binding
                    || Inventory::read(&target.join("xml"))? != *self.native
                {
                    return Err(Error::root("source_generation_corrupt"));
                }
                self.recheck()?;
                return Ok(target.join("xml"));
            }
            Ok(_) => return Err(Error::root("source_destination_exists")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(files::io(&error)),
        }
        let preparing = root.join(format!(".preparing-{id}"));
        fs::create_dir(&preparing).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                Error::root("source_generation_busy")
            } else {
                files::io(&error)
            }
        })?;
        let result = self.materialize(&preparing).and_then(|()| {
            self.recheck()?;
            files::write(&preparing.join("generation.json"), &marker)?;
            fs::rename(&preparing, &target).map_err(|error| files::io(&error))
        });
        if let Err(error) = result {
            fs::remove_dir_all(&preparing).map_err(|_| Error::root("source_cleanup_failed"))?;
            return Err(error);
        }
        Ok(target.join("xml"))
    }

    /// Every copied byte is checked against its selected origin, including module and CF payloads.
    fn materialize(&self, root: &Path) -> Result<(), Error> {
        let xml_root = root.join("xml");
        fs::create_dir(&xml_root).map_err(|error| files::io(&error))?;
        for path in &self.native.directories {
            fs::create_dir_all(xml_root.join(path.as_path())).map_err(|error| files::io(&error))?;
        }
        for entry in self.native.files.values() {
            let destination = xml_root.join(entry.path.as_path());
            if let Some(xml) = self.xml.get(&entry.path) {
                files::write(&destination, xml.as_bytes())?;
            } else {
                match self
                    .origins
                    .get(&entry.path)
                    .ok_or_else(|| Error::root("source_file_missing"))?
                {
                    Origin::Accepted => self.base.copy_file(&entry.path, &destination)?,
                    Origin::Working => files::copy(self.source, &xml_root, entry)?,
                }
            }
        }
        if Inventory::read(&xml_root)? != *self.native {
            return Err(Error::root("source_export_changed"));
        }
        Ok(())
    }

    /// XML resources keep their native representation, with bounded syntax checks for edits.
    fn validate_resources(&self) -> Result<(), Error> {
        for (path, origin) in self.origins {
            if !matches!(origin, Origin::Working)
                || self.xml.contains_key(path)
                || !path
                    .as_path()
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
                || self.working.files.get(path) == self.base.package.snapshot.files.get(path)
            {
                continue;
            }
            let text = read_working(self.source, self.working, path)?;
            crate::project::metadata_parser::check_envelope(&text)
                .map_err(|_| Error::root("source_resource_xml_invalid").in_field(path.as_str()))?;
            roxmltree::Document::parse_with_options(
                &text,
                roxmltree::ParsingOptions {
                    allow_dtd: false,
                    nodes_limit: 1_000_000,
                    ..roxmltree::ParsingOptions::default()
                },
            )
            .map_err(|_| Error::root("source_resource_xml_invalid").in_field(path.as_str()))?;
        }
        Ok(())
    }

    /// Recheck before publication so an input change cannot mix revisions in one ready directory.
    fn recheck(&self) -> Result<(), Error> {
        if Inventory::read(self.source)? != *self.working {
            return Err(Error::root("source_input_changed"));
        }
        self.base.verify()
    }
}

/// A dedicated store is outside both authorities and never claims an unrelated existing folder.
fn store(source: &Path, base: &AcceptedBase, output: &Path) -> Result<PathBuf, Error> {
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::root("source_path_invalid"))?;
    SourcePath::parse(name)?;
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let root = fs::canonicalize(parent)
        .map_err(|error| files::io(&error))?
        .join(name);
    if root.starts_with(source) || source.starts_with(&root) || base.overlaps(&root) {
        return Err(Error::root("source_tree_overlap"));
    }
    let marker = format!(
        "{{\"schema_version\":{},\"rules_version\":{},\"profile\":\"{}\"}}\n",
        super::super::SCHEMA_VERSION,
        super::super::RULES_VERSION,
        super::super::PROFILE
    );
    match fs::symlink_metadata(&root) {
        Ok(metadata) if metadata.file_type().is_dir() => {
            let path = root.join("store.json");
            if !fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_file())
                || files::read_text(&path)? != marker
            {
                return Err(Error::root("source_generation_store_invalid"));
            }
        }
        Ok(_) => return Err(Error::root("source_destination_exists")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&root).map_err(|error| files::io(&error))?;
            files::write(&root.join("store.json"), marker.as_bytes())?;
        }
        Err(error) => return Err(files::io(&error)),
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::source_format::{
        ObjectSource, Property, PropertyName, PropertyValue, Section, accept_source,
        export::Package, export_protected, import_designer,
    };

    mod support {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/mod.rs"));
    }

    /// A tiny own configuration keeps the race check independent of native support fixtures.
    fn prepare(root: &Path) -> (PathBuf, AcceptedBase) {
        let native = root.join("input");
        fs::create_dir(&native).unwrap();
        fs::write(native.join("Configuration.xml"), format!(
            "<MetaDataObject xmlns=\"{}\" version=\"2.20\"><Configuration uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>Race</Name><Comment>Accepted</Comment></Properties></Configuration></MetaDataObject>",
            crate::project::source_format::MD_NAMESPACE)).unwrap();
        let source = root.join("source");
        import_designer(&native, &source).unwrap();
        let accepted = root.join("base");
        let reference = accept_source(&source, &accepted, &[]).unwrap();
        (source, AcceptedBase::open(&accepted, &reference).unwrap())
    }

    /// Prepare a different valid generation before simulating a subsequent concurrent write.
    fn change_comment(root: &Path) {
        let path = root.join("configuration.yaml");
        let mut source = ObjectSource::from_yaml(&fs::read_to_string(&path).unwrap()).unwrap();
        for section in &mut source.document.descriptor.object.sections {
            if let Section::Properties(properties) = section {
                for property in properties {
                    if let Property::Typed {
                        name: PropertyName::Comment,
                        value,
                    } = property
                    {
                        *value = PropertyValue::Text("Prepared change".into());
                    }
                }
            }
        }
        fs::write(path, source.to_yaml().unwrap()).unwrap();
    }

    #[test]
    fn changed_inputs_between_preparation_and_publication_leave_old_generation_intact() {
        let test = support::TestDir::new();
        let (source, base) = prepare(&test.0);
        let output = test.0.join("generations");
        let previous = export_protected(&source, &base, &output, true).unwrap();
        let old_inventory = Inventory::read(&output).unwrap();
        change_comment(&source);
        let package = Package::read(&source).unwrap();
        let xml = package.native_xml().unwrap();
        let native = package.output_inventory(&xml).unwrap();
        let binding = Binding::new(
            &package.snapshot.revision,
            base.reference(),
            &native.revision,
        );
        let origins = BTreeMap::new();
        let plan = Plan {
            source: &source,
            working: &package.snapshot,
            base: &base,
            binding: &binding,
            xml: &xml,
            native: &native,
            origins: &origins,
        };
        fs::write(
            source.join("configuration.yaml"),
            "invalid concurrent input",
        )
        .unwrap();
        assert_eq!(
            plan.publish(&output).unwrap_err().code,
            "source_input_changed"
        );
        assert_eq!(Inventory::read(&output).unwrap(), old_inventory);
        assert!(previous.xml.join("Configuration.xml").is_file());
    }
}
