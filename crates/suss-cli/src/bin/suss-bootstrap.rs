//! Development-only, Java-free deterministic bootstrap artifact generator.
use std::{env, fs, path::PathBuf};
use suss_compile::portable::{bootstrap, resolve::Phase};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args_os().skip(1);
    let output = PathBuf::from(arguments.next().ok_or("expected output directory")?);
    if arguments.next().is_some() {
        return Err("expected only output directory".into());
    }
    fs::create_dir_all(&output)?;
    for phase in [Phase::Runtime, Phase::Macro] {
        let (wasm, manifest) = bootstrap::generate(phase)?;
        let name = bootstrap::phase_name(phase);
        fs::write(output.join(format!("{name}.wasm")), wasm)?;
        let mut json = serde_json::to_vec_pretty(&manifest)?;
        json.push(b'\n');
        fs::write(output.join(format!("{name}.json")), json)?;
    }
    Ok(())
}
