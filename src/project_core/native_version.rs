//! Authoritative native versions: XML properties for CF/CFE and object modules for EPF/ERF.

use std::{
    fmt, fs,
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    ops::Range,
    path::{Path, PathBuf},
};

use super::{Project, ProjectType, SourceFormat};
use serde::{Deserialize, Serialize};

mod external;
mod source;
pub use external::{VersionMigration, plan_migration, plan_migration_with_base};
pub use source::project_source_version;

const MAX_DESCRIPTOR_BYTES: u64 = 64 * 1024 * 1024;
const NAMESPACE: &str = "http://v8.1c.ru/8.3/MDClasses";

/// The numeric version contract, shared by manifests, editors and release planning.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum VersionScheme {
    #[serde(rename = "x.y.z")]
    ThreePart,
    #[serde(rename = "x.y.z.build")]
    FourPart,
}

impl VersionScheme {
    /// Return the portable manifest and protocol spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ThreePart => "x.y.z",
            Self::FourPart => "x.y.z.build",
        }
    }
}

/// Keep a three-part value free of an implicit fourth component.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Components {
    Three([u64; 3]),
    Four {
        values: [u64; 4],
        widths: [usize; 4],
    },
}

/// A validated numeric project version with its original scheme.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectVersion {
    components: Components,
}

impl ProjectVersion {
    /// Parse the numeric `x.y.z` or Designer `x.y.z.build` profile.
    ///
    /// # Errors
    ///
    /// Returns [`VersionError::InvalidVersion`] unless the input contains exactly
    /// three or four decimal components representable as `u64` values. Leading
    /// zeroes are allowed only in the four-part profile; suffixes are rejected.
    pub fn parse(value: &str) -> Result<Self, VersionError> {
        let parts: Vec<_> = value.split('.').collect();
        if !matches!(parts.len(), 3 | 4)
            || parts.iter().any(|part| {
                part.is_empty()
                    || !part.bytes().all(|byte| byte.is_ascii_digit())
                    || (parts.len() == 3 && part.len() > 1 && part.starts_with('0'))
            })
        {
            return Err(VersionError::InvalidVersion {
                value: value.to_owned(),
            });
        }
        let mut components = [0; 4];
        let mut widths = [0; 4];
        for (index, part) in parts.iter().enumerate() {
            components[index] = part.parse().map_err(|_| VersionError::InvalidVersion {
                value: value.to_owned(),
            })?;
            widths[index] = part.len();
        }
        Ok(Self {
            components: if parts.len() == 3 {
                Components::Three([components[0], components[1], components[2]])
            } else {
                Components::Four {
                    values: components,
                    widths,
                }
            },
        })
    }

    /// Expose the scheme without reparsing or inventing a build component.
    #[must_use]
    pub const fn scheme(&self) -> VersionScheme {
        match self.components {
            Components::Three(_) => VersionScheme::ThreePart,
            Components::Four { .. } => VersionScheme::FourPart,
        }
    }

    /// Validate an explicit policy without converting the stored version.
    ///
    /// # Errors
    /// Returns a mismatch when the configured scheme differs from the value.
    pub fn validate_scheme(&self, expected: VersionScheme) -> Result<(), VersionError> {
        let actual = self.scheme();
        if actual == expected {
            Ok(())
        } else {
            Err(VersionError::SchemeMismatch { expected, actual })
        }
    }

    /// Return the next version for the requested compatibility bump.
    ///
    /// Major, minor and patch map to the first three 1C components. Components
    /// below the bumped one restart at zero for three parts and at their legacy
    /// initial values for four parts. The scheme never changes during a bump.
    ///
    /// # Errors
    ///
    /// Returns [`VersionError::Overflow`] if the selected component is maximal.
    pub fn bumped(&self, kind: BumpKind) -> Result<Self, VersionError> {
        let mut next = self.clone();
        let index = match kind {
            BumpKind::Major => 0,
            BumpKind::Minor => 1,
            BumpKind::Patch => 2,
        };
        match &mut next.components {
            Components::Three(values) => {
                values[index] = values[index]
                    .checked_add(1)
                    .ok_or(VersionError::Overflow { kind })?;
                values[index + 1..].fill(0);
            }
            Components::Four { values, .. } => {
                values[index] = values[index]
                    .checked_add(1)
                    .ok_or(VersionError::Overflow { kind })?;
                match kind {
                    BumpKind::Major => values[1..].copy_from_slice(&[0, 1, 1]),
                    BumpKind::Minor => values[2..].copy_from_slice(&[1, 1]),
                    BumpKind::Patch => values[3] = 1,
                }
            }
        }
        Ok(next)
    }
}

