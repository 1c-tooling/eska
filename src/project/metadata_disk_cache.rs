//! Disposable private cache format; no serialized value is a public IDE protocol.
use super::Project;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    fs,
    io::{self, Read, Write},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

const VERSION: &str = concat!("metadata-3-", env!("CARGO_PKG_VERSION"));
const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// Cache failures are observable but never prevent reading source metadata.
#[derive(Clone, Copy, Debug, Default)]
pub struct DiskCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub writes: u64,
    pub failures: u64,
    pub bytes_read: u64,
    pub bytes_written: u64,
}

#[derive(Debug)]
pub struct DiskCache {
    root: PathBuf,
    source: PathBuf,
    stats: Cell<DiskCacheStats>,
}

#[derive(Serialize, Deserialize)]
struct Envelope<'a> {
    version: String,
    source_hash: [u8; 32],
    payload_hash: [u8; 32],
    #[serde(borrow)]
    payload: &'a serde_json::value::RawValue,
}

impl DiskCache {
    /// Bind storage to the selected project and canonical source directory.
    pub(crate) fn new(project: &Project) -> Self {
        Self {
            root: project.root().to_owned(),
            source: project.source().to_owned(),
            stats: Cell::default(),
        }
    }

    /// Return counters for this open session, including root discovery.
    pub(crate) const fn stats(&self) -> DiskCacheStats {
        self.stats.get()
    }

    /// Decode only a versioned, bounded entry matching the current source bytes.
    pub(crate) fn get<T: DeserializeOwned>(&self, key: &str, hash: [u8; 32]) -> Option<T> {
        let result = self.read(key, hash);
        let mut stats = self.stats.get();
        match &result {
            Ok(Some(_)) => stats.hits += 1,
            Ok(None) => stats.misses += 1,
            Err(_) => {
                stats.misses += 1;
                stats.failures += 1;
            }
        }
        self.stats.set(stats);
        result.ok().flatten()
    }

    /// Persist via an exclusive sibling temporary file; errors leave the source usable.
    pub(crate) fn put<T: Serialize>(&self, key: &str, hash: [u8; 32], value: &T) {
        if self.write(key, hash, value).is_err() {
            let mut stats = self.stats.get();
            stats.failures += 1;
            self.stats.set(stats);
        }
    }

    /// Inspect each managed directory without following a pre-existing symlink.
    fn directory(&self, create: bool) -> io::Result<PathBuf> {
        let mut path = self.root.clone();
        for component in [".eska", "cache", "metadata"] {
            path.push(component);
            if create {
                match fs::create_dir(&path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error),
                }
            }
            if !fs::symlink_metadata(&path)?.file_type().is_dir() {
                return Err(io::Error::other("cache directory is not a plain directory"));
            }
        }
        if create {
            let ignore = path.join(".gitignore");
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&ignore)
            {
                Ok(mut file) => file.write_all(b"*\n")?,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    if !fs::symlink_metadata(&ignore)?.file_type().is_file()
                        || fs::read(&ignore)? != b"*\n"
                    {
                        return Err(io::Error::other("cache ignore file differs"));
                    }
                }
                Err(error) => return Err(error),
            }
        }
        Ok(path)
    }

    /// File names contain only a hash, never source-controlled path components.
    fn filename(&self, key: &str) -> String {
        let mut hash = Sha256::new();
        hash.update(self.source.as_os_str().as_encoded_bytes());
        hash.update([0]);
        hash.update(VERSION);
        hash.update([0]);
        hash.update(key);
        let alphabet = b"0123456789abcdef";
        let mut name: String = hash
            .finalize()
            .iter()
            .flat_map(|byte| {
                [
                    char::from(alphabet[usize::from(byte >> 4)]),
                    char::from(alphabet[usize::from(byte & 15)]),
                ]
            })
            .collect();
        name.push_str(".json");
        name
    }

    /// Treat absence or outdated contents as a normal cache miss.
    fn read<T: DeserializeOwned>(&self, key: &str, hash: [u8; 32]) -> io::Result<Option<T>> {
        let directory = match self.directory(false) {
            Ok(path) => path,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let path = directory.join(self.filename(key));
        let metadata = match fs::symlink_metadata(&path) {
            Ok(value) => value,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        if !metadata.is_file() || metadata.len() > MAX_BYTES {
            return Ok(None);
        }
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)?;
        let mut stats = self.stats.get();
        stats.bytes_read += bytes.len() as u64;
        self.stats.set(stats);
        if bytes.len() as u64 > MAX_BYTES {
            return Ok(None);
        }
        let envelope: Envelope = serde_json::from_slice(&bytes)?;
        if envelope.version != VERSION || envelope.source_hash != hash {
            return Ok(None);
        }
        if <[u8; 32]>::from(Sha256::digest(envelope.payload.get().as_bytes()))
            != envelope.payload_hash
        {
            return Err(io::Error::other("cache checksum mismatch"));
        }
        Ok(Some(serde_json::from_str(envelope.payload.get())?))
    }

    /// Publish complete entries; concurrent sessions may replace equivalent entries safely.
    fn write<T: Serialize>(&self, key: &str, hash: [u8; 32], value: &T) -> io::Result<()> {
        let payload = serde_json::value::to_raw_value(value)?;
        let envelope = Envelope {
            version: VERSION.into(),
            source_hash: hash,
            payload_hash: Sha256::digest(payload.get().as_bytes()).into(),
            payload: &payload,
        };
        let bytes = serde_json::to_vec(&envelope)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Ok(());
        }
        let directory = self.directory(true)?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        let temporary = directory.join(format!(".{}-{nonce}.tmp", std::process::id()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let result = (|| {
            file.write_all(&bytes)?;
            // Cache loss after a power failure is recoverable; avoid fsync per descriptor.
            drop(file);
            fs::rename(&temporary, directory.join(self.filename(key)))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
        let mut stats = self.stats.get();
        stats.writes += 1;
        stats.bytes_written += bytes.len() as u64;
        self.stats.set(stats);
        Ok(())
    }
}
