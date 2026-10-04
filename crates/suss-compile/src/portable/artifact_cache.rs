//! Bounded process-local source artifact reuse after effectful analysis.
//! No expansion, environment, cell, or initialized value is cached here.
use super::{Diagnostic, SourceOrigin, bootstrap, ir, resolve::Phase};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use suss_reader::forms::Form;

const MAX_ENTRIES: usize = 64;
const MAX_BYTES: usize = 32 * 1024 * 1024;
const TARGET: &str = "portable-wasm-gc-shared-abi";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
    pub bypasses: usize,
    pub entries: usize,
    pub bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn graph(source: &str) -> ir::Function {
        ir::lower(&super::super::analyze(source).unwrap()).unwrap()
    }
    fn emit(
        cache: &mut ArtifactCache,
        function: &ir::Function,
        source: &str,
        dependencies: Option<&[(String, String)]>,
    ) -> Vec<u8> {
        cache
            .emit(
                function,
                Phase::Runtime,
                &[],
                Some(&SourceOrigin::new(source, None)),
                dependencies,
            )
            .unwrap()
    }
    #[test]
    fn source_cache_distinguishes_source_dependency_phase_and_origin() {
        let mut cache = ArtifactCache::default();
        let function = graph("42");
        let deps = [("tools".into(), "version-one".into())];
        let original = emit(&mut cache, &function, "42", Some(&deps));
        assert_eq!(emit(&mut cache, &function, "42", Some(&deps)), original);
        assert_eq!(cache.stats().hits, 1);
        assert_eq!(
            emit(&mut cache, &function, "42 ; changed source", Some(&deps)),
            original
        );
        let changed = [("tools".into(), "version-two".into())];
        assert_eq!(emit(&mut cache, &function, "42", Some(&changed)), original);
        cache
            .emit(
                &function,
                Phase::Macro,
                &[],
                Some(&SourceOrigin::new("42", None)),
                Some(&deps),
            )
            .unwrap();
        cache
            .emit(
                &function,
                Phase::Runtime,
                &[],
                Some(&SourceOrigin::new("42", Some("other.sus".into()))),
                Some(&deps),
            )
            .unwrap();
        assert_eq!(cache.stats().misses, 5);
        assert_eq!(cache.stats().hits, 1);
    }
    #[test]
    fn source_cache_unknown_provenance_bypasses_reuse_and_reverifies_graph() {
        let mut cache = ArtifactCache::default();
        let mut function = graph("42");
        assert_eq!(
            emit(&mut cache, &function, "42", None),
            emit(&mut cache, &function, "42", None)
        );
        assert_eq!(cache.stats().bypasses, 2);
        assert_eq!(cache.stats().entries, 0);
        emit(&mut cache, &function, "42", Some(&[]));
        function.blocks[0].terminator = ir::Terminator::Return(ir::ValueId(usize::MAX));
        assert!(
            cache
                .emit(
                    &function,
                    Phase::Runtime,
                    &[],
                    Some(&SourceOrigin::new("42", None)),
                    Some(&[])
                )
                .is_err()
        );
        assert_eq!(cache.stats().hits, 0);
    }
    #[test]
    fn source_cache_preserves_nan_payload_bits_and_checks_integrity() {
        let mut cache = ArtifactCache::default();
        let mut function = graph("0");
        let set_number = |function: &mut ir::Function, bits| {
            let instruction = function
                .blocks
                .iter_mut()
                .flat_map(|block| &mut block.instructions)
                .find(|instruction| {
                    matches!(
                        instruction.operation,
                        ir::Operation::Literal(super::super::hir::Literal::Number(_))
                    )
                })
                .unwrap();
            instruction.operation =
                ir::Operation::Literal(super::super::hir::Literal::Number(f64::from_bits(bits)));
        };
        set_number(&mut function, 0x7ff8000000000001);
        let first = emit(&mut cache, &function, "same source", Some(&[]));
        set_number(&mut function, 0x7ff8000000000002);
        let second = emit(&mut cache, &function, "same source", Some(&[]));
        assert_ne!(first, second);
        assert_eq!(cache.stats().misses, 2);
        cache.entries.back_mut().unwrap().wasm.push(0);
        let error = cache
            .emit(
                &function,
                Phase::Runtime,
                &[],
                Some(&SourceOrigin::new("same source", None)),
                Some(&[]),
            )
            .unwrap_err();
        assert!(error.message.contains("integrity mismatch"));
        assert_eq!(cache.stats().entries, 1);
        assert_eq!(cache.stats().hits, 0);
    }
    #[test]
    fn source_cache_rejected_single_entry_releases_accounting_and_recovers() {
        let mut cache = ArtifactCache::default();
        let function = graph("42");
        emit(&mut cache, &function, "42", Some(&[]));
        cache.entries.front_mut().unwrap().wasm.push(0);
        assert!(
            cache
                .emit(
                    &function,
                    Phase::Runtime,
                    &[],
                    Some(&SourceOrigin::new("42", None)),
                    Some(&[])
                )
                .is_err()
        );
        assert_eq!(cache.stats().bytes, 0);
        assert_eq!(cache.stats().entries, 0);
        let original = emit(&mut cache, &function, "42", Some(&[]));
        let entry = cache.entries.front_mut().unwrap();
        entry.wasm = vec![0];
        entry.digest = bootstrap::sha256(&entry.wasm);
        assert!(
            cache
                .emit(
                    &function,
                    Phase::Runtime,
                    &[],
                    Some(&SourceOrigin::new("42", None)),
                    Some(&[])
                )
                .is_err()
        );
        assert_eq!(cache.stats().bytes, 0);
        assert_eq!(cache.stats().entries, 0);
        assert_eq!(emit(&mut cache, &function, "42", Some(&[])), original);
        assert_eq!(emit(&mut cache, &function, "42", Some(&[])), original);
        assert_eq!(cache.stats().hits, 1);
    }
    #[test]
    fn source_cache_eviction_keeps_memory_and_entry_counts_bounded() {
        let mut cache = ArtifactCache::default();
        let function = graph("42");
        for index in 0..MAX_ENTRIES + 1 {
            emit(
                &mut cache,
                &function,
                &format!("42 ; source {index}"),
                Some(&[]),
            );
        }
        assert_eq!(cache.stats().entries, MAX_ENTRIES);
        assert!(cache.stats().bytes <= MAX_BYTES);
        let misses = cache.stats().misses;
        emit(&mut cache, &function, "42 ; source 0", Some(&[]));
        assert_eq!(cache.stats().misses, misses + 1);
    }
}
struct Entry {
    key: [u8; 32],
    digest: String,
    wasm: Vec<u8>,
    retained_bytes: usize,
}
#[derive(Default)]
pub struct ArtifactCache {
    entries: VecDeque<Entry>,
    stats: CacheStats,
}
fn field(hash: &mut Sha256, value: &[u8]) {
    hash.update((value.len() as u64).to_le_bytes());
    hash.update(value);
}
// Debug is a process-local encoding, qualified by the complete compiler source
// identity. It is never a published serialization format. Preserve binary64
// payload bits separately: Debug intentionally collapses distinct NaNs.
fn numbers(hash: &mut Sha256, function: &ir::Function) {
    for block in &function.blocks {
        for instruction in &block.instructions {
            match &instruction.operation {
                ir::Operation::Literal(super::hir::Literal::Number(value)) => {
                    hash.update(value.to_bits().to_le_bytes());
                }
                ir::Operation::MakeClosure { body, .. } => numbers(hash, &body.function),
                ir::Operation::MakeGeneralClosure { body, .. } => {
                    for method in &body.methods {
                        numbers(hash, &method.function);
                    }
                }
                _ => {}
            }
        }
    }
}
impl ArtifactCache {
    pub fn stats(&self) -> CacheStats {
        self.stats
    }
    /// Unknown external host provenance disables reuse. Dependency records must
    /// describe immutable source snapshots actually selected by the host, rather
    /// than a new read of files belonging to already-loaded modules.
    pub fn emit(
        &mut self,
        function: &ir::Function,
        phase: Phase,
        forms: &[Form],
        origin: Option<&SourceOrigin>,
        dependencies: Option<&[(String, String)]>,
    ) -> Result<Vec<u8>, Diagnostic> {
        // A hit must never bypass verifier failures for a newly analyzed graph.
        ir::verify(function)?;
        let Some(dependencies) = dependencies else {
            self.stats.bypasses += 1;
            return super::compile_ir(function);
        };
        let mut hash = Sha256::new();
        field(&mut hash, b"suss-source-artifact-cache-v1");
        field(&mut hash, env!("SUSS_COMPILER_SOURCE_SHA256").as_bytes());
        field(
            &mut hash,
            format!("{:?}", crate::runtime_abi::Manifest::default()).as_bytes(),
        );
        field(&mut hash, TARGET.as_bytes());
        // The portable emitter currently has one target profile and no flags.
        // Future configurable options must become explicit key inputs here.
        field(&mut hash, b"default-profile;flags=[]");
        field(&mut hash, bootstrap::phase_name(phase).as_bytes());
        if let Some(origin) = origin {
            field(&mut hash, origin.text().as_bytes());
            field(&mut hash, format!("{:?}", origin.path()).as_bytes());
        } else {
            field(&mut hash, b"unknown-source-origin");
        }
        field(&mut hash, format!("{forms:?}").as_bytes());
        let mut dependencies = dependencies.to_vec();
        dependencies.sort();
        field(&mut hash, format!("{dependencies:?}").as_bytes());
        field(&mut hash, format!("{function:?}").as_bytes());
        numbers(&mut hash, function);
        let key: [u8; 32] = hash.finalize().into();
        if let Some(index) = self.entries.iter().position(|entry| entry.key == key) {
            let entry = self.entries.remove(index).unwrap();
            if bootstrap::sha256(&entry.wasm) != entry.digest {
                self.stats.bytes -= entry.retained_bytes;
                self.stats.entries = self.entries.len();
                return Err(Diagnostic {
                    span: function.span.clone(),
                    message: "Cached source artifact integrity mismatch".into(),
                });
            }
            if let Err(message) = crate::runtime_abi::verify_artifact(
                &entry.wasm,
                &crate::runtime_abi::Manifest::default(),
            )
            .and_then(|_| {
                super::artifact_identity::verify(&entry.wasm, Default::default()).map(|_| ())
            }) {
                self.stats.bytes -= entry.retained_bytes;
                self.stats.entries = self.entries.len();
                return Err(Diagnostic {
                    span: function.span.clone(),
                    message,
                });
            }
            let wasm = entry.wasm.clone();
            self.entries.push_back(entry);
            self.stats.hits += 1;
            return Ok(wasm);
        }
        self.stats.misses += 1;
        let wasm = super::compile_ir(function)?;
        if wasm.len() <= MAX_BYTES {
            while self.entries.len() >= MAX_ENTRIES || self.stats.bytes + wasm.len() > MAX_BYTES {
                let old = self.entries.pop_front().unwrap();
                self.stats.bytes -= old.retained_bytes;
            }
            self.stats.bytes += wasm.len();
            self.entries.push_back(Entry {
                key,
                digest: bootstrap::sha256(&wasm),
                retained_bytes: wasm.len(),
                wasm: wasm.clone(),
            });
            self.stats.entries = self.entries.len();
        }
        Ok(wasm)
    }
}