impl fmt::Display for ProjectVersion {
    /// Preserve the width convention of every original numeric component.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.components {
            Components::Three(values) => {
                write!(formatter, "{}.{}.{}", values[0], values[1], values[2])
            }
            Components::Four { values, widths } => write!(
                formatter,
                "{:0w0$}.{:0w1$}.{:0w2$}.{:0w3$}",
                values[0],
                values[1],
                values[2],
                values[3],
                w0 = widths[0],
                w1 = widths[1],
                w2 = widths[2],
                w3 = widths[3],
            ),
        }
    }
}

/// Explicit version component selected by the user.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BumpKind {
    Major,
    Minor,
    Patch,
}

impl BumpKind {
    /// Return the stable machine-facing bump name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Major => "major",
            Self::Minor => "minor",
            Self::Patch => "patch",
        }
    }
}

/// Version value and the native source file that owns it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionInfo {
    pub path: PathBuf,
    pub version: ProjectVersion,
}

/// Completed narrow replacement of a project version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BumpOutcome {
    pub path: PathBuf,
    pub previous: ProjectVersion,
    pub current: ProjectVersion,
    pub kind: BumpKind,
}

/// Read the version from its authoritative native source without changing it.
///
/// # Errors
///
/// Returns a structured descriptor, XML, version or filesystem failure.
pub fn inspect(project: &Project) -> Result<VersionInfo, VersionError> {
    let descriptor = load_project_descriptor(project)?;
    Ok(VersionInfo {
        path: descriptor.path,
        version: descriptor.version,
    })
}

/// Replace only the authoritative version bytes in the XML property or BSL literal.
///
/// No XML serializer is used: BOM, line endings, whitespace, attribute ordering
/// and every byte outside the version text range remain unchanged.
///
/// # Errors
///
/// Returns a structured failure without publishing a partially written descriptor.
pub fn bump(project: &Project, kind: BumpKind) -> Result<BumpOutcome, VersionError> {
    bump_with_base(project, kind, None)
}

/// Bump an ESKA external module only under an independently supplied accepted-base digest.
///
/// # Errors
/// Returns missing/unexpected trust, denied permissions, invalid version or publication failures.
pub fn bump_with_base(
    project: &Project,
    kind: BumpKind,
    expected: Option<&super::source_format::AcceptedRef>,
) -> Result<BumpOutcome, VersionError> {
    if matches!(
        project.configuration().source_format(),
        SourceFormat::Eska { .. }
    ) {
        return external::bump_eska(project, kind, expected);
    }
    if expected.is_some() {
        return Err(VersionError::AcceptedBaseUnexpected);
    }
    let descriptor = load_project_descriptor(project)?;
    let current = descriptor.version.bumped(kind)?;
    let replacement = current.to_string();
    let mut output = Vec::with_capacity(
        descriptor.input.len() - descriptor.value_range.len() + replacement.len(),
    );
    output.extend_from_slice(&descriptor.input[..descriptor.value_range.start]);
    output.extend_from_slice(replacement.as_bytes());
    output.extend_from_slice(&descriptor.input[descriptor.value_range.end..]);
    if matches!(
        project.configuration().project_type(),
        ProjectType::Processing | ProjectType::Report
    ) {
        external::publish_existing(
            project.source(),
            &descriptor.path,
            &descriptor.input,
            &output,
        )?;
    } else {
        replace(&descriptor.path, &output)?;
    }
    Ok(BumpOutcome {
        path: descriptor.path,
        previous: descriptor.version,
        current,
        kind,
    })
}

