//! Public dynamic HIR/IR guards, independent from native source/runtime execution.
use suss_compile::portable::{
    self,
    hir::{Expression, Hir, Literal, Type},
    ir::{self, Operation},
    resolve::{Environment, Phase},
};
#[test]
fn dynamic_scope_requires_a_fixed_body_and_complete_snapshot_value_pairs() {
    let e = Environment::default();
    let fragment = portable::prepare_fragment("(def ^:dynamic *x* 1)", &e, Phase::Runtime).unwrap();
    let hir = portable::analyze_in(
        "(binding [*x* 7] *x*)",
        &fragment.environment,
        Phase::Runtime,
    )
    .unwrap();
    let graph = ir::lower(&hir).unwrap();
    ir::verify(&graph).unwrap();
    portable::compile_ir(&graph).unwrap();
    let mut malformed = graph.clone();
    let inst = malformed
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.operation, Operation::DynamicScope { .. }))
        .unwrap();
    let Operation::DynamicScope { operands, .. } = &mut inst.operation else {
        unreachable!()
    };
    operands.pop();
    assert!(ir::verify(&malformed).is_err());
    assert!(portable::compile_ir(&malformed).is_err());
    let mut malformed = hir.clone();
    let Expression::Do(items) = malformed.kind_mut() else {
        panic!("fragment")
    };
    let Expression::DynamicScope { body, .. } = items[0].kind_mut() else {
        panic!("scope")
    };
    **body = Hir {
        source: None,
        span: 1..5,
        metadata: vec![],
        ty: Type::Nil,
        kind: std::sync::Arc::new(Expression::Literal(Literal::Nil)),
    };
    assert!(ir::lower(&malformed).is_err());
}
#[test]
fn dynamic_targets_preserve_namespace_aliases_phase_and_lexical_shadowing() {
    let mut e = Environment::default();
    e.enter_namespace(Phase::Runtime, "config").unwrap();
    let fragment = portable::prepare_fragment("(def ^:dynamic *x* 1)", &e, Phase::Runtime).unwrap();
    let mut e = fragment.environment;
    e.enter_namespace(Phase::Runtime, "user").unwrap();
    e.alias(Phase::Runtime, "cfg", "config").unwrap();
    portable::compile_in("(binding [cfg/*x* 7] config/*x*)", &e, Phase::Runtime).unwrap();
    assert!(portable::compile_in("(binding [cfg/*x* 7] config/*x*)", &e, Phase::Macro).is_err());
    portable::compile_in("(let [binding (fn [x] x)] (binding 7))", &e, Phase::Runtime).unwrap();
}

#[test]
fn a_global_read_reads_a_static_root_inline_and_asks_binding_get_otherwise() {
    // Only a cell a binding frame has held searches the frames; any other
    // bound cell's read is a field read at the read site (#354).
    use wasmparser::{Operator, Parser, Payload};
    let e = Environment::default();
    let fragment = portable::prepare_fragment("(def x 1)", &e, Phase::Runtime).unwrap();
    let hir = portable::analyze_in("x", &fragment.environment, Phase::Runtime).unwrap();
    let wasm = portable::compile_ir(&ir::lower(&hir).unwrap()).unwrap();
    let mut operators = vec![];
    for payload in Parser::new(0).parse_all(&wasm) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            let mut reader = body.get_operators_reader().unwrap();
            while !reader.eof() {
                operators.push(format!("{:?}", reader.read().unwrap()));
            }
        }
    }
    let field = |field_index| format!("{:?}", Operator::StructGet { struct_type_index: 5, field_index });
    let at = operators.iter().position(|op| *op == field(1)).expect("flag read");
    // Exactly bound (CELL_BOUND, not CELL_DYNAMIC): the root, inline.
    assert_eq!(operators[at + 1..at + 3], ["I32Const { value: 1 }", "I32Eq"]);
    assert!(operators[at + 3].starts_with("If"));
    assert_eq!(operators[at + 6], field(0));
    assert!(operators[at + 7] == "Else", "{:?}", &operators[at..]);
    assert!(operators[at + 7..].iter().any(|op| op.starts_with("Call")), "a dynamic read calls binding-get");
}
