//! Reuse complete support snapshots only after validating their source content and file inventory.
use super::{ProjectSession, SupportSnapshot};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

const KEY: &str = "support-snapshot-2";
const CHUNK: usize = 16;
const MAX_FILES: usize = 1_000_000;
const MAX_INPUT: u64 = 1024 * 1024 * 1024;

impl ProjectSession {
    /// No disk-cache opt-in means no extra scan or cache writes.
    pub(super) fn support_fingerprint(&self) -> Option<[u8; 32]> {
        self.source.disk_cache.as_ref()?;
        fingerprint(self.project().source(), self.source.descriptor()).ok()
    }

    /// Cache corruption, unsupported paths and missing entries fall back to the ordinary scanner.
    pub(super) fn load_support_pages(&self, hash: [u8; 32]) -> Option<Vec<Arc<SupportSnapshot>>> {
        let cache = self.source.disk_cache.as_ref()?;
        let key = format!("{KEY}:{}", self.source.root().id());
        let count: usize = cache.get(&key, hash)?;
        if count == 0 || count > MAX_FILES {
            return None;
        }
        let mut pages = Vec::new();
        for part in 0..count.div_ceil(CHUNK) {
            let chunk: Vec<SupportSnapshot> = cache.get(&format!("{key}:{part}"), hash)?;
            if chunk.len() != CHUNK.min(count - part * CHUNK) {
                return None;
            }
            pages.extend(chunk);
        }
        if pages.is_empty()
            || pages.iter().enumerate().any(|(index, page)| {
                page.next_offset != (index + 1 < pages.len()).then_some(index + 1)
                    || !page.diagnostics.is_empty()
                    || page.files.iter().any(|file| {
                        file.path.is_absolute()
                            || file
                                .path
                                .components()
                                .any(|part| !matches!(part, std::path::Component::Normal(_)))
                    })
            })
        {
            return None;
        }
        compact_pages(pages)
    }

    /// Publish only a successful complete pass over a source that stayed unchanged during analysis.
    pub(super) fn save_support_pages(&self, hash: [u8; 32], pages: &[Arc<SupportSnapshot>]) {
        if pages.iter().any(|page| !page.diagnostics.is_empty())
            || self.support_fingerprint() != Some(hash)
        {
            return;
        }
        if let Some(cache) = &self.source.disk_cache {
            let key = format!("{KEY}:{}", self.source.root().id());
            for (part, chunk) in pages.chunks(CHUNK).enumerate() {
                cache.put(
                    &format!("{key}:{part}"),
                    hash,
                    &chunk.iter().map(Arc::as_ref).collect::<Vec<_>>(),
                );
            }
            // Publish the manifest last; missing or mismatched chunks reject the whole snapshot.
            cache.put(&key, hash, &pages.len());
        }
    }
}

/// Hash descriptor/predefined XML and rules bytes and every relative entry, but ignore module and form/template payload contents.
/// Streaming reads stay bounded; symlinks disable reuse rather than weaken containment checks.
fn fingerprint(root: &Path, root_descriptor: &Path) -> io::Result<[u8; 32]> {
    let mut pending = vec![PathBuf::new()];
    let mut count = 0;
    let mut total = 0_u64;
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    while let Some(relative) = pending.pop() {
        let mut entries = fs::read_dir(root.join(&relative))?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            count += 1;
            if count > MAX_FILES {
                return Err(io::Error::other("support inventory limit"));
            }
            let path = relative.join(entry.file_name());
            let kind = entry.file_type()?;
            let name = path.as_os_str().as_encoded_bytes();
            hash.update(name.len().to_le_bytes());
            hash.update(name);
            if kind.is_dir() {
                hash.update([0]);
                pending.push(path);
            } else if kind.is_file() {
                hash.update([1]);
                let payload = path
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|name| name.eq_ignore_ascii_case("Ext"))
                    && path
                        .file_name()
                        .is_none_or(|name| !name.eq_ignore_ascii_case("Predefined.xml"))
                    && path != root_descriptor;
                if (path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("xml"))
                    && !payload)
                    || path
                        .file_name()
                        .is_some_and(|name| name.eq_ignore_ascii_case("ParentConfigurations.bin"))
                {
                    let mut file = fs::File::open(entry.path())?;
                    let mut content = Sha256::new();
                    loop {
                        let size = file.read(&mut buffer)?;
                        if size == 0 {
                            break;
                        }
                        total += size as u64;
                        if total > MAX_INPUT {
                            return Err(io::Error::other("support content limit"));
                        }
                        content.update(&buffer[..size]);
                    }
                    hash.update(content.finalize());
                }
            } else {
                return Err(io::Error::other(
                    "support inventory contains a special file",
                ));
            }
        }
    }
    Ok(hash.finalize().into())
}

/// Cached pages need no XML work; coalesce them to avoid thousands of extension-host round trips.
fn compact_pages(pages: Vec<SupportSnapshot>) -> Option<Vec<Arc<SupportSnapshot>>> {
    let mut result: Vec<SupportSnapshot> = Vec::new();
    let mut bytes = 0;
    for mut page in pages {
        let size = serde_json::to_vec(&page).ok()?.len();
        if bytes + size > 4 * 1024 * 1024 || result.is_empty() {
            result.push(page);
            bytes = size;
        } else if let Some(last) = result.last_mut() {
            last.objects.append(&mut page.objects);
            last.files.append(&mut page.files);
            last.suppliers.append(&mut page.suppliers);
            bytes += size;
        }
    }
    let count = result.len();
    for (index, page) in result.iter_mut().enumerate() {
        page.next_offset = (index + 1 < count).then_some(index + 1);
    }
    Some(result.into_iter().map(Arc::new).collect())
}