/// Apply the resolved policy to both read and write paths before any publication.
fn load_project_descriptor(project: &Project) -> Result<LoadedDescriptor, VersionError> {
    let descriptor = match project.configuration().source_format() {
        SourceFormat::DesignerXml => {
            load_descriptor(project.source(), project.configuration().project_type())?
        }
        SourceFormat::Eska { .. } => external::load_eska(project)?,
    };
    if let Some(scheme) = project.configuration().version_scheme() {
        descriptor.version.validate_scheme(scheme)?;
    }
    Ok(descriptor)
}

/// Validate policy on an already materialized native input before launching the platform.
#[cfg(feature = "build")]
pub(crate) fn validate_native_policy(
    source: &Path,
    project_type: ProjectType,
    scheme: Option<VersionScheme>,
) -> Result<(), VersionError> {
    let version = load_descriptor(source, project_type)?.version;
    scheme.map_or(Ok(()), |scheme| version.validate_scheme(scheme))
}

#[derive(Debug)]
pub enum VersionError {
    SourceFormatUnsupported,
    AcceptedBaseRequired,
    AcceptedBaseUnexpected,
    Source {
        path: PathBuf,
        error: super::source_format::Error,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
    DescriptorTooLarge {
        path: PathBuf,
    },
    InvalidXml {
        path: PathBuf,
        source: roxmltree::Error,
    },
    DescriptorMissing {
        path: PathBuf,
    },
    DescriptorAmbiguous {
        path: PathBuf,
    },
    VersionMissing {
        path: PathBuf,
    },
    VersionAmbiguous {
        path: PathBuf,
    },
    InvalidVersion {
        value: String,
    },
    Overflow {
        kind: BumpKind,
    },
    SchemeMismatch {
        expected: VersionScheme,
        actual: VersionScheme,
    },
    ModuleContract {
        path: PathBuf,
        error: super::object_module_version::ContractError,
    },
    ModuleMissing {
        path: PathBuf,
    },
    ModuleProtected {
        path: PathBuf,
    },
    ModuleAmbiguous {
        path: PathBuf,
    },
    Publication {
        path: PathBuf,
        error: super::publish::PublishError,
    },
    MigrationUnsupported,
}

impl VersionError {
    /// Stable machine diagnostics never include an OS or parser's localized wording.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        use super::publish::PublishError;
        match self {
            Self::SourceFormatUnsupported => "version_source_format_unsupported",
            Self::AcceptedBaseRequired => "accepted_base_pin_required",
            Self::AcceptedBaseUnexpected => "version_accepted_base_unexpected",
            Self::Source { error, .. } => error.code,
            Self::Io { .. }
            | Self::Publication {
                error: PublishError::Io(_),
                ..
            } => "version_source_io",
            Self::DescriptorTooLarge { .. } => "version_descriptor_too_large",
            Self::InvalidXml { .. } => "version_descriptor_invalid_xml",
            Self::DescriptorMissing { .. } => "version_descriptor_missing",
            Self::DescriptorAmbiguous { .. } => "version_descriptor_ambiguous",
            Self::VersionMissing { .. } => "version_value_missing",
            Self::VersionAmbiguous { .. } => "version_value_ambiguous",
            Self::InvalidVersion { .. } => "version_value_invalid",
            Self::Overflow { .. } => "version_overflow",
            Self::SchemeMismatch { .. } => "version_scheme_mismatch",
            Self::ModuleContract { error, .. } => error.kind.code(),
            Self::ModuleMissing { .. } => "version_module_missing",
            Self::ModuleProtected { .. } => "version_module_protected",
            Self::ModuleAmbiguous { .. } => "version_module_ambiguous",
            Self::Publication {
                error: PublishError::Conflict,
                ..
            } => "version_source_changed",
            Self::Publication {
                error: PublishError::UnsafePath,
                ..
            } => "version_source_unsafe",
            Self::Publication {
                error: PublishError::ReadOnly,
                ..
            } => "version_source_read_only",
            Self::Publication {
                error: PublishError::Cancelled,
                ..
            } => "source_read_cancelled",
            Self::MigrationUnsupported => "version_migration_unsupported",
        }
    }

