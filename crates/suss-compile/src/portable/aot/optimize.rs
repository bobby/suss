//! Whole-component Binaryen optimization of AOT builds (ADR-0005, #357).
//!
//! The component's runtime, bindings, fragment and adapter modules merge into
//! one module with `wasm-merge`, in instantiation order, so their start
//! functions run in the same order. Only the adapter's exports stay exported,
//! and `wasm-opt -O3` optimizes the result. Binaryen is a separately installed
//! tool: `wasm-opt` from `SUSS_WASM_OPT` or `PATH`, with `wasm-merge` beside it.
use super::Diagnostic;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The oldest Binaryen release whose output has been checked against Suss's
/// baseline and wasmtime 49.
pub(crate) const MINIMUM_VERSION: u32 = 133;
/// Every proposal wasmtime 49 accepts. Newer ones stay off: Binaryen would
/// otherwise narrow exported types to exact references (custom descriptors)
/// or write imports in the compact encoding, which wasmtime 49 rejects.
const FEATURES: &[&str] = &[
    "--all-features",
    "--disable-custom-descriptors",
    "--disable-compact-imports",
    "--disable-strings",
    "--disable-stack-switching",
    "--disable-shared-everything",
    "--disable-fp16",
    "--disable-multibyte",
    "--disable-custom-page-sizes",
    "--disable-wide-arithmetic",
    "--disable-acquire-release-atomics",
    "--disable-relaxed-atomics",
];
/// The adapter's exports, renamed so that merging can't rename them for a
/// conflict with another module's export.
const PREFIX: &str = "suss.out.";

fn error(message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        span: 0..0,
        message: message.into(),
    }
}

struct Binaryen {
    opt: PathBuf,
    merge: PathBuf,
}

fn binaryen() -> Result<Binaryen, Diagnostic> {
    let opt =
        std::env::var_os("SUSS_WASM_OPT").map_or_else(|| PathBuf::from("wasm-opt"), PathBuf::from);
    let merge = match opt.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        Some(parent) => parent.join("wasm-merge"),
        None => PathBuf::from("wasm-merge"),
    };
    let output = Command::new(&opt).arg("--version").output().map_err(|failure| {
        error(format!(
            "--optimize needs Binaryen {MINIMUM_VERSION} or later: running {} failed ({failure}). \
             Install Binaryen, or set SUSS_WASM_OPT to its wasm-opt",
            opt.display()
        ))
    })?;
    let text = String::from_utf8_lossy(&output.stdout);
    let version = text
        .split("version ")
        .nth(1)
        .and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|digits| digits.parse::<u32>().ok())
        .ok_or_else(|| error(format!("Unrecognized wasm-opt version: {}", text.trim())))?;
    if version < MINIMUM_VERSION {
        return Err(error(format!(
            "--optimize needs Binaryen {MINIMUM_VERSION} or later; {} is version {version}",
            opt.display()
        )));
    }
    Ok(Binaryen { opt, merge })
}

/// A copy of `module` whose exports are `rename`'s answer for each export
/// name, dropping those it maps to None. Other sections are copied as they are.
fn rewrite_exports(
    module: &[u8],
    rename: impl Fn(&str) -> Option<String>,
) -> Result<Vec<u8>, Diagnostic> {
    use wasmparser::{ExternalKind, Parser, Payload};
    let invalid =
        |failure: wasmparser::BinaryReaderError| error(format!("Invalid module: {failure}"));
    let mut output = module[..8].to_vec();
    for payload in Parser::new(0).parse_all(module) {
        let payload = payload.map_err(invalid)?;
        let Some((id, range)) = payload.as_section() else {
            continue;
        };
        if let Payload::ExportSection(reader) = payload {
            let mut exports = wasm_encoder::ExportSection::new();
            for export in reader {
                let export = export.map_err(invalid)?;
                let Some(name) = rename(export.name) else {
                    continue;
                };
                let kind = match export.kind {
                    ExternalKind::Func => wasm_encoder::ExportKind::Func,
                    ExternalKind::Table => wasm_encoder::ExportKind::Table,
                    ExternalKind::Memory => wasm_encoder::ExportKind::Memory,
                    ExternalKind::Global => wasm_encoder::ExportKind::Global,
                    ExternalKind::Tag => wasm_encoder::ExportKind::Tag,
                    other => return Err(error(format!("Unsupported export kind {other:?}"))),
                };
                exports.export(&name, kind, export.index);
            }
            wasm_encoder::Section::append_to(&exports, &mut output);
        } else {
            let range = range.start as usize..range.end as usize;
            output.push(id);
            wasm_encoder::Encode::encode(&(range.len() as u32), &mut output);
            output.extend_from_slice(&module[range]);
        }
    }
    Ok(output)
}

