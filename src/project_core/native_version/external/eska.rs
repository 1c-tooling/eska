//! External versions use declared ESKA modules and independently pinned publication permissions.

use super::{
    LoadedDescriptor, MigrationBinding, ProjectVersion, VersionError, VersionMigration, contract,
    publication, read_xml,
};
use crate::project_core::{
    Project, ProjectType, SourceFormat,
    eska_source::EskaSource,
    metadata_model::ProjectScope,
    native_version::{BumpKind, BumpOutcome},
    object_module_version,
    publish::PublishError,
    source_format::{
        AcceptedBase, AcceptedRef, Error, SourceEditPlan, SourcePath, SourceReader,
        prepare_module_edit,
    },
};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

/// Reopening the pinned base prevents an old preview from granting permission after replacement.
#[derive(Debug)]
pub(super) struct Binding {
    project: Project,
    expected: AcceptedRef,
    descriptor: PathBuf,
    descriptor_text: String,
    manifest_text: String,
    edit: SourceEditPlan,
}

impl Binding {
    /// Verify root, declarations, accepted policy and exact module bytes at publication time.
    pub(super) fn publish(&self) -> Result<(), VersionError> {
        let reader = reader(&self.project)?;
        let base = accepted(&self.project, Some(&self.expected))?;
        base.validate_source_root(&reader)
            .map_err(|error| source(&self.project, error))?;
        if read_xml(&self.descriptor)? != self.descriptor_text
            || read_xml(&reader.root().join("source.json"))? != self.manifest_text
            || module_path(&reader)? != *self.edit.path()
        {
            return Err(publication(
                &reader.root().join(self.edit.path().as_path()),
                PublishError::Conflict,
            ));
        }
        self.edit
            .publish(&base)
            .map_err(|error| source(&self.project, error))
    }
}

/// Inspection requires declared source bytes, without opening or implicitly trusting a base.
pub(in crate::project_core::native_version) fn load(
    project: &Project,
) -> Result<LoadedDescriptor, VersionError> {
    let reader = reader(project)?;
    load_module(project, &reader)
}

/// Keep the version literal as the sole authority; YAML and TOML never duplicate its value.
pub(in crate::project_core::native_version) fn bump(
    project: &Project,
    kind: BumpKind,
    expected: Option<&AcceptedRef>,
) -> Result<BumpOutcome, VersionError> {
    let reader = reader(project)?;
    let descriptor = load_module(project, &reader)?;
    let current = descriptor.version.bumped(kind)?;
    let original = std::str::from_utf8(&descriptor.input)
        .map_err(|_| source(project, Error::root("source_encoding_unsupported")))?;
    let mut updated = original.to_owned();
    updated.replace_range(descriptor.value_range, &current.to_string());
    object_module_version::inspect_contract(&updated)
        .map_err(|error| contract(&descriptor.path, error))?;
    prepare(project, &reader, original, &updated, expected)?.publish()?;
    Ok(BumpOutcome {
        path: descriptor.path,
        previous: descriptor.version,
        current,
        kind,
    })
}

/// Preview is subject to the same accepted permission as its eventual publication.
pub(super) fn plan_migration(
    project: &Project,
    initial: Option<&ProjectVersion>,
    expected: Option<&AcceptedRef>,
) -> Result<VersionMigration, VersionError> {
    let reader = reader(project)?;
    let path = module_path(&reader)?;
    let original = reader
        .read_module_text(&path, object_module_version::MAX_MODULE_BYTES as u64)
        .map_err(|error| source(project, error))?;
    let plan = object_module_version::plan_migration(&original, initial)
        .map_err(|error| contract(&reader.root().join(path.as_path()), error))?;
    let updated = plan
        .apply(&original)
        .map_err(|error| contract(&reader.root().join(path.as_path()), error))?;
    if let Some(scheme) = project.configuration().version_scheme() {
        plan.version().validate_scheme(scheme)?;
    }
    let binding = prepare(project, &reader, &original, &updated, expected)?;
    Ok(VersionMigration {
        path: reader.root().join(path.as_path()),
        original: Some(original),
        plan,
        binding: MigrationBinding::Eska(Box::new(binding)),
    })
}

