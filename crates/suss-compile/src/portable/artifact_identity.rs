//! Portable artifact provenance and compatibility, independent of type indices.
//! None records unknown provenance; it never stands for a known empty graph.
use super::{SourceOrigin, bootstrap, resolve::Phase};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use wasm_encoder::{Encode, Section};

const SECTION: &str = "suss.source-artifact";
const FORMAT: u32 = 1;
const TARGET: &str = "portable-wasm-gc-shared-abi";
const MAX_MANIFEST: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format_version: u32,
    pub compiler_source_sha256: String,
    pub runtime_abi: u32,
    pub compiler: String,
    pub wasm_tools: String,
    pub target: String,
    pub profile: String,
    pub flags: Vec<String>,
    pub phase: Option<String>,
    pub source_sha256: Option<String>,
    pub source_path: Option<String>,
    pub macro_dependencies: Option<Vec<(String, String)>>,
    /// Digest of the complete module with this one identity section removed.
    pub wasm_sha256: String,
}
#[derive(Default)]
pub struct Expected<'a> {
    pub phase: Option<Phase>,
    pub source: Option<&'a str>,
    pub macro_dependencies: Option<&'a [(String, String)]>,
}
pub fn compiler_source_sha256() -> &'static str {
    env!("SUSS_COMPILER_SOURCE_SHA256")
}
fn current() -> Manifest {
    let abi = crate::runtime_abi::Manifest::default();
    Manifest {
        format_version: FORMAT,
        compiler_source_sha256: compiler_source_sha256().into(),
        runtime_abi: abi.runtime_abi,
        compiler: abi.compiler,
        wasm_tools: abi.wasm_tools,
        target: TARGET.into(),
        profile: "default".into(),
        flags: vec![],
        phase: None,
        source_sha256: None,
        source_path: None,
        macro_dependencies: None,
        wasm_sha256: String::new(),
    }
}
fn split(bytes: &[u8]) -> Result<(Vec<u8>, Option<Manifest>), String> {
    let mut body = Vec::new();
    let mut manifest = None;
    let mut previous_end = 0;
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|error| error.to_string())?;
        match &payload {
            wasmparser::Payload::Version {
                encoding: wasmparser::Encoding::Module,
                range,
                ..
            } => {
                let start = usize::try_from(range.start)
                    .map_err(|_| "Artifact offset exceeds address space")?;
                let end = usize::try_from(range.end)
                    .map_err(|_| "Artifact offset exceeds address space")?;
                body.extend_from_slice(
                    bytes
                        .get(start..end)
                        .ok_or("Invalid artifact header range")?,
                );
                previous_end = end;
            }
            wasmparser::Payload::Version { .. } => {
                return Err("Source artifact identity requires a core module".into());
            }
            _ => {}
        }
        if let Some((_, range)) = payload.as_section() {
            let end =
                usize::try_from(range.end).map_err(|_| "Artifact offset exceeds address space")?;
            let identity = matches!(&payload, wasmparser::Payload::CustomSection(section) if section.name() == SECTION);
            if identity {
                if manifest.is_some() {
                    return Err("Duplicate source artifact identity".into());
                }
                let wasmparser::Payload::CustomSection(section) = payload else {
                    unreachable!()
                };
                if section.data().len() > MAX_MANIFEST {
                    return Err("Source artifact identity exceeds bounds".into());
                }
                manifest = Some(
                    serde_json::from_slice(section.data())
                        .map_err(|error| format!("Malformed source artifact identity: {error}"))?,
                );
            } else {
                body.extend_from_slice(
                    bytes
                        .get(previous_end..end)
                        .ok_or("Invalid artifact section range")?,
                );
            }
            previous_end = end;
        }
    }
    if previous_end != bytes.len() {
        return Err("Incomplete source artifact module".into());
    }
    Ok((body, manifest))
}
fn seal(bytes: &[u8], mut manifest: Manifest) -> Result<Vec<u8>, String> {
    let (mut body, _) = split(bytes)?;
    manifest.wasm_sha256 = bootstrap::sha256(&body);
    let data = serde_json::to_vec(&manifest).map_err(|error| error.to_string())?;
    if data.len() > MAX_MANIFEST {
        return Err("Source artifact identity exceeds bounds".into());
    }
    let section = wasm_encoder::CustomSection {
        name: Cow::Borrowed(SECTION),
        data: Cow::Owned(data),
    };
    body.push(section.id());
    section.encode(&mut body);
    Ok(body)
}
/// Low-level IR emission has known compiler/ABI/target identity but no claimed
/// source, phase or macro graph. Source preparation supplies those independently.
pub fn annotate_ir(bytes: &[u8]) -> Result<Vec<u8>, String> {
    seal(bytes, current())
}
pub fn annotate_source(
    bytes: &[u8],
    phase: Phase,
    origin: Option<&SourceOrigin>,
    dependencies: Option<&[(String, String)]>,
) -> Result<Vec<u8>, String> {
    let mut manifest = current();
    manifest.phase = Some(bootstrap::phase_name(phase).into());
    manifest.source_sha256 = origin.map(|origin| bootstrap::sha256(origin.text().as_bytes()));
    manifest.source_path = origin
        .and_then(|origin| origin.path())
        .and_then(|path| path.to_str())
        .map(str::to_owned);
    manifest.macro_dependencies = dependencies.map(|dependencies| {
        let mut dependencies = dependencies.to_vec();
        dependencies.sort();
        dependencies
    });
    seal(bytes, manifest)
}
pub fn read(bytes: &[u8]) -> Result<Manifest, String> {
    let (_, manifest) = split(bytes)?;
    manifest.ok_or_else(|| "Missing source artifact identity".into())
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
/// Verify before allocating cells, publishing catalogs or executing initializers.
/// Hosts must additionally perform ordinary Wasm validation and linking.
pub fn verify(bytes: &[u8], expected: Expected<'_>) -> Result<Manifest, String> {
    let (body, manifest) = split(bytes)?;
    let found = manifest.ok_or_else(|| "Missing source artifact identity".to_owned())?;
    let current = current();
    if found.format_version != current.format_version {
        return Err("Source artifact format identity mismatch".into());
    }
    if found.compiler_source_sha256 != current.compiler_source_sha256 {
        return Err("Source artifact compiler build identity mismatch".into());
    }
    if (found.runtime_abi, &found.compiler, &found.wasm_tools)
        != (current.runtime_abi, &current.compiler, &current.wasm_tools)
    {
        return Err("Source artifact runtime ABI identity mismatch".into());
    }
    if (&found.target, &found.profile, &found.flags)
        != (&current.target, &current.profile, &current.flags)
    {
        return Err("Source artifact target profile/flags identity mismatch".into());
    }
    if !digest(&found.wasm_sha256) || found.wasm_sha256 != bootstrap::sha256(&body) {
        return Err("Source artifact Wasm integrity mismatch".into());
    }
    if found
        .phase
        .as_deref()
        .is_some_and(|phase| !matches!(phase, "runtime" | "macro"))
    {
        return Err("Invalid source artifact phase".into());
    }
    if let Some(phase) = expected.phase {
        if found.phase.as_deref() != Some(bootstrap::phase_name(phase)) {
            return Err("Source artifact phase identity mismatch".into());
        }
    }
    if found
        .source_sha256
        .as_deref()
        .is_some_and(|source| !digest(source))
    {
        return Err("Invalid source artifact source digest".into());
    }
    if let Some(source) = expected.source {
        if found.source_sha256 != Some(bootstrap::sha256(source.as_bytes())) {
            return Err("Source artifact source identity mismatch".into());
        }
    }
    if let Some(dependencies) = &found.macro_dependencies {
        if dependencies
            .iter()
            .any(|(identity, version)| identity.is_empty() || !digest(version))
            || dependencies.windows(2).any(|pair| pair[0].0 >= pair[1].0)
        {
            return Err("Invalid source artifact dependency graph".into());
        }
    }
    if let Some(expected) = expected.macro_dependencies {
        let mut expected = expected.to_vec();
        expected.sort();
        if found.macro_dependencies != Some(expected) {
            return Err("Source artifact macro dependency identity mismatch".into());
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> Vec<u8> {
        super::super::compile("42").unwrap()
    }
    #[test]
    fn artifact_identity_preserves_exact_source_and_phase_and_unknown_provenance() {
        let wasm = source();
        let found = verify(
            &wasm,
            Expected {
                phase: Some(Phase::Runtime),
                source: Some("42"),
                macro_dependencies: Some(&[]),
            },
        )
        .unwrap();
        assert_eq!(found.compiler_source_sha256, compiler_source_sha256());
        assert_eq!(found.phase.as_deref(), Some("runtime"));
        assert_eq!(found.macro_dependencies, Some(vec![]));
        let function = super::super::ir::lower(&super::super::analyze("42").unwrap()).unwrap();
        let bare = super::super::compile_ir(&function).unwrap();
        let found = verify(&bare, Expected::default()).unwrap();
        assert_eq!(found.phase, None);
        assert_eq!(found.source_sha256, None);
        assert_eq!(found.macro_dependencies, None);
        assert!(
            verify(
                &bare,
                Expected {
                    phase: Some(Phase::Runtime),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            verify(
                &bare,
                Expected {
                    macro_dependencies: Some(&[]),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    #[test]
    fn artifact_identity_checks_compiler_abi_profile_flags_and_selected_dependency_versions() {
        let wasm = source();
        let mut stale = read(&wasm).unwrap();
        stale.compiler_source_sha256 = "0".repeat(64);
        assert!(
            verify(&seal(&wasm, stale).unwrap(), Expected::default())
                .unwrap_err()
                .contains("compiler build identity")
        );
        for field in ["abi", "profile", "flags"] {
            let mut stale = read(&wasm).unwrap();
            match field {
                "abi" => stale.runtime_abi += 1,
                "profile" => stale.profile = "other".into(),
                "flags" => stale.flags.push("unrecognized".into()),
                _ => unreachable!(),
            }
            assert!(verify(&seal(&wasm, stale).unwrap(), Expected::default()).is_err());
        }
        let versions = [("module:tools".into(), bootstrap::sha256(b"old tools"))];
        let newer = [("module:tools".into(), bootstrap::sha256(b"new tools"))];
        let wasm = annotate_source(
            &wasm,
            Phase::Runtime,
            Some(&SourceOrigin::new("42", Some("input.sus".into()))),
            Some(&versions),
        )
        .unwrap();
        assert_eq!(
            verify(
                &wasm,
                Expected {
                    macro_dependencies: Some(&versions),
                    ..Default::default()
                }
            )
            .unwrap()
            .source_path
            .as_deref(),
            Some("input.sus")
        );
        assert!(
            verify(
                &wasm,
                Expected {
                    macro_dependencies: Some(&newer),
                    ..Default::default()
                }
            )
            .unwrap_err()
            .contains("dependency identity")
        );
        assert!(
            verify(
                &wasm,
                Expected {
                    phase: Some(Phase::Macro),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert!(
            verify(
                &wasm,
                Expected {
                    source: Some("43"),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    #[test]
    fn artifact_identity_preserves_custom_sections_and_code_across_reannotation() {
        let function = super::super::ir::lower(&super::super::analyze("42").unwrap()).unwrap();
        let emitted = super::super::emit::emit(&function).unwrap();
        let mut body = emitted[..8].to_vec();
        let before = wasm_encoder::CustomSection {
            name: Cow::Borrowed("review.before"),
            data: Cow::Borrowed(b"\x00\xffunchanged"),
        };
        body.push(before.id());
        before.encode(&mut body);
        body.extend_from_slice(&emitted[8..]);
        let first = annotate_ir(&body).unwrap();
        let after = wasm_encoder::CustomSection {
            name: Cow::Borrowed("review.after"),
            data: Cow::Borrowed(b"\xff\x00unchanged"),
        };
        let mut with_after = first;
        with_after.push(after.id());
        after.encode(&mut with_after);
        body.push(after.id());
        after.encode(&mut body);
        // A custom section appended after sealing changes the module body too.
        assert!(
            verify(&with_after, Expected::default())
                .unwrap_err()
                .contains("integrity")
        );
        let origin = SourceOrigin::new("42", None);
        let annotated =
            annotate_source(&with_after, Phase::Runtime, Some(&origin), Some(&[])).unwrap();
        assert_eq!(split(&annotated).unwrap().0, body);
        let repeated =
            annotate_source(&annotated, Phase::Runtime, Some(&origin), Some(&[])).unwrap();
        assert_eq!(annotated, repeated);
        verify(
            &repeated,
            Expected {
                phase: Some(Phase::Runtime),
                source: Some("42"),
                macro_dependencies: Some(&[]),
            },
        )
        .unwrap();
        wasmparser::Validator::new()
            .validate_all(&repeated)
            .unwrap();
    }
    #[test]
    fn artifact_identity_rejects_missing_duplicate_corrupt_and_malformed_records() {
        let wasm = source();
        let (body, _) = split(&wasm).unwrap();
        assert!(
            verify(&body, Expected::default())
                .unwrap_err()
                .contains("Missing")
        );
        let mut duplicate = wasm.clone();
        let section = wasm_encoder::CustomSection {
            name: Cow::Borrowed(SECTION),
            data: Cow::Owned(serde_json::to_vec(&read(&wasm).unwrap()).unwrap()),
        };
        duplicate.push(section.id());
        section.encode(&mut duplicate);
        assert!(
            verify(&duplicate, Expected::default())
                .unwrap_err()
                .contains("Duplicate")
        );
        let mut corrupt = wasm;
        let constant = 42.0f64.to_bits().to_le_bytes();
        let index = corrupt
            .windows(8)
            .position(|bytes| bytes == constant)
            .unwrap();
        corrupt[index..index + 8].copy_from_slice(&43.0f64.to_bits().to_le_bytes());
        assert!(
            verify(&corrupt, Expected::default())
                .unwrap_err()
                .contains("integrity")
        );
        let mut malformed = body;
        let section = wasm_encoder::CustomSection {
            name: Cow::Borrowed(SECTION),
            data: Cow::Borrowed(b"{}"),
        };
        malformed.push(section.id());
        section.encode(&mut malformed);
        assert!(
            verify(&malformed, Expected::default())
                .unwrap_err()
                .contains("Malformed")
        );
    }
}
