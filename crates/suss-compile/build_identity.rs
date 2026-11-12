// Shared by suss-compile's and suss-compile-generator's build scripts
// (`include!`d): the compiler identity both packages embed as
// SUSS_COMPILER_SOURCE_SHA256, so generated bootstrap artifacts record the
// identity of the compiler that loads them (#383).
fn sources(directory: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("compiler source directory") {
        let path = entry.expect("compiler source entry").path();
        if path.is_dir() {
            sources(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs" || extension == "sus") {
            files.push(path);
        }
    }
}
/// The compiler source digest for the repository at `root`, printing the
/// rerun-if-changed lines for its inputs.
fn compiler_identity(root: &std::path::Path) -> String {
    use sha2::{Digest, Sha256};
    let crate_root = root.join("crates/suss-compile");
    let mut files = vec![
        root.join("Cargo.toml"),
        root.join("Cargo.lock"),
        crate_root.join("build.rs"),
        crate_root.join("build_identity.rs"),
        // Executable runtime dependencies are part of the artifact identity,
        // even when their implementation is supplied as embedded bytes.
        root.join("runtime/numeric/artifact/numeric.wasm"),
        root.join("runtime/numeric/artifact/manifest.json"),
        // The shipped command embeds upstream WIT, so changed source/lock bytes
        // must invalidate compiled artifacts even without a Rust source edit.
        root.join("docs/roadmap/wasi-wit-lock.json"),
        root.join("vendor/wasi/wasi-wit-0.3.1/cli/cli.wit"),
        root.join("vendor/wasi/wasi-wit-0.3.1/cli/deps/clocks.wit"),
        root.join("vendor/wasi/wasi-wit-0.3.1/cli/deps/filesystem.wit"),
        root.join("vendor/wasi/wasi-wit-0.3.1/cli/deps/random.wit"),
        root.join("vendor/wasi/wasi-wit-0.3.1/cli/deps/sockets.wit"),
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
        let bytes = std::fs::read(&path).expect("compiler fingerprint source");
        hash.update((relative.len() as u64).to_le_bytes());
        hash.update(relative.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    format!("{:x}", hash.finalize())
}
