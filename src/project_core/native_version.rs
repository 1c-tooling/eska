//! Narrow project-version access for Designer XML root descriptors.

use std::{
    fmt, fs,
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    ops::Range,
    path::{Path, PathBuf},
};

use super::{Project, ProjectType};

const MAX_DESCRIPTOR_BYTES: u64 = 64 * 1024 * 1024;
const NAMESPACE: &str = "http://v8.1c.ru/8.3/MDClasses";

/// A validated four-component 1C project version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectVersion {
    components: [u64; 4],
    widths: [usize; 4],
}

impl ProjectVersion {
    /// Parse the Designer version format `revision.subrevision.version.build`.
    ///
    /// # Errors
    ///
    /// Returns [`VersionError::InvalidVersion`] unless the input contains exactly
    /// four dot-separated decimal components representable as `u64` values.
    pub fn parse(value: &str) -> Result<Self, VersionError> {
        let parts: Vec<_> = value.split('.').collect();
        if parts.len() != 4
            || parts
                .iter()
                .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return Err(VersionError::InvalidVersion {
                value: value.to_owned(),
            });
        }
        let mut components = [0; 4];
        let mut widths = [0; 4];
        for (index, part) in parts.into_iter().enumerate() {
            components[index] = part.parse().map_err(|_| VersionError::InvalidVersion {
                value: value.to_owned(),
            })?;
            widths[index] = part.len();
        }
        Ok(Self { components, widths })
    }

    /// Return the next version for the requested compatibility bump.
    ///
    /// Major, minor and patch map to the first three 1C components. Components
    /// below the bumped one restart at their documented initial values; build is 1.
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
        next.components[index] = next.components[index]
            .checked_add(1)
            .ok_or(VersionError::Overflow { kind })?;
        match kind {
            BumpKind::Major => next.components[1..].copy_from_slice(&[0, 1, 1]),
            BumpKind::Minor => next.components[2..].copy_from_slice(&[1, 1]),
            BumpKind::Patch => next.components[3] = 1,
        }
        Ok(next)
    }
}

impl fmt::Display for ProjectVersion {
    /// Preserve the width convention of every original numeric component.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:0w0$}.{:0w1$}.{:0w2$}.{:0w3$}",
            self.components[0],
            self.components[1],
            self.components[2],
            self.components[3],
            w0 = self.widths[0],
            w1 = self.widths[1],
            w2 = self.widths[2],
            w3 = self.widths[3],
        )
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

/// Version value and the root descriptor that owns it.
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

/// Read the version from the root Designer XML descriptor without changing it.
///
/// # Errors
///
/// Returns a structured descriptor, XML, version or filesystem failure.
pub fn inspect(project: &Project) -> Result<VersionInfo, VersionError> {
    let descriptor = load_descriptor(project)?;
    Ok(VersionInfo {
        path: descriptor.path,
        version: descriptor.version,
    })
}

/// Replace only the text bytes inside the root object's `Properties/Version` tag.
///
/// No XML serializer is used: BOM, line endings, whitespace, attribute ordering
/// and every byte outside the version text range remain unchanged.
///
/// # Errors
///
/// Returns a structured failure without publishing a partially written descriptor.
pub fn bump(project: &Project, kind: BumpKind) -> Result<BumpOutcome, VersionError> {
    let descriptor = load_descriptor(project)?;
    let current = descriptor.version.bumped(kind)?;
    let replacement = current.to_string();
    let mut output = Vec::with_capacity(
        descriptor.input.len() - descriptor.value_range.len() + replacement.len(),
    );
    output.extend_from_slice(&descriptor.input[..descriptor.value_range.start]);
    output.extend_from_slice(replacement.as_bytes());
    output.extend_from_slice(&descriptor.input[descriptor.value_range.end..]);
    replace(&descriptor.path, &output)?;
    Ok(BumpOutcome {
        path: descriptor.path,
        previous: descriptor.version,
        current,
        kind,
    })
}

#[derive(Debug)]
pub enum VersionError {
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
fn load_descriptor(project: &Project) -> Result<LoadedDescriptor, VersionError> {
    if matches!(
        project.configuration().project_type(),
        ProjectType::Configuration | ProjectType::Extension
    ) {
        let path = project.source().join("Configuration.xml");
        let descriptor = read_descriptor(&path, project.configuration().project_type())?
            .ok_or_else(|| VersionError::DescriptorMissing { path: path.clone() })?;
        return Ok(descriptor);
    }

    let mut found = None;
    for entry in fs::read_dir(project.source()).map_err(|source| VersionError::Io {
        path: project.source().to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| VersionError::Io {
            path: project.source().to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if entry.file_name() == "ConfigDumpInfo.xml"
            || !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
        {
            continue;
        }
        if let Some(descriptor) = read_descriptor(&path, project.configuration().project_type())? {
            if found.is_some() {
                return Err(VersionError::DescriptorAmbiguous {
                    path: project.source().to_path_buf(),
                });
            }
            found = Some(descriptor);
        }
    }
    found.ok_or_else(|| VersionError::DescriptorMissing {
        path: project.source().to_path_buf(),
    })
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
        for value in ["1.2.3", "1.2.3.4.5", "1..3.4", "1.2.x.4", " 1.2.3.4"] {
            assert!(matches!(
                ProjectVersion::parse(value),
                Err(VersionError::InvalidVersion { .. })
            ));
        }
    }
}
