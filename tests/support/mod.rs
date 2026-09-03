use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

pub struct TestDir(pub PathBuf);

impl TestDir {
    pub fn new() -> Self {
        let base =
            std::env::var_os("ESKA_TEST_ROOT").map_or_else(std::env::temp_dir, PathBuf::from);
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after Unix epoch")
            .as_nanos();
        let path = base.join(format!("eska-test-{}-{unique} каталог", std::process::id()));
        fs::create_dir(&path).expect("create isolated test directory");
        Self(fs::canonicalize(path).expect("canonical test directory"))
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove owned test directory");
    }
}
