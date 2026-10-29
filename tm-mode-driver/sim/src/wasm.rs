//! Builds modes for `wasm32-unknown-unknown` from their tests.

use std::{path::Path, process::Command};

/// The wasm of the mode crate whose tests call it, built for
/// `wasm32-unknown-unknown` with the `mode` profile, see [`build_wasm`].
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

/// The cargo profile of the modes, the one they are deployed with.
pub const MODE_PROFILE: &str = "mode";

/// Builds a crate of the workspace for `wasm32-unknown-unknown` with the
/// [`MODE_PROFILE`] and returns its wasm. The build goes to `modes` in `target_dir`, e.g. the
/// `CARGO_TARGET_TMPDIR` of the tests, which the cargo running them does not lock.
/// Builds of all modes share it.
pub fn build_wasm(package: &str, manifest_dir: &Path, target_dir: &Path) -> Vec<u8> {
    let target_dir = target_dir.join("modes");
    let output = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .current_dir(manifest_dir)
        // Flags for the host the tests run on are no flags for wasm.
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .args([
            "build",
            "--profile",
            MODE_PROFILE,
            // The tests build the crates of the lock file, never others.
            "--locked",
            "-p",
            package,
            "--target",
            "wasm32-unknown-unknown",
            "--target-dir",
        ])
        .arg(&target_dir)
        .output()
        .expect("cargo could not be started");
    if !output.status.success() {
        let errors = String::from_utf8_lossy(&output.stderr);
        let hint = if errors.contains("target may not be installed") {
            "\nInstall the target with `rustup target add wasm32-unknown-unknown`."
        } else {
            ""
        };
        panic!("Building {package} for wasm failed:\n{errors}{hint}");
    }

    let file = target_dir
        .join("wasm32-unknown-unknown")
        .join(MODE_PROFILE)
        .join(format!("{}.wasm", package.replace('-', "_")));
    std::fs::read(&file).unwrap_or_else(|_| panic!("{} was not built", file.display()))
}
