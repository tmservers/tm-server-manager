//! Builds the first-party modes for the integration tests.

use std::{path::PathBuf, process::Command};

/// Builds a mode crate of the workspace for wasm32-unknown-unknown and returns the module.
pub fn build_mode(package: &str) -> Vec<u8> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let target_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("modes");
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .current_dir(&workspace)
        .args([
            "build",
            "-p",
            package,
            "--target",
            "wasm32-unknown-unknown",
            "--release",
            "--target-dir",
        ])
        .arg(&target_dir)
        .status()
        .expect("cargo could not be started");
    assert!(status.success(), "Building {package} failed");

    let file = format!("{}.wasm", package.replace('-', "_"));
    std::fs::read(
        target_dir
            .join("wasm32-unknown-unknown/release")
            .join(&file),
    )
    .unwrap_or_else(|_| panic!("{file} was not built"))
}
