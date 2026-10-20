//! Bounded process-local source artifact reuse after effectful analysis.
//! No expansion, environment, cell, or initialized value is cached here.
use super::{Diagnostic, SourceOrigin, bootstrap, ir, resolve::Phase};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use suss_reader::forms::Form;

const MAX_ENTRIES: usize = 64;
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_KEY_WORK: u64 = 64 * 1024;
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
    fn large_cache_keys_emit_fresh_without_changing_retained_entries() {
        let function = graph("42");
        let mut cache = ArtifactCache::default();
        let original = emit(&mut cache, &function, "42", Some(&[]));
        let before = cache.stats();
        let source = format!("42 ;{}", "a".repeat(128 * 1024));
        let origin = SourceOrigin::new(source.as_str(), None);
        for round in 1..=2 {
            let mut spent = 0u64;
            let actual = cache
                .emit_metered(
                    &function,
                    Phase::Runtime,
                    &[],
                    Some(&origin),
                    Some(&[]),
                    &mut |work| {
                        spent += work;
                        Ok(())
                    },
                )
                .unwrap();
            assert_eq!(actual, original);
            assert_eq!(
                cache.stats().bypasses,
                before.bypasses + round,
                "optional key work must not consume unbounded compilation budget"
            );
            assert_eq!(cache.stats().entries, before.entries);
            assert_eq!(cache.stats().bytes, before.bytes);
            assert_eq!(cache.stats().hits, before.hits);
            assert_eq!(cache.stats().misses, before.misses);
            assert!(
                spent < 128 * 1024,
                "key admission must stop formatting early: {spent}"
            );
        }
        let before = cache.stats();
        let mut invalid = function.clone();
        invalid.blocks[0].instructions[0].result = ir::ValueId(99);
        let error = cache
            .emit_metered(
                &invalid,
                Phase::Runtime,
                &[],
                Some(&origin),
                Some(&[]),
                &mut |_| Ok(()),
            )
            .unwrap_err();
        assert!(error.message.contains("definition is out of range"));
        assert_eq!(
            cache.stats(),
            before,
            "large keys must not bypass verification"
        );
        assert_eq!(emit(&mut cache, &function, "42", Some(&[])), original);
        assert_eq!(cache.stats().hits, before.hits + 1);
    }
    #[test]
    fn key_admission_preserves_real_errors_and_stops_partial_encoding() {
        for message in ["real caller fuel exhausted", "real caller interrupted"] {
            let mut calls = 0;
            let error = probe_key_work(
                3,
                &(0..0),
                &mut |work| {
                    calls += 1;
                    assert_eq!(work, 4);
                    Err(Diagnostic {
                        span: 7..9,
                        message: message.into(),
                    })
                },
                |meter| {
                    meter(4)?;
                    Ok(42)
                },
            )
            .unwrap_err();
            assert_eq!(error.span, 7..9);
            assert_eq!(error.message, message);
            assert_eq!(
                calls, 1,
                "real failure wins on the charge crossing admission"
            );
        }
        let mut charged = 0;
        let declined = probe_key_work(
            3,
            &(0..0),
            &mut |work| {
                charged += work;
                Ok(())
            },
            |meter| {
                let _ = meter(4);
                let _ = meter(1000);
                Ok(42)
            },
        )
        .unwrap();
        assert_eq!(
            declined, None,
            "partial keys cannot be published by a formatter ignoring its error"
        );
        assert_eq!(charged, 4, "bill the crossing charge, then stop encoding");
        let error = probe_key_work(3, &(0..0), &mut |_| Ok(()), |_meter| {
            Err::<u32, _>(Diagnostic {
                span: 3..4,
                message: "formatter failed".into(),
            })
        })
        .unwrap_err();
        assert_eq!(error.message, "formatter failed");
        assert_eq!(error.span, 3..4);
        assert_eq!(
            probe_key_work(3, &(0..0), &mut |_| Ok(()), |meter| {
                meter(3)?;
                Ok(42)
            })
            .unwrap(),
            Some(42)
        );
    }
    #[test]
    fn large_ir_key_declines_with_small_source_and_matches_fresh_emission() {
        let source = format!("(+ {})", vec!["1"; 2000].join(" "));
        let function = graph(&source);
        assert!(function.values.len() > 2000);
        let origin = SourceOrigin::new("(large-ir)", None);
        let declined = probe_key_work(MAX_KEY_WORK, &function.span, &mut |_| Ok(()), |meter| {
            debug_field(&mut Sha256::new(), &function, meter, &function.span)
        })
        .unwrap();
        assert_eq!(
            declined, None,
            "the actual IR Debug field exceeds key admission"
        );
        let expected = super::super::compile_ir(&function).unwrap();
        let mut cache = ArtifactCache::default();
        let actual = cache
            .emit_metered(
                &function,
                Phase::Runtime,
                &[],
                Some(&origin),
                Some(&[]),
                &mut |_| Ok(()),
            )
            .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(cache.stats().bypasses, 1);
        assert_eq!(cache.stats().entries, 0);
        let before = cache.stats();
        let mut invalid = function.clone();
        invalid.blocks[0].instructions[0].result = ir::ValueId(function.values.len() + 1);
        let error = cache.emit_metered(&invalid, Phase::Runtime, &[], Some(&origin), Some(&[]),
            &mut |_| Ok(())).unwrap_err();
        assert!(error.message.contains("definition is out of range"));
        assert_eq!(cache.stats(), before, "large invalid IR must fail before optional admission");

    }
    #[test]
    fn key_decline_preserves_session_fuel_interrupt_and_final_cancellation_poll() {
        use crate::portable_session::{Session, SessionError};
        let function = graph("42");
        let mut verification = 0;
        ir::verify_metered(&function, &mut |work| {
            verification += work;
            Ok(())
        })
        .unwrap();
        let source = format!("42 ;{}", "x".repeat(128 * 1024));
        let origin = SourceOrigin::new(source.as_str(), None);
        for mode in 0..3 {
            let mut session = Session::new().unwrap();
            session.set_operation_fuel(if mode == 0 {
                verification + MAX_KEY_WORK
            } else {
                1_000_000
            });
            let configured = session.options().fuel_per_operation;
            let handle = session.interrupt_handle();
            let request = session.operation_budget_scope().unwrap();
            let mut cache = ArtifactCache::default();
            let before = cache.stats();
            let mut charged = 0;
            let mut injected = false;
            let mut observed = false;
            let error = cache.emit_metered(&function, Phase::Runtime, &[], Some(&origin), Some(&[]),
                &mut |work| {
                    let crossing = !injected && charged + work > verification + MAX_KEY_WORK;
                    if crossing && mode == 1 { handle.interrupt(); injected = true; }
                    let result = session.charge_host_work(work);
                    if let Err(error) = result {
                        observed = matches!(&error, SessionError::Trap(error)
                            if error.downcast_ref::<wasmtime::Trap>() == Some(
                                &if mode == 0 { wasmtime::Trap::OutOfFuel } else { wasmtime::Trap::Interrupt }));
                        return Err(Diagnostic { span: 0..0, message: error.to_string() });
                    }
                    charged += work;
                    if crossing && mode == 2 { handle.interrupt(); injected = true; }
                    Ok(())
                }).unwrap_err();
            assert!(observed, "mode={mode}: {error}");
            assert_eq!(session.operation_fuel_remaining().unwrap(), 0);
            assert_eq!(
                cache.stats(),
                before,
                "a real error must not publish a bypass"
            );
            drop(request);
            let value = session.eval("42").unwrap();
            assert_eq!(session.options().fuel_per_operation, configured);
            session
                .inspect(&value, |mut store, value| {
                    let number = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    assert_eq!(number.field(&mut store, 0)?.unwrap_f64(), 42.0);
                    Ok(())
                })
                .unwrap();
        }
    }
    #[test]
    fn formatted_fragment_work_is_charged_before_optional_decline() {
        use crate::portable_session::{Session, SessionError};
        struct Fragment(String);
        impl std::fmt::Debug for Fragment {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
        let mut session = Session::new().unwrap();
        session.set_operation_fuel(100_000);
        let _request = session.operation_budget_scope().unwrap();
        let value = Fragment("x".repeat(128 * 1024));
        let mut observed = false;
        let error = probe_key_work(
            MAX_KEY_WORK,
            &(0..0),
            &mut |work| {
                session.charge_host_work(work).map_err(|error| {
                    observed = matches!(&error, SessionError::Trap(error)
                    if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel));
                    Diagnostic {
                        span: 0..0,
                        message: error.to_string(),
                    }
                })
            },
            |meter| debug_field(&mut Sha256::new(), &value, meter, &(0..0)),
        )
        .unwrap_err();
        assert!(
            observed,
            "a produced fragment cannot become unpaid skipped formatting: {error}"
        );
        assert_eq!(session.operation_fuel_remaining().unwrap(), 0);
    }
    #[test]
    fn source_cache_hit_and_miss_debit_the_caller_verification_budget() {
        let function = graph("42");
        let mut cache = ArtifactCache::default();
        let mut consumed = 0;
        let original = cache.emit_metered(&function, Phase::Runtime, &[], None, Some(&[]),
            &mut |work| { consumed += work; Ok(()) }).unwrap();
        assert!(consumed > 0);
        let mut hit_consumed = 0;
        assert_eq!(cache.emit_metered(&function, Phase::Runtime, &[], None, Some(&[]),
            &mut |work| { hit_consumed += work; Ok(()) }).unwrap(), original);
        assert!(hit_consumed > 0);
        assert!(consumed > hit_consumed, "miss also pays integer planning");
        let before = cache.stats();
        let mut remaining = hit_consumed - 1;
        let error = cache.emit_metered(&function, Phase::Runtime, &[], None, Some(&[]),
            &mut |work| {
                remaining = remaining.checked_sub(work).ok_or_else(|| Diagnostic {
                    span: 0..0, message: "test shared verification fuel exhausted".into(),
                })?;
                Ok(())
            }).unwrap_err();
        assert!(error.message.contains("fuel exhausted"));
        assert_eq!(cache.stats(), before);
        let mut retry_consumed = 0;
        assert_eq!(cache.emit_metered(&function, Phase::Runtime, &[], None, Some(&[]),
            &mut |work| { retry_consumed += work; Ok(()) }).unwrap(), original);
        assert_eq!(retry_consumed, hit_consumed);
        assert_eq!(cache.stats().hits, before.hits + 1);
    }
    #[test]
    fn integer_planning_cancellation_propagates_through_cache_routes_and_retries() {
        use super::super::hir::{Bitwise, Expression, Hir, Literal, Type};
        let number = |value| Hir {
            source: None, span: 0..1, metadata: vec![], ty: Type::Number,
            kind: Expression::Literal(Literal::Number(value)),
        };
        let inner = Hir {
            source: None, span: 0..1, metadata: vec![], ty: Type::Number,
            kind: Expression::Bitwise { operation: Bitwise::Int, arguments: vec![number(7.0)] },
        };
        let function = ir::lower(&Hir {
            source: None, span: 0..1, metadata: vec![], ty: Type::Number,
            kind: Expression::Bitwise { operation: Bitwise::Xor, arguments: vec![inner, number(3.0)] },
        }).unwrap();
        // Observe the emitter's actual planning suffix; do not assume a tiny
        // allowance or equality between cache admission and emission costs.
        let mut planning = Vec::new();
        super::super::emit::emit_verified_metered(&function,
            &mut |work| { planning.push(work); Ok(()) }).unwrap();
        assert!(!planning.is_empty());
        let large = SourceOrigin::new("x".repeat(128 * 1024), None);
        for mode in 0..3 {
            let dependencies = if mode == 0 { None } else { Some(&[][..]) };
            let origin = if mode == 1 { Some(&large) } else { None };
            let mut measured = ArtifactCache::default();
            let mut charges = Vec::new();
            let expected = measured.emit_metered(&function, Phase::Runtime, &[], origin, dependencies,
                &mut |work| { charges.push(work); Ok(()) }).unwrap();
            assert!(charges.ends_with(&planning));
            let start = charges.len() - planning.len();
            // Fail before, during and after scratch preparation. None may
            // publish an entry or turn the original error into a cache bypass.
            for boundary in [start, start + planning.len() / 2, charges.len() - 1] {
            for message in ["planner fuel exhausted", "planner interrupted"] {
                let mut cache = ArtifactCache::default();
                let mut call = 0;
                let error = cache.emit_metered(&function, Phase::Runtime, &[], origin, dependencies,
                    &mut |_| {
                        let fail = call == boundary;
                        call += 1;
                        if fail { Err(Diagnostic { span: 7..9, message: message.into() }) }
                        else { Ok(()) }
                    }).unwrap_err();
                assert_eq!(error.span, 7..9);
                assert_eq!(error.message, message);
                assert_eq!(cache.stats().entries, 0, "failed emission cannot publish");
                let mut retry_charges = Vec::new();
                assert_eq!(cache.emit_metered(&function, Phase::Runtime, &[], origin, dependencies,
                    &mut |work| { retry_charges.push(work); Ok(()) }).unwrap(), expected);
                assert_eq!(retry_charges, charges, "retry pays the complete route");
            }
            }
        }
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
// Stream framed Debug fragments rather than allocating an entire tree string.
fn debug_field(
    hash: &mut Sha256,
    value: &impl std::fmt::Debug,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
    span: &std::ops::Range<usize>,
) -> Result<(), Diagnostic> {
    struct Writer<'a, 'm> {
        hash: &'a mut Sha256,
        meter: &'a mut (dyn FnMut(u64) -> Result<(), Diagnostic> + 'm),
        error: Option<Diagnostic>,
        span: &'a std::ops::Range<usize>,
    }
    impl std::fmt::Write for Writer<'_, '_> {
        fn write_str(&mut self, text: &str) -> std::fmt::Result {
            // Formatting has already produced this fragment. Bill all of it
            // before optional admission can stop hashing a prefix of it.
            if let Err(mut error) = (self.meter)(text.len() as u64) {
                error.span = self.span.clone();
                self.error = Some(error);
                return Err(std::fmt::Error);
            }
            for chunk in text.as_bytes().chunks(4096) {
                field(self.hash, chunk);
            }
            Ok(())
        }
    }
    field(hash, b"debug-start");
    let mut writer = Writer { hash, meter, error: None, span };
    if std::fmt::write(&mut writer, format_args!("{value:?}")).is_err() {
        return Err(writer.error.unwrap_or_else(|| Diagnostic {
            span: span.clone(), message: "Unable to encode source artifact key".into(),
        }));
    }
    field(writer.hash, b"debug-end");
    Ok(())
}
// Debug is a process-local encoding, qualified by the complete compiler source
// identity. It is never a published serialization format. Preserve binary64
// payload bits separately: Debug intentionally collapses distinct NaNs.
fn numbers(hash: &mut Sha256, function: &ir::Function,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {

    for block in &function.blocks {
        meter(1)?;
        for instruction in &block.instructions {
            meter(1)?;
            match &instruction.operation {
                ir::Operation::Literal(super::hir::Literal::Number(value)) => {
                    hash.update(value.to_bits().to_le_bytes());
                }
                ir::Operation::MakeFuture { body, .. } => numbers(hash, &body.continuation.function, meter)?,
                ir::Operation::MakeClosure { body, .. } => numbers(hash, &body.function, meter)?,
                ir::Operation::MakeGeneralClosure { body, .. } => {
                    for method in &body.methods {
                        numbers(hash, &method.function, meter)?;
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
    })
}
// Optional key work has its own admission decision, never its own fuel.
// The caller is debited first, including the operation crossing this allowance.
fn probe_key_work<T>(
    allowance: u64,
    span: &std::ops::Range<usize>,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
    body: impl FnOnce(&mut dyn FnMut(u64) -> Result<(), Diagnostic>) -> Result<T, Diagnostic>,
) -> Result<Option<T>, Diagnostic> {
    enum Failure {
        Declined,
        Caller(Diagnostic),
    }
    fn feedback(failure: &Failure, span: &std::ops::Range<usize>) -> Diagnostic {
        match failure {
            Failure::Caller(error) => Diagnostic {
                span: error.span.clone(),
                message: error.message.clone(),
            },
            Failure::Declined => Diagnostic {
                span: span.clone(),
                message: "Optional artifact key admission declined".into(),
            },
        }
    }
    let mut left = allowance;
    let mut failure = None;
    let result = {
        let mut bounded = |work| {
            // Preserve the first failure even if a formatter attempts another
            // write after returning fmt::Error. It cannot resume key work.
            if let Some(failure) = &failure {
                return Err(feedback(failure, span));
            }
            if let Err(error) = meter(work) {
                failure = Some(Failure::Caller(error));
                return Err(feedback(failure.as_ref().unwrap(), span));
            }
            match left.checked_sub(work) {
                Some(remaining) => {
                    left = remaining;
                    Ok(())
                }
                None => {
                    failure = Some(Failure::Declined);
                    Err(feedback(failure.as_ref().unwrap(), span))
                }
            }
        };
        body(&mut bounded)
    };
    match failure {
        Some(Failure::Caller(error)) => Err(error),
        Some(Failure::Declined) => Ok(None),
        None => result.map(Some),
    }
}
fn artifact_key(
    function: &ir::Function,
    phase: Phase,
    forms: &[Form],
    origin: Option<&SourceOrigin>,
    dependencies: &[(String, String)],
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<[u8; 32], Diagnostic> {
    let mut hash = Sha256::new();
    field(&mut hash, b"suss-source-artifact-cache-v2");
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
        hash.update((origin.text().len() as u64).to_le_bytes());
        for chunk in origin.text().as_bytes().chunks(4096) {
            meter(chunk.len() as u64)?;
            hash.update(chunk);
        }
        debug_field(&mut hash, &origin.path(), meter, &function.span)?;
    } else {
        field(&mut hash, b"unknown-source-origin");
    }
    debug_field(&mut hash, &forms, meter, &function.span)?;
    meter(dependencies.len() as u64)?;
    let mut dependencies: Vec<_> = dependencies.iter().collect();
    // Fallible insertion sort permits cancellation at each comparison and
    // preserves the same lexicographic tuple ordering without text clones.
    for index in 1..dependencies.len() {
        let mut current = index;
        while current > 0 {
            let left = dependencies[current - 1];
            let right = dependencies[current];
            for text in [&left.0, &left.1, &right.0, &right.1] {
                meter(text.len() as u64)?;
            }
            if left <= right {
                break;
            }
            dependencies.swap(current - 1, current);
            current -= 1;
        }
    }
    debug_field(&mut hash, &dependencies, meter, &function.span)?;
    debug_field(&mut hash, function, meter, &function.span)?;
    numbers(&mut hash, function, meter)?;
    Ok(hash.finalize().into())
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
        let mut remaining = super::DEFAULT_OPERATION_FUEL;
        self.emit_metered(function, phase, forms, origin, dependencies, &mut |work| {
            remaining = remaining.checked_sub(work).ok_or_else(|| Diagnostic {
                span: function.span.clone(),
                message: "Artifact preparation fuel exhausted".into(),
            })?;
            Ok(())
        })
    }
    pub(crate) fn emit_metered(
        &mut self,
        function: &ir::Function,
        phase: Phase,
        forms: &[Form],
        origin: Option<&SourceOrigin>,
        dependencies: Option<&[(String, String)]>,
        meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
    ) -> Result<Vec<u8>, Diagnostic> {
        // A hit must never bypass verifier failures for a newly analyzed graph.
        ir::verify_metered(function, meter)?;
        let Some(dependencies) = dependencies else {
            self.stats.bypasses += 1;
            return super::compile_verified_ir(function, meter);
        };
        let key = probe_key_work(MAX_KEY_WORK, &function.span, meter, |meter| {
            artifact_key(function, phase, forms, origin, dependencies, meter)
        })?;
        let Some(key) = key else {
            // A decline only omits optional reuse. Verification already ran;
            // real fuel/cancellation errors cannot become a successful bypass.
            meter(0)?;
            let wasm = super::compile_verified_ir(function, meter)?;
            self.stats.bypasses += 1;
            return Ok(wasm);
        };
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
        let wasm = super::compile_verified_ir(function, meter)?;
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
