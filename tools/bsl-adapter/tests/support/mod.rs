//! Each file-based test owns a unique playground directory and never edits the native corpus.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub struct TestDirectory(pub PathBuf);

impl TestDirectory {
    /// Tests require the explicitly configured adjacent playground, not a system temp directory.
    pub fn new() -> Self {
        let root = std::env::var_os("ESKA_TEST_ROOT").expect("ESKA_TEST_ROOT is required");
        let root = Path::new(&root)
            .canonicalize()
            .expect("existing playground");
        assert!(root.is_dir());
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = root.join(format!(
            "eska-bsl-adapter-test-{}-{timestamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).expect("exclusive test directory");
        Self(directory)
    }
}

impl Drop for TestDirectory {
    /// Delete only the directory successfully claimed by this test.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Resolve the checked-in corpus through the adapter's location inside the project.
pub fn native_corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("tests/fixtures/eska-format/designer")
}