    /// Return the native source coordinate when the failure belongs to a particular file.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Source { path, .. }
            | Self::Io { path, .. }
            | Self::DescriptorTooLarge { path }
            | Self::InvalidXml { path, .. }
            | Self::DescriptorMissing { path }
            | Self::DescriptorAmbiguous { path }
            | Self::VersionMissing { path }
            | Self::VersionAmbiguous { path }
            | Self::ModuleContract { path, .. }
            | Self::ModuleMissing { path }
            | Self::ModuleProtected { path }
            | Self::ModuleAmbiguous { path }
            | Self::Publication { path, .. } => Some(path),
            _ => None,
        }
    }

    /// Syntax diagnostics use original UTF-8 bytes, including any BOM.
    #[must_use]
    pub fn range(&self) -> Option<Range<usize>> {
        match self {
            Self::ModuleContract { error, .. } => Some(error.range.clone()),
            _ => None,
        }
    }

    /// Consumers localize the actionable repair instead of inventing their own contract.
    #[must_use]
    pub fn remediation(&self) -> &'static str {
        use super::{object_module_version::ContractErrorKind as Kind, publish::PublishError};
        match self {
            Self::AcceptedBaseRequired => "supply_accepted_base",
            Self::AcceptedBaseUnexpected => "remove_accepted_base",
            Self::Source { error, .. } => match error.code {
                "accepted_base_pin_required"
                | "accepted_base_pin_invalid"
                | "source_baseline_untrusted" => "supply_accepted_base",
                "source_edit_native_restricted" | "source_edit_frozen" => {
                    "check_source_permissions"
                }
                _ => "repeat_version_preview",
            },
            Self::ModuleMissing { .. }
            | Self::ModuleContract {
                error:
                    super::object_module_version::ContractError {
                        kind:
                            Kind::MissingFunction
                            | Kind::NotExported
                            | Kind::RegistrationUnbound
                            | Kind::InitialVersionRequired
                            | Kind::InitialVersionConflict,
                        ..
                    },
                ..
            } => "version_migration",
            Self::ModuleContract {
                error:
                    super::object_module_version::ContractError {
                        kind: Kind::SourceChanged,
                        ..
                    },
                ..
            }
            | Self::Publication {
                error: PublishError::Conflict,
                ..
            } => "repeat_version_preview",
            Self::ModuleProtected { .. } => "provide_readable_module",
            Self::ModuleAmbiguous { .. }
            | Self::Publication {
                error: PublishError::UnsafePath,
                ..
            } => "use_regular_source_path",
            Self::Publication {
                error: PublishError::ReadOnly,
                ..
            } => "check_source_permissions",
            Self::ModuleContract { .. } => "repair_version_contract",
            Self::SchemeMismatch { .. } => "align_version_scheme",
            Self::MigrationUnsupported => "select_external_project",
            _ => "repair_native_version",
        }
    }
}

struct LoadedDescriptor {
    path: PathBuf,
    input: Vec<u8>,
    version: ProjectVersion,
    value_range: Range<usize>,
}

struct ParsedDescriptor<'a> {
    project_type: ProjectType,
    value: &'a str,
    value_range: Range<usize>,
}

/// Locate the single root descriptor matching the configured project type.
fn load_descriptor(
    source: &Path,
    project_type: ProjectType,
) -> Result<LoadedDescriptor, VersionError> {
    if matches!(
        project_type,
        ProjectType::Configuration | ProjectType::Extension
    ) {
        let path = source.join("Configuration.xml");
        let descriptor = read_descriptor(&path, project_type)?
            .ok_or_else(|| VersionError::DescriptorMissing { path: path.clone() })?;
        return Ok(descriptor);
    }

    external::load(source, project_type)
}

