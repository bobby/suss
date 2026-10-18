//! Development-only Java-free compiler-native field table source generator.
//! Writes one explicitly selected Rust source file; no bootstrap/Session/load.
use std::{env, fs, path::PathBuf};
use suss_compile::portable::bootstrap;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args_os().skip(1);
    let output = PathBuf::from(
        arguments
            .next()
            .ok_or("expected explicit Rust output path")?,
    );
    if arguments.next().is_some() {
        return Err("expected only explicit Rust output path".into());
    }
    let source = bootstrap::generate_native_field_tables()?;
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output, source.as_bytes())?;
    eprintln!(
        "native-field-codegen wrote={} bytes={} bootstrap_regeneration_required=true",
        output.display(),
        source.len()
    );
    Ok(())
}
