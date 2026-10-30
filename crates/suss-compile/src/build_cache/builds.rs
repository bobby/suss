//! Whole-build reuse for `suss compile` (#272). A manifest, keyed by the
//! compiler identity and the request's semantic arguments, records every
//! input the build observed and the digest of each output. A later request
//! with the same key reuses the outputs only while every observation holds.
//!
//! Macro expansion runs in an isolated Store with no host imports, so its
//! effects cannot outlive the build; a completed build's outputs are a
//! function of its recorded inputs (outputs are byte-deterministic). Only
//! successful builds are recorded; a failed build is always recomputed, so
//! its diagnostics are never replayed from the cache.
use super::{Key, Layer, current, inputs, trace};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    inputs: inputs::Recorded,
    /// Output name to the SHA-256 (hex) of its contents.
    outputs: BTreeMap<String, String>,
}

/// The compiler's identity: its source digest, the embedded core and
/// bootstrap digest, and the runtime ABI.
fn compiler_identity() -> Vec<String> {
    vec![
        crate::portable::artifact_identity::compiler_source_sha256().into(),
        crate::portable::bootstrap::embedded_sha256().into(),
        format!("{:?}", crate::runtime_abi::Manifest::default()),
    ]
}
/// The manifest key: the compiler identity, the portable target and profile,
/// and the request's semantic fields in order.
fn key(request: &[(&str, String)]) -> [u8; 32] {
    key_for(&compiler_identity(), request)
}
fn key_for(identity: &[String], request: &[(&str, String)]) -> [u8; 32] {
    let mut key = Key::new(Layer::BuildManifest);
    for part in identity {
        key.field(part.as_bytes());
    }
    key.field(b"portable-wasm-gc-shared-abi;default-profile");
    for (name, value) in request {
        key.field(name.as_bytes()).field(value.as_bytes());
    }
    key.finish()
}
fn output_key(digest: &str) -> [u8; 32] {
    let mut key = Key::new(Layer::BuildOutput);
    key.field(digest.as_bytes());
    key.finish()
}

/// Outputs of `build`, reused when a recorded build with the same request
/// observed identical inputs. Returns whether they came from the cache.
pub fn reuse_or_build<E>(
    request: &[(&str, String)],
    build: impl FnOnce() -> Result<BTreeMap<String, Vec<u8>>, E>,
) -> Result<(BTreeMap<String, Vec<u8>>, bool), E> {
    let Some(directory) = current() else {
        return build().map(|outputs| (outputs, false));
    };
    let key = key(request);
    if let Some(outputs) = reuse(directory, &key) {
        return Ok((outputs, true));
    }
    let (result, recorded) = inputs::record(build);
    let outputs = result?;
    let mut digests = BTreeMap::new();
    for (name, bytes) in &outputs {
        let digest = format!("{:x}", Sha256::digest(bytes));
        directory.publish(Layer::BuildOutput, &output_key(&digest), bytes);
        digests.insert(name.clone(), digest);
    }
    // The manifest is the checkpoint: it is published last and refers only
    // to outputs already stored, so a reader never sees a partial build.
    let manifest = Manifest { inputs: recorded, outputs: digests };
    if let Ok(bytes) = serde_json::to_vec(&manifest) {
        directory.publish(Layer::BuildManifest, &key, &bytes);
    }
    Ok((outputs, false))
}

fn reuse(directory: &super::BuildDir, key: &[u8; 32]) -> Option<BTreeMap<String, Vec<u8>>> {
    let bytes = directory.read(Layer::BuildManifest, key)?;
    let Ok(manifest) = serde_json::from_slice::<Manifest>(&bytes) else {
        trace(Layer::BuildManifest, "reject", key, "malformed manifest");
        return None;
    };
    if let Some(change) = manifest.inputs.changed() {
        trace(Layer::BuildManifest, "stale", key, &change);
        return None;
    }
    let mut outputs = BTreeMap::new();
    for (name, digest) in manifest.outputs {
        let output = output_key(&digest);
        let bytes = directory.read(Layer::BuildOutput, &output)?;
        if format!("{:x}", Sha256::digest(&bytes)) != digest {
            trace(Layer::BuildOutput, "reject", &output, "output digest mismatch");
            return None;
        }
        trace(Layer::BuildOutput, "hit", &output, "");
        outputs.insert(name, bytes);
    }
    trace(Layer::BuildManifest, "hit", key, "");
    Some(outputs)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_compiler_identity_and_request_field_changes_the_key() {
        let identity = compiler_identity();
        assert_eq!(identity.len(), 3);
        let request = [("mode", "file".to_owned()), ("compile-fuel", "None".to_owned())];
        let base = key_for(&identity, &request);
        assert_eq!(base, key(&request));
        // Compiler source, embedded core/bootstrap and runtime ABI each count.
        for index in 0..identity.len() {
            let mut changed = identity.clone();
            changed[index].push('x');
            assert_ne!(key_for(&changed, &request), base, "identity part {index}");
        }
        for index in 0..request.len() {
            let mut changed = request.clone();
            changed[index].1.push('x');
            assert_ne!(key_for(&identity, &changed), base, "request field {index}");
        }
        // Fields cannot run together.
        assert_ne!(
            key_for(&identity, &[("a", "bc".into())]),
            key_for(&identity, &[("ab", "c".into())])
        );
    }
}
