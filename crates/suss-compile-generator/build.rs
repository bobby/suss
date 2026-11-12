//! The compiler identity for suss-compile built as its bootstrap generator
//! (#383): the same digest of the same sources, without the native host.
include!("../suss-compile/build_identity.rs");

fn main() {
    let crate_root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let root = crate_root.parent().unwrap().parent().unwrap().to_path_buf();
    println!("cargo:rustc-env=SUSS_COMPILER_SOURCE_SHA256={}", compiler_identity(&root));
    println!("cargo:rustc-check-cfg=cfg(suss_host)");
    println!("cargo:rustc-check-cfg=cfg(suss_generator)");
    println!("cargo:rustc-cfg=suss_generator");
}
