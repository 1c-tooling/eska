//! Preserve the actual compilation target in the installation contract.

/// Cargo supplies TARGET to build scripts for native and cross compilation alike.
fn main() -> Result<(), std::env::VarError> {
    println!(
        "cargo:rustc-env=ESKA_BUILD_TARGET={}",
        std::env::var("TARGET")?
    );
    Ok(())
}