/// Read and inspect one bounded UTF-8 XML candidate.
fn read_descriptor(
    path: &Path,
    expected_type: ProjectType,
) -> Result<Option<LoadedDescriptor>, VersionError> {
    let metadata = fs::metadata(path).map_err(|source| VersionError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(VersionError::DescriptorMissing {
            path: path.to_path_buf(),
        });
    }
    let mut input = Vec::new();
    File::open(path)
        .and_then(|file| file.take(MAX_DESCRIPTOR_BYTES + 1).read_to_end(&mut input))
        .map_err(|source| VersionError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    if input.len() as u64 > MAX_DESCRIPTOR_BYTES {
        return Err(VersionError::DescriptorTooLarge {
            path: path.to_path_buf(),
        });
    }
    let text = std::str::from_utf8(&input).map_err(|source| VersionError::Io {
        path: path.to_path_buf(),
        source: io::Error::new(io::ErrorKind::InvalidData, source),
    })?;
    let Some(parsed) = parse_descriptor(text, path)? else {
        return Ok(None);
    };
    let ParsedDescriptor {
        project_type,
        value,
        value_range,
    } = parsed;
    if project_type != expected_type {
        return Ok(None);
    }
    let version = ProjectVersion::parse(value)?;
    Ok(Some(LoadedDescriptor {
        path: path.to_path_buf(),
        input,
        version,
        value_range,
    }))
}

/// Parse only the root object and its direct `Properties/Version` child.
fn parse_descriptor<'a>(
    input: &'a str,
    path: &Path,
) -> Result<Option<ParsedDescriptor<'a>>, VersionError> {
    let document = roxmltree::Document::parse_with_options(
        input,
        roxmltree::ParsingOptions {
            nodes_limit: 1_000_000,
            ..Default::default()
        },
    )
    .map_err(|source| VersionError::InvalidXml {
        path: path.to_path_buf(),
        source,
    })?;
    let root = document.root_element();
    if !root.has_tag_name((NAMESPACE, "MetaDataObject")) {
        return Ok(None);
    }
    let mut objects = root.children().filter(roxmltree::Node::is_element);
    let Some(object) = objects.next() else {
        return Ok(None);
    };
    if objects.next().is_some() || object.tag_name().namespace() != Some(NAMESPACE) {
        return Ok(None);
    }
    let project_type = match object.tag_name().name() {
        "Configuration" => {
            let is_extension = object
                .children()
                .filter(|node| node.has_tag_name((NAMESPACE, "Properties")))
                .flat_map(|node| node.children())
                .any(|node| node.has_tag_name((NAMESPACE, "ConfigurationExtensionPurpose")));
            if is_extension {
                ProjectType::Extension
            } else {
                ProjectType::Configuration
            }
        }
        "ExternalDataProcessor" => ProjectType::Processing,
        "ExternalReport" => ProjectType::Report,
        _ => return Ok(None),
    };
    let properties: Vec<_> = object
        .children()
        .filter(|node| node.has_tag_name((NAMESPACE, "Properties")))
        .collect();
    let [properties] = properties.as_slice() else {
        return Err(VersionError::VersionMissing {
            path: path.to_path_buf(),
        });
    };
    let versions: Vec<_> = properties
        .children()
        .filter(|node| node.has_tag_name((NAMESPACE, "Version")))
        .collect();
    let [version] = versions.as_slice() else {
        return Err(if versions.is_empty() {
            VersionError::VersionMissing {
                path: path.to_path_buf(),
            }
        } else {
            VersionError::VersionAmbiguous {
                path: path.to_path_buf(),
            }
        });
    };
    let children: Vec<_> = version.children().collect();
    let [text] = children.as_slice() else {
        return Err(VersionError::VersionMissing {
            path: path.to_path_buf(),
        });
    };
    if !text.is_text() {
        return Err(VersionError::VersionMissing {
            path: path.to_path_buf(),
        });
    }
    text.text().ok_or_else(|| VersionError::VersionMissing {
        path: path.to_path_buf(),
    })?;
    let range = text.range();
    Ok(Some(ParsedDescriptor {
        project_type,
        value: &input[range.clone()],
        value_range: range,
    }))
}

