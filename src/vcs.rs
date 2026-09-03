//! The narrow Git initialization boundary; repository workflows are not implemented.

use std::path::Path;

pub fn initialize(root: &Path) -> Result<(), Box<gix::init::Error>> {
    // Ignore user/system config and Git environment overrides: initialization
    // must affect only the directory owned by the creation transaction.
    gix::ThreadSafeRepository::init_opts(
        root,
        gix::create::Kind::WithWorktree,
        gix::create::Options::default(),
        gix::open::Options::isolated(),
    )
    .map(|_| ())
    .map_err(Box::new)
}
