//! The filesystem inputs a build observed (#272): every file whose contents it
//! read, every path whose presence or symlink target it probed, and every
//! directory it listed. A recorded build may be reused only while every
//! observation still holds. Recording is thread-local and inactive unless a
//! caller wraps a build in `record`, so sessions and the REPL pay nothing.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recorded {
    /// Absolute path to the SHA-256 of the complete contents read.
    pub files: BTreeMap<PathBuf, String>,
    /// Absolute path to its observed state: see `state`.
    pub probes: BTreeMap<PathBuf, String>,
    /// Absolute directory to the SHA-256 of its sorted entry names.
    pub directories: BTreeMap<PathBuf, String>,
}

thread_local! {
    static ACTIVE: RefCell<Option<Recorded>> = const { RefCell::new(None) };
}

/// Run `build`, returning what it observed. Nested recording is not
/// supported; an inner `record` would steal the outer observations.
pub fn record<T>(build: impl FnOnce() -> T) -> (T, Recorded) {
    ACTIVE.with(|active| {
        assert!(active.borrow().is_none(), "nested input recording");
        *active.borrow_mut() = Some(Recorded::default());
    });
    // Restore the inactive state even if the build unwinds.
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ACTIVE.with(|active| active.borrow_mut().take());
        }
    }
    let reset = Reset;
    let value = build();
    let recorded = ACTIVE.with(|active| active.borrow_mut().take()).unwrap_or_default();
    drop(reset);
    (value, recorded)
}

fn with(update: impl FnOnce(&mut Recorded)) {
    ACTIVE.with(|active| {
        if let Some(recorded) = active.borrow_mut().as_mut() {
            update(recorded);
        }
    });
}
fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// The build read all of `contents` from `path`.
pub fn file(path: &Path, contents: &[u8]) {
    with(|recorded| {
        recorded.files.insert(absolute(path), digest(contents));
    });
}
/// The build examined whether `path` exists, what it is, or where it leads.
pub fn probe(path: &Path) {
    with(|recorded| {
        recorded.probes.insert(absolute(path), state(path));
    });
}
/// The build listed `path`'s entries. Listings inside the active build
/// directory are not recorded: the build's own entries would change them.
pub fn directory(path: &Path) {
    with(|recorded| {
        if !super::inside_build_dir(path) {
            recorded.directories.insert(absolute(path), listing(path));
        }
    });
}
/// Record a directory tree completely: every listing and every file. Each
/// real directory is visited once, so symlink cycles terminate, and the
/// active build directory is skipped.
pub fn tree(path: &Path) {
    let mut visited = std::collections::BTreeSet::new();
    tree_from(path, &mut visited);
}
fn tree_from(path: &Path, visited: &mut std::collections::BTreeSet<PathBuf>) {
    if super::inside_build_dir(path) {
        return;
    }
    probe(path);
    if !path.is_dir() {
        if let Ok(contents) = std::fs::read(path) {
            file(path, &contents);
        }
        return;
    }
    if !visited.insert(path.canonicalize().unwrap_or_else(|_| absolute(path))) {
        return;
    }
    directory(path);
    if let Ok(entries) = std::fs::read_dir(path) {
        let mut entries: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
        entries.sort();
        for entry in entries {
            tree_from(&entry, visited);
        }
    }
}
/// Whether the build should skip `path` entirely (it is build-directory state).
pub fn skipped(path: &Path) -> bool {
    super::inside_build_dir(path)
}

/// What a probe distinguishes: absence, the canonical target of a file or
/// directory (so a swapped symlink changes it), or another kind of entry.
fn state(path: &Path) -> String {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "absent".into(),
        Err(error) => format!("error:{:?}", error.kind()),
        Ok(_) => match path.canonicalize() {
            Ok(target) if target.is_file() => format!("file:{}", target.display()),
            Ok(target) if target.is_dir() => format!("dir:{}", target.display()),
            Ok(target) => format!("other:{}", target.display()),
            Err(error) => format!("unresolved:{:?}", error.kind()),
        },
    }
}
fn listing(path: &Path) -> String {
    match std::fs::read_dir(path) {
        Ok(entries) => {
            let mut names: Vec<_> = entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            digest(names.join("\0").as_bytes())
        }
        Err(error) => format!("error:{:?}", error.kind()),
    }
}

impl Recorded {
    /// The first observation that no longer holds, if any.
    pub fn changed(&self) -> Option<String> {
        for (path, expected) in &self.probes {
            if &state(path) != expected {
                return Some(format!("{} changed kind or target", path.display()));
            }
        }
        for (path, expected) in &self.directories {
            if &listing(path) != expected {
                return Some(format!("{} gained or lost entries", path.display()));
            }
        }
        for (path, expected) in &self.files {
            match std::fs::read(path) {
                Ok(contents) if &digest(&contents) == expected => {}
                _ => return Some(format!("{} changed", path.display())),
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recorded_observations_detect_each_kind_of_change() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("a.sus");
        std::fs::write(&source, "(ns a)").unwrap();
        let missing = root.path().join("b.sus");
        let ((), recorded) = record(|| {
            file(&source, b"(ns a)");
            probe(&source);
            probe(&missing);
            directory(root.path());
        });
        assert_eq!(recorded.files.len(), 1);
        assert_eq!(recorded.changed(), None);
        // Inactive outside `record`.
        file(&source, b"ignored");
        std::fs::write(&source, "(ns a) 1").unwrap();
        assert!(recorded.changed().unwrap().contains("a.sus changed"));
        std::fs::write(&source, "(ns a)").unwrap();
        std::fs::write(&missing, "(ns b)").unwrap();
        assert!(recorded.changed().unwrap().contains("b.sus changed kind"));
        std::fs::remove_file(&missing).unwrap();
        std::fs::write(root.path().join("c.sus"), "").unwrap();
        assert!(recorded.changed().unwrap().contains("gained or lost"));
    }
    #[cfg(unix)]
    #[test]
    fn tree_records_each_directory_once_through_symlink_cycles() {
        let root = tempfile::tempdir().unwrap();
        let wit = root.path().join("wit");
        std::fs::create_dir_all(wit.join("deps")).unwrap();
        std::fs::write(wit.join("api.wit"), "package a:b;").unwrap();
        std::os::unix::fs::symlink(&wit, wit.join("deps/loop")).unwrap();
        let ((), recorded) = record(|| tree(&wit));
        assert_eq!(recorded.files.len(), 1);
        assert_eq!(recorded.directories.len(), 2);
        assert_eq!(recorded.changed(), None);
    }
}
