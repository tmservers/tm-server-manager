//! Released modes never change how they play.
//!
//! `released.txt` holds a hash of every snapshot of the simulations of every built-in
//! mode. A change of rules shows in the snapshots, so a hash which does not match
//! means the change has to become a new version of the mode instead, e.g. `rounds-v2`
//! next to `rounds-v1`.
//!
//! `RELEASE_SNAPSHOTS=1 cargo test -p tm-mode-driver-builtin released` adds the
//! snapshots which are not listed yet, e.g. of a new version or a new simulation of a
//! released one. It never changes a listed hash.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    path::{Path, PathBuf},
};

use super::BuiltInMode;

const LIST: &str = "released.txt";

/// Where the snapshots of the simulations of a mode are, relative to the driver crates.
fn snapshots_of(mode: BuiltInMode) -> PathBuf {
    Path::new("modes")
        .join(mode.family())
        .join("tests")
        .join(format!("simulation_v{}", mode.version()))
        .join("snapshots")
}

/// The report of a snapshot, without the header insta writes. The header names the
/// test source, which may move without changing how the mode plays.
fn report(snapshot: &str) -> &str {
    match snapshot.strip_prefix("---\n").and_then(|s| s.split_once("\n---\n")) {
        Some((_, report)) => report,
        None => snapshot,
    }
}

/// FNV-1a, a hash which stays the same across platforms and versions of Rust.
fn hash(report: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in report.replace("\r\n", "\n").bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[derive(Debug, PartialEq, Eq)]
enum Problem {
    /// The rules of a released mode changed.
    Changed { mode: String, file: String },
    /// A listed snapshot is gone, which hides what the mode played.
    Missing { mode: String, file: String },
    /// A snapshot is not listed yet.
    Unlisted { mode: String, file: String, hash: String },
    /// A mode has no snapshots at all, nothing holds its rules.
    Unchecked { mode: String },
    /// An entry names no built-in mode.
    Unknown { mode: String },
}

/// Entries by mode and file to their hash.
type Entries = BTreeMap<(String, String), String>;

fn parse(list: &str) -> Entries {
    list.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut parts = line.split_whitespace();
            let (Some(mode), Some(file), Some(hash), None) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            else {
                panic!("{LIST}: `{line}` is no `<mode> <file> <hash>`");
            };
            ((mode.into(), file.into()), hash.into())
        })
        .collect()
}

/// Compares the listed hashes with the snapshots there are, per mode its files and their hashes.
fn check(listed: &Entries, found: &BTreeMap<String, Vec<(String, String)>>) -> Vec<Problem> {
    let mut problems = Vec::new();
    for (mode, _) in listed.keys() {
        if !found.contains_key(mode) {
            problems.push(Problem::Unknown { mode: mode.clone() });
        }
    }
    for (mode, files) in found {
        if files.is_empty() {
            problems.push(Problem::Unchecked { mode: mode.clone() });
        }
        for (file, hash) in files {
            match listed.get(&(mode.clone(), file.clone())) {
                Some(listed) if listed == hash => {}
                Some(_) => problems.push(Problem::Changed {
                    mode: mode.clone(),
                    file: file.clone(),
                }),
                None => problems.push(Problem::Unlisted {
                    mode: mode.clone(),
                    file: file.clone(),
                    hash: hash.clone(),
                }),
            }
        }
        for (listed_mode, file) in listed.keys() {
            if listed_mode == mode && !files.iter().any(|(found, _)| found == file) {
                problems.push(Problem::Missing {
                    mode: mode.clone(),
                    file: file.clone(),
                });
            }
        }
    }
    problems.dedup();
    problems
}

/// The snapshots of every built-in mode with their hashes.
fn found(root: &Path) -> BTreeMap<String, Vec<(String, String)>> {
    BuiltInMode::ALL
        .iter()
        .map(|mode| {
            let dir = snapshots_of(*mode);
            let mut files: Vec<(String, String)> = std::fs::read_dir(root.join(&dir))
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|e| e == "snap"))
                .map(|path| {
                    let snapshot = std::fs::read_to_string(&path).unwrap();
                    let file = dir.join(path.file_name().unwrap());
                    (file.to_string_lossy().replace('\\', "/"), hash(report(&snapshot)))
                })
                .collect();
            files.sort();
            (mode.name(), files)
        })
        .collect()
}

