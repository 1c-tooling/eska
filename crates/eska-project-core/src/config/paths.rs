//! Manifest paths use portable separators without changing native filesystem identities.

use std::path::{Path, PathBuf};

use serde::{Serialize, Serializer};

pub(super) struct Portable<'a>(pub(super) &'a Path);

impl Serialize for Portable<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        one(self.0, serializer)
    }
}

pub(super) fn one<S: Serializer>(path: &Path, serializer: S) -> Result<S::Ok, S::Error> {
    #[cfg(windows)]
    {
        let value = path
            .to_str()
            .ok_or_else(|| serde::ser::Error::custom("path is not UTF-8"))?;
        serializer.serialize_str(&value.replace('\\', "/"))
    }
    #[cfg(not(windows))]
    path.serialize(serializer)
}

pub(super) fn many<S: Serializer>(paths: &[PathBuf], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(paths.iter().map(|path| Portable(path)))
}
