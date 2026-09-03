use std::{
    fs, io,
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
        // Clock resolution is not a uniqueness guarantee across parallel tests.
        // Exclusively claim a path and retry collisions without touching it.
        for attempt in 0..1024 {
            let path = base.join(format!(
                "eska-test-{}-{unique}-{attempt} каталог",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(fs::canonicalize(path).expect("canonical test directory")),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create isolated test directory: {error}"),
            }
        }
        panic!("could not claim an isolated test directory after 1024 attempts");
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove owned test directory");
    }
}
