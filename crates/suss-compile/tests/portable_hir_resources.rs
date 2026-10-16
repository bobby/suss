//! Deep analyzed trees must be safe to release on success and failed requests.
use suss_compile::portable::hir::{Expression, Hir, Literal, Type};

fn deep_tree() -> Hir {
    let mut tree = Hir {
        source: None,
        span: 0..1,
        metadata: vec![],
        ty: Type::Number,
        kind: Expression::Literal(Literal::Number(42.0)),
    };
    for _ in 0..10_000 {
        tree = Hir {
            source: None,
            span: 0..1,
            metadata: vec![],
            ty: Type::Number,
            kind: Expression::Do(vec![tree]),
        };
    }
    tree
}

#[test]
fn deep_hir_clone_and_success_cleanup_use_bounded_native_stack() {
    let tree = deep_tree();
    let copied = tree.clone();
    drop(tree);
    drop(copied);
}

#[test]
fn deep_partial_hir_cleanup_survives_error_and_unwind() {
    let result = (|| -> Result<(), &'static str> {
        let _partial = deep_tree();
        Err("request rejected")
    })();
    assert_eq!(result, Err("request rejected"));
    let unwind = std::panic::catch_unwind(|| {
        let _partial = deep_tree();
        panic!("test request unwind");
    });
    assert!(unwind.is_err());
}

fn nested_ir(depth: usize) -> suss_compile::portable::ir::Function {
    use suss_compile::portable::ir::{Block, ClosureBody, Function, Instruction, Operation, Terminator, Value, ValueId};
    let mut function = Function {
        values: vec![Value { ty: Type::Nil, span: 12..15 }],
        blocks: vec![Block {
            parameters: vec![],
            instructions: vec![Instruction { result: ValueId(0), operation: Operation::Literal(Literal::Nil), span: 12..15 }],
            terminator: Terminator::Return(ValueId(0)),
        }],
        span: 12..15,
    };
    for _ in 0..depth {
        function = Function {
            values: vec![Value { ty: Type::Closure(0), span: 12..15 }],
            blocks: vec![Block {
                parameters: vec![],
                instructions: vec![Instruction {
                    result: ValueId(0),
                    operation: Operation::MakeClosure {
                        body: Box::new(ClosureBody {
                            display_name: None, variadic: false, capture_types: vec![], arity: 0, function,
                        }),
                        captures: vec![],
                    },
                    span: 12..15,
                }],
                terminator: Terminator::Return(ValueId(0)),
            }],
            span: 12..15,
        };
    }
    function
}

#[test]
fn ir_closure_depth_is_governed_by_work_and_retains_nested_validation() {
    use suss_compile::portable::{Diagnostic, ir};
    let mut graph = nested_ir(128);
    let mut charged = 0;
    ir::verify_metered(&graph, &mut |work| { charged += work; Ok(()) }).unwrap();
    assert!(charged > 128);
    let mut remaining = charged - 1;
    let error = ir::verify_metered(&graph, &mut |work| {
        remaining = remaining.checked_sub(work).ok_or_else(|| Diagnostic {
            span: 0..0, message: "test verifier fuel exhausted".into(),
        })?;
        Ok(())
    }).unwrap_err();
    assert!(error.message.contains("fuel exhausted"));
    assert_eq!(error.span, 12..15);
    ir::verify(&graph).unwrap();
    let mut child = &mut graph;
    for _ in 0..128 {
        let ir::Operation::MakeClosure { body, .. } = &mut child.blocks[0].instructions[0].operation else { panic!("closure fixture") };
        child = &mut body.function;
    }
    child.blocks[0].terminator = ir::Terminator::Return(ir::ValueId(1));
    let error = ir::verify(&graph).unwrap_err();
    assert!(error.message.contains("no definition"), "{error:?}");
}

#[test]
fn ir_verification_polls_cancellation_and_recovers_without_publishing() {
    use suss_compile::portable::{Diagnostic, ir};
    let graph = nested_ir(128);
    let mut calls = 0;
    let error = ir::verify_metered(&graph, &mut |_| {
        calls += 1;
        if calls == 50 {
            Err(Diagnostic { span: 0..0, message: "test verifier cancelled".into() })
        } else { Ok(()) }
    }).unwrap_err();
    assert_eq!(error.message, "test verifier cancelled");
    assert_eq!(error.span, 12..15);
    ir::verify(&graph).unwrap();
}

#[test]
fn deep_ir_cache_key_and_emission_remain_stack_safe() {
    use suss_compile::portable::{artifact_cache::ArtifactCache, resolve::Phase};
    let graph = nested_ir(128);
    let mut cache = ArtifactCache::default();
    let first = cache.emit(&graph, Phase::Runtime, &[], None, Some(&[])).unwrap();
    suss_compile::runtime_abi::verify_artifact(&first, &Default::default()).unwrap();
    let second = cache.emit(&graph, Phase::Runtime, &[], None, Some(&[])).unwrap();
    assert_eq!(first, second);
    assert_eq!(cache.stats().hits, 1);
}

#[test]
fn deep_ir_cleanup_survives_success_rejection_and_unwind() {
    use suss_compile::portable::{Diagnostic, ir};
    let graph = nested_ir(10_000);
    let copy = graph.clone();
    let error = ir::verify_metered(&graph, &mut |_| Err(Diagnostic {
        span: 0..0, message: "test rejected".into(),
    })).unwrap_err();
    assert_eq!(error.message, "test rejected");
    drop(graph);
    drop(copy);
    assert!(std::panic::catch_unwind(|| {
        let _partial = nested_ir(10_000);
        panic!("test IR cleanup unwind");
    }).is_err());
}

#[test]
fn verifier_dominance_crosses_word_boundaries_and_rejects_bypassed_definitions() {
    use suss_compile::portable::ir::{self, Block, Function, Instruction, Operation, Terminator, Value, ValueId};
    let mut blocks = Vec::new();
    for index in 0..130 {
        blocks.push(Block {
            parameters: vec![],
            instructions: if index < 2 { vec![Instruction {
                result: ValueId(index), operation: Operation::Literal(Literal::Nil), span: 0..1,
            }] } else { vec![] },
            terminator: if index == 129 { Terminator::Return(ValueId(1)) }
                else { Terminator::Jump { target: index + 1, arguments: vec![] } },
        });
    }
    let mut graph = Function {
        values: vec![Value { ty: Type::Nil, span: 0..1 }; 2], blocks, span: 0..1,
    };
    ir::verify(&graph).unwrap();
    graph.blocks[0].terminator = Terminator::Branch {
        condition: ValueId(0), consequent: 1, alternative: 65,
    };
    assert!(ir::verify(&graph).unwrap_err().message.contains("does not dominate"));
}
