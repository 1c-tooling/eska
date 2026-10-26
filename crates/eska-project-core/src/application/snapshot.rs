//! Exact context inputs are independent of derived analyzer configuration or executable availability.

use std::{fmt::Write, io::Read, path::Path};

use sha2::{Digest, Sha256};

use super::{AnalysisContext, ContextError};
use crate::project::designer_source::SourceError;

/// Bind an editor operation to selected application inputs without reading BSL or generated files.
#[derive(Debug)]
pub struct ContextSnapshot {
    pub context: AnalysisContext,
    pub fingerprint: String,
}

/// Resolve the application and hash only its manifests and declared root XML files.
///
/// # Errors
/// Rejects invalid/ambiguous composition and unreadable or oversized declared inputs.
pub fn resolve_snapshot(
    start: &Path,
    requested: Option<&str>,
) -> Result<ContextSnapshot, ContextError> {
    let context = super::resolve(start, requested)?;
    let mut hash = Sha256::new();
    field(&mut hash, b"eska-query-context-1");
    field(&mut hash, context.root().as_os_str().as_encoded_bytes());
    field(
        &mut hash,
        context.name().map_or("", |name| name.as_str()).as_bytes(),
    );
    let manifest = context.root().join("eska.toml");
    input(&mut hash, &manifest)?;
    for source in context.sources() {
        let member = source.project().root().join("eska.toml");
        if !source.is_reference() && member != manifest {
            input(&mut hash, &member)?;
        }
        field(
            &mut hash,
            source.project().source().as_os_str().as_encoded_bytes(),
        );
        input(
            &mut hash,
            &source.project().source().join(source.descriptor()),
        )?;
    }
    let fingerprint = hash
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            let _ = write!(output, "{byte:02x}");
            output
        });
    Ok(ContextSnapshot {
        context,
        fingerprint,
    })
}

/// Length prefixes separate native paths, names and digest bytes without requiring UTF-8 paths.
fn field(hash: &mut Sha256, bytes: &[u8]) {
    hash.update(bytes.len().to_le_bytes());
    hash.update(bytes);
}

/// Stream an independently bounded file so a large root XML does not become another retained snapshot.
fn input(hash: &mut Sha256, path: &Path) -> Result<(), ContextError> {
    const LIMIT: u64 = 64 * 1024 * 1024;
    let failure = |source| {
        ContextError::Source(Box::new(SourceError::Io {
            path: path.to_path_buf(),
            source,
        }))
    };
    let mut file = std::fs::File::open(path).map_err(failure)?.take(LIMIT + 1);
    let mut digest = Sha256::new();
    let mut buffer = [0; 8192];
    let mut length = 0;
    loop {
        let count = file.read(&mut buffer).map_err(failure)?;
        if count == 0 {
            break;
        }
        length += count as u64;
        if length > LIMIT {
            return Err(ContextError::Source(Box::new(
                SourceError::DescriptorTooLarge {
                    path: path.to_path_buf(),
                },
            )));
        }
        digest.update(&buffer[..count]);
    }
    field(hash, path.as_os_str().as_encoded_bytes());
    field(hash, &digest.finalize());
    Ok(())
}