/// Publish a complete sibling file so interrupted writes cannot truncate the XML.
fn replace(path: &Path, contents: &[u8]) -> Result<(), VersionError> {
    let metadata = fs::metadata(path).map_err(|source| VersionError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    for sequence in 0..1_000_u16 {
        let temporary = path.with_extension(format!("xml.eska-version-{sequence}"));
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(VersionError::Io {
                    path: temporary,
                    source,
                });
            }
        };
        let write_result = file.write_all(contents).and_then(|()| file.sync_all());
        drop(file);
        let result = write_result
            .and_then(|()| fs::set_permissions(&temporary, metadata.permissions()))
            .and_then(|()| replace_published(&temporary, path));
        if let Err(source) = result {
            let _ = fs::remove_file(&temporary);
            return Err(VersionError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
        return Ok(());
    }
    Err(VersionError::Io {
        path: path.to_path_buf(),
        source: io::Error::new(
            io::ErrorKind::AlreadyExists,
            "no unique temporary descriptor path available",
        ),
    })
}

/// Atomically replace an existing descriptor where the operating system supports it.
#[cfg(not(windows))]
fn replace_published(temporary: &Path, path: &Path) -> io::Result<()> {
    fs::rename(temporary, path)
}

/// Replace an existing descriptor on Windows and restore it if publication fails.
#[cfg(windows)]
fn replace_published(temporary: &Path, path: &Path) -> io::Result<()> {
    let backup = path.with_extension("xml.eska-version-backup");
    fs::rename(path, &backup)?;
    if let Err(source) = fs::rename(temporary, path) {
        let _ = fs::rename(&backup, path);
        return Err(source);
    }
    fs::remove_file(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_bumps_four_component_versions_without_losing_widths() {
        let version = ProjectVersion::parse("01.0.2.01").expect("valid version");
        assert_eq!(version.to_string(), "01.0.2.01");
        assert_eq!(
            version.bumped(BumpKind::Patch).unwrap().to_string(),
            "01.0.3.01"
        );
        assert_eq!(
            version.bumped(BumpKind::Minor).unwrap().to_string(),
            "01.1.1.01"
        );
        assert_eq!(
            version.bumped(BumpKind::Major).unwrap().to_string(),
            "02.0.1.01"
        );
    }

    #[test]
    fn rejects_values_outside_the_designer_version_contract() {
        for value in [
            "1.2",
            "1.2.3.4.5",
            "1..3.4",
            "1.2.x.4",
            " 1.2.3.4",
            "01.2.3",
            "1.02.3",
            "1.2.03",
            "1.2.3-rc.1",
            "1.2.3+build",
            "1.2.+3",
            "1.2.٣",
        ] {
            assert!(matches!(
                ProjectVersion::parse(value),
                Err(VersionError::InvalidVersion { .. })
            ));
        }
    }

    /// Every bump keeps exactly three components and resets lower values to zero.
    #[test]
    fn three_component_versions_follow_numeric_semver_rules() {
        for (input, patch, minor, major) in [
            ("1.2.3", "1.2.4", "1.3.0", "2.0.0"),
            ("0.0.0", "0.0.1", "0.1.0", "1.0.0"),
        ] {
            let version = ProjectVersion::parse(input).unwrap();
            assert_eq!(version.scheme(), VersionScheme::ThreePart);
            assert_eq!(version.to_string(), input);
            for (kind, expected) in [
                (BumpKind::Patch, patch),
                (BumpKind::Minor, minor),
                (BumpKind::Major, major),
            ] {
                let next = version.bumped(kind).unwrap();
                assert_eq!(next.to_string(), expected);
                assert_eq!(next.scheme(), VersionScheme::ThreePart);
            }
            assert!(matches!(
                version.validate_scheme(VersionScheme::FourPart),
                Err(VersionError::SchemeMismatch { .. })
            ));
        }
    }

    /// Parsing and arithmetic reject overflow without mutating the original value.
    #[test]
    fn rejects_numeric_overflow_in_both_schemes() {
        for suffix in ["", ".01"] {
            for (input, kind) in [
                (format!("{}.0.0{suffix}", u64::MAX), BumpKind::Major),
                (format!("0.{}.0{suffix}", u64::MAX), BumpKind::Minor),
                (format!("0.0.{}{suffix}", u64::MAX), BumpKind::Patch),
            ] {
                let version = ProjectVersion::parse(&input).unwrap();
                assert!(matches!(
                    version.bumped(kind),
                    Err(VersionError::Overflow { .. })
                ));
                assert_eq!(version.to_string(), input);
            }
            assert!(ProjectVersion::parse(&format!("18446744073709551616.0.0{suffix}")).is_err());
        }
    }
}