#[test]
fn released_modes_play_like_they_were_released() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let list_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(LIST);
    let mut list = std::fs::read_to_string(&list_path).unwrap();
    let problems = check(&parse(&list), &found(&root));

    let release = std::env::var_os("RELEASE_SNAPSHOTS").is_some();
    let mut failures = String::new();
    for problem in problems {
        match problem {
            Problem::Unlisted { mode, file, hash } if release => {
                writeln!(list, "{mode} {file} {hash}")
            }
            Problem::Unlisted { mode, file, .. } => writeln!(
                failures,
                "{file} of {mode} is not in {LIST}. `RELEASE_SNAPSHOTS=1 cargo test -p tm-mode-driver-builtin released` lists it."
            ),
            Problem::Changed { mode, file } => writeln!(
                failures,
                "{file} changed. {mode} is released and never changes how it plays, the change has to be a new version of the mode."
            ),
            Problem::Missing { mode, file } => writeln!(
                failures,
                "{file} of {mode} is gone. Released modes keep the simulations which show how they play."
            ),
            Problem::Unchecked { mode } => writeln!(
                failures,
                "{mode} has no snapshots in {}, nothing holds its rules.",
                snapshots_of(BuiltInMode::from_name(&mode).unwrap()).display()
            ),
            Problem::Unknown { mode } => {
                writeln!(failures, "{LIST} lists {mode}, which is no built-in mode.")
            }
        }
        .unwrap();
    }
    if release {
        std::fs::write(&list_path, list).unwrap();
    }
    assert!(failures.is_empty(), "{failures}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(entries: &[(&str, &str, &str)]) -> Entries {
        entries
            .iter()
            .map(|(m, f, h)| ((m.to_string(), f.to_string()), h.to_string()))
            .collect()
    }

    fn found(modes: &[(&str, &[(&str, &str)])]) -> BTreeMap<String, Vec<(String, String)>> {
        modes
            .iter()
            .map(|(mode, files)| {
                let files = files.iter().map(|(f, h)| (f.to_string(), h.to_string()));
                (mode.to_string(), files.collect())
            })
            .collect()
    }

    #[test]
    fn only_snapshots_as_listed_pass() {
        let listed = entries(&[("a-v1", "s1", "11"), ("a-v1", "s2", "22"), ("b-v1", "s3", "33")]);
        assert_eq!(
            check(&listed, &found(&[("a-v1", &[("s1", "11"), ("s2", "22")]), ("b-v1", &[("s3", "33")])])),
            []
        );

        let problems = check(
            &listed,
            &found(&[
                ("a-v1", &[("s1", "10"), ("s4", "44")]),
                ("b-v1", &[("s3", "33")]),
                ("c-v1", &[]),
            ]),
        );
        assert_eq!(
            problems,
            [
                Problem::Changed { mode: "a-v1".into(), file: "s1".into() },
                Problem::Unlisted { mode: "a-v1".into(), file: "s4".into(), hash: "44".into() },
                Problem::Missing { mode: "a-v1".into(), file: "s2".into() },
                Problem::Unchecked { mode: "c-v1".into() },
            ]
        );
    }

    #[test]
    fn entries_of_modes_which_do_not_exist_fail() {
        let listed = entries(&[("gone-v1", "s1", "11")]);
        assert_eq!(check(&listed, &found(&[])), [Problem::Unknown { mode: "gone-v1".into() }]);
    }

    #[test]
    fn only_the_report_counts_not_the_header() {
        let snapshot = "---\nsource: a.rs\nexpression: report\n---\nrounds · seed 1\n";
        let moved = "---\nsource: b.rs\nexpression: outcome.report\n---\nrounds · seed 1\n";
        assert_eq!(hash(report(snapshot)), hash(report(moved)));
        assert_ne!(hash(report(snapshot)), hash(report("---\nsource: a.rs\n---\nrounds · seed 2\n")));
        assert_eq!(hash("a\r\nb"), hash("a\nb"));
        // The hash may never change, it is stored.
        assert_eq!(hash("rounds"), "6e99a8003d10ccb8");
    }

    #[test]
    fn every_mode_has_its_own_snapshots() {
        assert_eq!(
            snapshots_of(BuiltInMode::KnockoutV1),
            Path::new("modes/knockout/tests/simulation_v1/snapshots")
        );
    }
}
