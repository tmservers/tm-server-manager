//! Builds modes for `wasm32-unknown-unknown` from their tests.

use std::{path::Path, process::Command};

/// The wasm of the mode crate whose tests call it, built for
/// `wasm32-unknown-unknown` in release, see [`build_wasm`].
#[macro_export]
macro_rules! wasm {
    () => {
        $crate::build_wasm(
            env!("CARGO_PKG_NAME"),
            ::std::path::Path::new(env!("CARGO_MANIFEST_DIR")),
            ::std::path::Path::new(env!("CARGO_TARGET_TMPDIR")),
        )
    };
}

/// Builds a crate of the workspace for `wasm32-unknown-unknown` in release and
/// returns its wasm. The build goes to `modes` in `target_dir`, e.g. the
/// `CARGO_TARGET_TMPDIR` of the tests, which the cargo running them does not lock.
/// Builds of all modes share it.
pub fn build_wasm(package: &str, manifest_dir: &Path, target_dir: &Path) -> Vec<u8> {
    let target_dir = target_dir.join("modes");
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .current_dir(manifest_dir)
        .args([
            "build",
            "--release",
            "-p",
            package,
            "--target",
            "wasm32-unknown-unknown",
            "--target-dir",
        ])
        .arg(&target_dir)
        .status()
        .expect("cargo could not be started");
    assert!(status.success(), "Building {package} failed");

    let file = target_dir
        .join("wasm32-unknown-unknown/release")
        .join(format!("{}.wasm", package.replace('-', "_")));
    std::fs::read(&file).unwrap_or_else(|_| panic!("{} was not built", file.display()))
}
