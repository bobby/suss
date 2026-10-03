//! Fingerprint the implementation that produces portable artifacts.
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
};
fn sources(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("compiler source directory") {
        let path = entry.expect("compiler source entry").path();
        if path.is_dir() {
            sources(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
fn main() {
    let crate_root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let root = crate_root.parent().unwrap().parent().unwrap();
    let mut files = vec![
        root.join("Cargo.toml"),
        root.join("Cargo.lock"),
        crate_root.join("build.rs"),
    ];
    for name in ["suss-core", "suss-reader", "suss-compile"] {
        let directory = root.join("crates").join(name);
        files.push(directory.join("Cargo.toml"));
        sources(&directory.join("src"), &mut files);
        println!("cargo:rerun-if-changed={}", directory.join("src").display());
    }
    files.sort();
    let mut hash = Sha256::new();
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .components()
            .map(|component| component.as_os_str().to_str().expect("UTF-8 source path"))
            .collect::<Vec<_>>()
            .join("/");
        let bytes = fs::read(&path).expect("compiler fingerprint source");
        hash.update((relative.len() as u64).to_le_bytes());
        hash.update(relative.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    println!(
        "cargo:rustc-env=SUSS_COMPILER_SOURCE_SHA256={:x}",
        hash.finalize()
    );
}
