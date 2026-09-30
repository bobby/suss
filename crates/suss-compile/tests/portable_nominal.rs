//! Public nominal HIR/IR guards and phase/name isolation. Runtime/source behavior
//! is independently executed by runtime_abi and native persistent-session tests.
use suss_compile::{
    portable::{
        self,
        hir::{Expression, Hir, Literal, NativeKind, Nominal, Type},
        ir,
        resolve::{Environment, Phase},
    },
    runtime_abi,
};

#[test]
fn nominal_public_hir_and_ir_reject_bad_shapes_before_emission() {
    let value = Hir {
        span: 2..5,
        metadata: vec![],
        ty: Type::Nil,
        kind: Expression::Literal(Literal::Nil),
    };
    for operation in [
        Nominal::Class,
        Nominal::Construct,
        Nominal::Instance,
        Nominal::Field(usize::MAX),
        Nominal::Key(usize::MAX),
        Nominal::Dispatcher,
        Nominal::LiveDispatcher,
        Nominal::NativeMarker(NativeKind::Nil),
        Nominal::NativeSet(NativeKind::Default),
        Nominal::Set,
        Nominal::Marker,
        Nominal::Satisfies,
    ] {
        let invalid = Hir {
            span: 1..9,
            metadata: vec![],
            ty: operation.result(),
            kind: Expression::Nominal {
                operation,
                arguments: vec![],
            },
        };
        assert!(ir::lower(&invalid).is_err(), "{operation:?}");
    }
    let invalid_descriptor = Hir {
        span: 1..9,
        metadata: vec![],
        ty: Type::Value,
        kind: Expression::Nominal {
            operation: Nominal::Descriptor,
            arguments: vec![value.clone()],
        },
    };
    assert!(ir::lower(&invalid_descriptor).is_err());
    let valid = Hir {
        span: 1..9,
        metadata: vec![],
        ty: Type::Value,
        kind: Expression::Nominal {
            operation: Nominal::Array,
            arguments: vec![value],
        },
    };
    let graph = ir::lower(&valid).unwrap();
    ir::verify(&graph).unwrap();
    for operation in [
        Nominal::Descriptor,
        Nominal::Field(usize::MAX),
        Nominal::Set,
        Nominal::Marker,
    ] {
        let mut malformed = graph.clone();
        let instruction = malformed
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.instructions)
            .find(|instruction| matches!(instruction.operation, ir::Operation::Nominal { .. }))
            .unwrap();
        let ir::Operation::Nominal {
            operation: target, ..
        } = &mut instruction.operation
        else {
            unreachable!()
        };
        *target = operation;
        assert!(ir::verify(&malformed).is_err(), "{operation:?}");
        assert!(portable::compile_ir(&malformed).is_err());
    }
}

#[test]
fn nominal_compiler_keys_are_phase_isolated_stable_and_inaccessible_to_source() {
    let environment = Environment::default();
    let source = "(defprotocol Reader (read-it [this])) (deftype Read [value] Reader (read-it [this] value))";
    let runtime = portable::prepare_fragment(source, &environment, Phase::Runtime).unwrap();
    let macro_phase = portable::prepare_fragment(source, &environment, Phase::Macro).unwrap();
    for (fragment, phase) in [(&runtime, Phase::Runtime), (&macro_phase, Phase::Macro)] {
        runtime_abi::verify_artifact(&fragment.wasm, &runtime_abi::Manifest::default()).unwrap();
        assert!(fragment.cells.iter().all(|cell| cell.phase() == phase));
        let keys = fragment
            .cells
            .iter()
            .filter(|cell| cell.namespace() == "suss.internal.protocol-keys")
            .collect::<Vec<_>>();
        assert_eq!(keys.len(), 2);
        for key in keys {
            assert!(
                portable::compile_in(&key.import_name(), &fragment.environment, phase).is_err()
            );
        }
    }
    let redeclared = portable::prepare_fragment(
        "(defprotocol Reader (read-it [this]))",
        &runtime.environment,
        Phase::Runtime,
    )
    .unwrap();
    let keys = |environment: &Environment| {
        environment
            .cells()
            .into_iter()
            .filter(|cell| cell.namespace() == "suss.internal.protocol-keys")
            .collect::<Vec<_>>()
    };
    assert_eq!(keys(&runtime.environment), keys(&redeclared.environment));
    assert_ne!(keys(&runtime.environment), keys(&macro_phase.environment));
}

#[test]
fn live_protocol_cell_reference_rejects_forged_hir_and_ir_result_types() {
    let mut environment = Environment::default();
    let global = environment
        .declare_cell(Phase::Runtime, "user", "read")
        .unwrap();
    let mut hir = Hir {
        span: 1..4,
        metadata: vec![],
        ty: Type::Number,
        kind: Expression::GlobalCell(global),
    };
    assert!(
        ir::lower(&hir)
            .unwrap_err()
            .message
            .contains("cell-reference")
    );
    hir.ty = Type::Value;
    let mut function = ir::lower(&hir).unwrap();
    ir::verify(&function).unwrap();
    let result = function.blocks[0].instructions[0].result;
    function.values[result.0].ty = Type::Bool;
    assert!(ir::verify(&function).is_err());
}