/// Source profile admission cannot be inferred from a leftover XML tree or filename.
fn reader(project: &Project) -> Result<SourceReader, VersionError> {
    if !matches!(
        project.configuration().project_type(),
        ProjectType::Processing | ProjectType::Report
    ) {
        return Err(VersionError::SourceFormatUnsupported);
    }
    EskaSource::open(project.clone(), ProjectScope::Standalone)
        .map(EskaSource::into_reader)
        .map_err(|error| source(project, error))
}

/// Resolve only the root's declared object module, retaining binary/ambiguous failures explicitly.
fn module_path(reader: &SourceReader) -> Result<SourcePath, VersionError> {
    let mut found = None;
    for path in reader.module_paths() {
        let Some(module) = reader.declared_module(path) else {
            continue;
        };
        if module.owner != &reader.configuration().path {
            continue;
        }
        let name = module.native_path.as_path().file_name();
        if name.is_some_and(|name| name == "ObjectModule.bin") {
            return Err(VersionError::ModuleProtected {
                path: reader.root().join(path.as_path()),
            });
        }
        if name.is_some_and(|name| name == "ObjectModule.bsl")
            && found.replace(path.clone()).is_some()
        {
            return Err(VersionError::ModuleAmbiguous {
                path: reader.root().to_owned(),
            });
        }
    }
    found.ok_or_else(|| VersionError::ModuleMissing {
        path: reader.root().join("ObjectModule.bsl"),
    })
}

/// Preserve the authoritative module's original byte coordinates and configured numeric policy.
fn load_module(project: &Project, reader: &SourceReader) -> Result<LoadedDescriptor, VersionError> {
    let relative = module_path(reader)?;
    let path = reader.root().join(relative.as_path());
    let input = reader
        .read_module_text(&relative, object_module_version::MAX_MODULE_BYTES as u64)
        .map_err(|error| source(project, error))?;
    let info =
        object_module_version::inspect_contract(&input).map_err(|error| contract(&path, error))?;
    if let Some(scheme) = project.configuration().version_scheme() {
        info.version.validate_scheme(scheme)?;
    }
    Ok(LoadedDescriptor {
        path,
        input: input.into_bytes(),
        version: info.version,
        value_range: info.value_range,
    })
}

/// The configured path locates accepted bytes; only the caller's digest grants trust.
fn accepted(
    project: &Project,
    expected: Option<&AcceptedRef>,
) -> Result<AcceptedBase, VersionError> {
    let expected = expected.ok_or(VersionError::AcceptedBaseRequired)?;
    let SourceFormat::Eska { accepted_base } = project.configuration().source_format() else {
        return Err(VersionError::AcceptedBaseUnexpected);
    };
    AcceptedBase::open(&project.root().join(accepted_base.as_path()), expected)
        .map_err(|error| source(project, error))
}

/// Capture exact root/manifest guards and a permission-checked module replacement together.
fn prepare(
    project: &Project,
    reader: &SourceReader,
    original: &str,
    updated: &str,
    expected: Option<&AcceptedRef>,
) -> Result<Binding, VersionError> {
    let base = accepted(project, expected)?;
    base.validate_source_root(reader)
        .map_err(|error| source(project, error))?;
    let relative = module_path(reader)?;
    let descriptor = reader.root().join(reader.configuration().path.as_path());
    let descriptor_text = read_xml(&descriptor)?;
    let manifest_text = read_xml(&reader.root().join("source.json"))?;
    let snapshot = reader.configuration();
    if digest(&descriptor_text) != snapshot.source_sha256
        || digest(&manifest_text) != snapshot.manifest_sha256
    {
        return Err(publication(&descriptor, PublishError::Conflict));
    }
    let edit = prepare_module_edit(reader.root(), &base, &relative, original, updated)
        .map_err(|error| source(project, error))?;
    Ok(Binding {
        project: project.clone(),
        expected: expected.ok_or(VersionError::AcceptedBaseRequired)?.clone(),
        descriptor,
        descriptor_text,
        manifest_text,
        edit,
    })
}

/// Stable hashes connect captured guard bytes to the reader's validated snapshot.
fn digest(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    Sha256::digest(text.as_bytes())
        .iter()
        .flat_map(|byte| {
            [
                char::from(HEX[usize::from(byte >> 4)]),
                char::from(HEX[usize::from(byte & 15)]),
            ]
        })
        .collect()
}

/// Preserve source-format coordinates without depending on CLI or localized strings.
fn source(project: &Project, error: Error) -> VersionError {
    VersionError::Source {
        path: project.source().to_owned(),
        error,
    }
}