/// A copy of `module` whose imports take `rename`'s module name where it
/// gives one. Other sections are copied as they are.
pub(crate) fn rename_import_modules(
    module: &[u8],
    rename: impl Fn(&str) -> Option<String>,
) -> Result<Vec<u8>, Diagnostic> {
    use wasm_encoder::reencode::{Reencode, RoundtripReencoder};
    use wasmparser::{Parser, Payload};
    let invalid =
        |failure: wasmparser::BinaryReaderError| error(format!("Invalid module: {failure}"));
    let mut output = module[..8].to_vec();
    for payload in Parser::new(0).parse_all(module) {
        let payload = payload.map_err(invalid)?;
        let Some((id, range)) = payload.as_section() else {
            continue;
        };
        if let Payload::ImportSection(reader) = payload {
            let mut imports = wasm_encoder::ImportSection::new();
            for import in reader.into_imports() {
                let import = import.map_err(invalid)?;
                let ty = RoundtripReencoder
                    .entity_type(import.ty)
                    .map_err(|failure| error(format!("Invalid import: {failure}")))?;
                let module = rename(import.module).unwrap_or_else(|| import.module.to_owned());
                imports.import(&module, import.name, ty);
            }
            wasm_encoder::Section::append_to(&imports, &mut output);
        } else {
            let range = range.start as usize..range.end as usize;
            output.push(id);
            wasm_encoder::Encode::encode(&(range.len() as u32), &mut output);
            output.extend_from_slice(&module[range]);
        }
    }
    Ok(output)
}

fn run(command: &mut Command, tool: &Path) -> Result<(), Diagnostic> {
    let output = command
        .output()
        .map_err(|failure| error(format!("Running {} failed: {failure}", tool.display())))?;
    if !output.status.success() {
        return Err(error(format!(
            "{} failed ({}): {}",
            tool.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

/// Merge `modules`, named as their importers name them and in instantiation
/// order, the last being the adapter, and optimize the result. The merged
/// module exports what the adapter exported, and imports only what no module
/// in the list provides.
pub(crate) fn merge(modules: &[(String, Vec<u8>)]) -> Result<Vec<u8>, Diagnostic> {
    let binaryen = binaryen()?;
    let directory = std::env::temp_dir().join(format!(
        "suss-optimize-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos())
    ));
    std::fs::create_dir_all(&directory)
        .map_err(|failure| error(format!("Creating {}: {failure}", directory.display())))?;
    let result = (|| {
        let write = |name: &str, bytes: &[u8]| {
            let path = directory.join(name);
            std::fs::write(&path, bytes)
                .map_err(|failure| error(format!("Writing {}: {failure}", path.display())))?;
            Ok::<_, Diagnostic>(path)
        };
        let mut merge = Command::new(&binaryen.merge);
        merge.args(FEATURES).arg("--rename-export-conflicts");
        let last = modules.len() - 1;
        for (index, (name, bytes)) in modules.iter().enumerate() {
            let bytes = if index == last {
                rewrite_exports(bytes, |export| Some(format!("{PREFIX}{export}")))?
            } else {
                bytes.clone()
            };
            merge
                .arg(write(&format!("module{index}.wasm"), &bytes)?)
                .arg(name);
        }
        let merged = directory.join("merged.wasm");
        run(merge.arg("-o").arg(&merged), &binaryen.merge)?;
        let merged = std::fs::read(&merged)
            .map_err(|failure| error(format!("Reading merged module: {failure}")))?;
        let exported = rewrite_exports(&merged, |export| {
            export.strip_prefix(PREFIX).map(str::to_owned)
        })?;
        let input = write("exported.wasm", &exported)?;
        let optimized = directory.join("optimized.wasm");
        run(
            Command::new(&binaryen.opt)
                .args(FEATURES)
                .arg("-O3")
                .arg(&input)
                .arg("-o")
                .arg(&optimized),
            &binaryen.opt,
        )?;
        std::fs::read(&optimized)
            .map_err(|failure| error(format!("Reading optimized module: {failure}")))
    })();
    let _ = std::fs::remove_dir_all(&directory);
    result
}
