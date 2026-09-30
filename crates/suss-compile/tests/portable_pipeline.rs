mod support;
use suss_compile::{portable, runtime_abi};
use wasmtime::{Instance, Linker, Module, Store, Val};

fn execute(source: &str) -> (Store<()>, Val) {
    let fragment = portable::prepare_fragment(
        source,
        &portable::resolve::Environment::default(),
        portable::resolve::Phase::Runtime,
    )
    .unwrap();
    execute_fragment(fragment.wasm, fragment.cells)
}

fn execute_bytes(bytes: Vec<u8>) -> (Store<()>, Val) {
    execute_fragment(bytes, Vec::new())
}
fn execute_fragment(bytes: Vec<u8>, cells: Vec<portable::resolve::Global>) -> (Store<()>, Val) {
    runtime_abi::verify_artifact(&bytes, &runtime_abi::Manifest::default()).unwrap();
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let mut linker = Linker::new(&engine);
    linker
        .instance(&mut store, "suss.runtime", runtime)
        .unwrap();
    let mut class = [Val::null_any_ref()];
    runtime
        .get_func(&mut store, "core-exception-info-class")
        .unwrap()
        .call(&mut store, &[], &mut class)
        .unwrap();
    let mut class_cell = [Val::null_any_ref()];
    runtime
        .get_func(&mut store, "binding-new")
        .unwrap()
        .call(&mut store, &class, &mut class_cell)
        .unwrap();
    for identity in cells {
        let mut value = [Val::null_any_ref()];
        let arithmetic = if identity.namespace() == "suss.core" {
            match identity.name() {
                "+" => Some("add"),
                "-" => Some("subtract"),
                "*" => Some("multiply"),
                "/" => Some("divide"),
                _ => None,
            }
        } else {
            None
        };
        let core_export = if identity.namespace() == "suss.core" {
            match identity.name() {
                "native-satisfies?" => Some("native-satisfies-function"),
                "nil?" => Some("predicate-nil"),
                "false?" => Some("predicate-false"),
                "true?" => Some("predicate-true"),
                "undefined?" => Some("predicate-undefined"),
                "number?" => Some("predicate-number"),
                "string?" => Some("predicate-string"),
                "identical?" => Some("predicate-identical"),
                "ExceptionInfo" => Some("core-exception-info-class"),
                "ex-info" => Some("core-ex-info"),
                "ex-data" => Some("core-ex-data"),
                "ex-message" => Some("core-ex-message"),
                "ex-cause" => Some("core-ex-cause"),
                _ => None,
            }
        } else {
            None
        };
        if identity.namespace() == "suss.core" && identity.name() == "ExceptionInfo" {
            value = class_cell.clone();
        } else if let Some(export) = core_export {
            let mut function = [Val::null_any_ref()];
            if export == "core-ex-info" {
                let nil = runtime.get_func(&mut store, "nil").unwrap();
                nil.call(&mut store, &[], &mut value).unwrap();
                let mut cell = [Val::null_any_ref()];
                runtime
                    .get_func(&mut store, "binding-new")
                    .unwrap()
                    .call(&mut store, &value, &mut cell)
                    .unwrap();
                runtime
                    .get_func(&mut store, export)
                    .unwrap()
                    .call(
                        &mut store,
                        &[class_cell[0].clone(), cell[0].clone()],
                        &mut function,
                    )
                    .unwrap();
                runtime
                    .get_func(&mut store, "binding-set")
                    .unwrap()
                    .call(&mut store, &[cell[0].clone(), function[0].clone()], &mut [])
                    .unwrap();
                value = cell;
            } else {
                runtime
                    .get_func(&mut store, export)
                    .unwrap()
                    .call(
                        &mut store,
                        if export.starts_with("predicate-") || export == "native-satisfies-function"
                        {
                            &[]
                        } else {
                            &class_cell
                        },
                        &mut function,
                    )
                    .unwrap();
                runtime
                    .get_func(&mut store, "binding-new")
                    .unwrap()
                    .call(&mut store, &function, &mut value)
                    .unwrap();
            }
        } else if let Some(name) = arithmetic {
            let mut function = [Val::null_any_ref()];
            runtime
                .get_func(&mut store, &format!("arithmetic-{name}"))
                .unwrap()
                .call(&mut store, &[], &mut function)
                .unwrap();
            runtime
                .get_func(&mut store, "binding-new")
                .unwrap()
                .call(&mut store, &function, &mut value)
                .unwrap();
        } else {
            runtime
                .get_func(&mut store, "binding-unbound")
                .unwrap()
                .call(&mut store, &[], &mut value)
                .unwrap();
        }
        let ty = value[0]
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap()
            .ty(&store)
            .unwrap();
        let global = wasmtime::Global::new(
            &mut store,
            wasmtime::GlobalType::new(
                wasmtime::ValType::Ref(wasmtime::RefType::new(false, ty.into())),
                wasmtime::Mutability::Const,
            ),
            value[0].clone(),
        )
        .unwrap();
        linker
            .define(
                &store,
                identity.import_module(),
                &identity.import_name(),
                global,
            )
            .unwrap();
    }
    let fragment = linker
        .instantiate(&mut store, &Module::new(&engine, bytes).unwrap())
        .unwrap();
    let mut result = [Val::null_any_ref()];
    fragment
        .get_func(&mut store, "eval")
        .unwrap()
        .call(&mut store, &[], &mut result)
        .unwrap();
    store.gc(None).unwrap();
    (store, result[0].clone())
}

#[test]
fn compiled_literal_uses_shared_abi_and_binary64_rounding() {
    let (mut store, value) = execute("9007199254740993");
    let object = value
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap();
    assert_eq!(
        object.field(&mut store, 0).unwrap().unwrap_f64().to_bits(),
        9007199254740992.0f64.to_bits()
    );
}

fn number(source: &str) -> f64 {
    let (mut store, value) = execute(source);
    value
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap()
        .field(&mut store, 0)
        .unwrap()
        .unwrap_f64()
}
#[test]
fn empty_let_body_returns_nil() {
    let (store, value) = execute("(let [x 1])");
    assert_eq!(
        value
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_u32(),
        0
    );
}
#[test]
fn compiled_scalars_match_the_pinned_reader_observations() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/reader-cases.json")).unwrap();
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let mut ids = std::collections::HashSet::new();
    for case in corpus["cases"].as_array().unwrap() {
        assert!(ids.insert(case["id"].as_str().unwrap()));
        let (mut store, value) = execute(case["source"].as_str().unwrap());
        let reference = value.unwrap_anyref().unwrap();
        let actual = match case["expected"]["tag"].as_str().unwrap() {
            "f64" => {
                let object = reference.as_struct(&store).unwrap().unwrap();
                let bits = object.field(&mut store, 0).unwrap().unwrap_f64().to_bits();
                serde_json::json!({"tag":"f64","bits":format!("{bits:016x}")})
            }
            "string" => {
                let array = reference.as_array(&store).unwrap().unwrap();
                let units = array
                    .elems(&mut store)
                    .unwrap()
                    .map(|unit| unit.unwrap_i32() as u16)
                    .collect::<Vec<_>>();
                serde_json::json!({"tag":"string","units":units})
            }
            _ => panic!("unknown oracle tag"),
        };
        assert_eq!(actual, case["expected"], "{}", case["source"]);
    }
    assert_eq!(ids.len(), 14);
}
#[test]
fn numeric_calls_and_truthiness_execute_with_portable_bits() {
    for (source, expected) in [
        ("(+ 9007199254740992 1)", 9007199254740992.0f64),
        ("(- 0)", -0.0),
        ("(+ -0.0)", -0.0),
        ("(/ 4)", 0.25),
        ("(+)", 0.0),
        ("(*)", 1.0),
        ("(- 10 3 2)", 5.0),
        ("(/ 20 2 5)", 2.0),
        ("(if 0 1 2)", 1.0),
        ("(if \"\" 1 2)", 1.0),
        ("(if nil 1 2)", 2.0),
        ("(if false 1 2)", 2.0),
        ("(if true 1 2)", 1.0),
        ("(let [x 2 x (+ x 3)] (let [x 7] x) (* x 4))", 20.0),
        ("(+ (if false (* 3 4) (/ 8 2)) (if true 2 9))", 6.0),
        ("#?(:jvm 1 :cljs (suss.core/+ 1 (cljs.core/* 2 3)))", 7.0),
    ] {
        assert_eq!(number(source).to_bits(), expected.to_bits(), "{source}");
    }
    assert!(number("(/ 0 0)").is_nan());
    assert_eq!(number("(/ 1 0)"), f64::INFINITY);
}
#[test]
fn unary_sum_and_product_preserve_operand_types_and_closure_arity() {
    use portable::hir::Type;
    for (source, expected) in [
        ("(+ nil)", Type::Nil),
        ("(* false)", Type::Bool),
        ("(+ \"\\uD800\")", Type::String),
        ("(* (fn [x] x))", Type::Closure(1)),
        ("(+ (if true 1 nil))", Type::Value),
    ] {
        let forms = suss_reader::forms::read_forms(source).unwrap();
        let hir = portable::hir::analyze(&forms, 0..source.len()).unwrap();
        assert_eq!(hir.ty, expected, "{source}");
        // Validate and execute the fragment; the shared corpus independently
        // checks the scalar results and calls identity-returned closures.
        execute(source);
    }
    let source = "((+ (fn [x] x)))";
    let error = portable::compile(source).unwrap_err();
    assert_eq!(error.span, 0..source.len());
    assert!(error.message.contains("Wrong arity"));
    // Identity must retain String information when a later addition concatenates.
    let source = "(+ (+ \"x\") 1)";
    assert_eq!(portable::analyze(source).unwrap().ty, Type::String);
    let (mut store, value) = execute(source);
    assert_eq!(
        tagged(&mut store, &value),
        serde_json::json!({"tag":"string","units":[120,49]})
    );
    let error = portable::compile("(- (+ (fn [] 1)))").unwrap_err();
    assert!(error.message.contains("object coercions"));
}
#[test]
fn arithmetic_import_trace_observes_once_only_source_order_and_short_circuit() {
    use wasmtime::Func;
    let engine = support::engine();
    let mut store = Store::new(&engine, Vec::<String>::new());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let mut linker = Linker::new(&engine);
    linker
        .instance(&mut store, "suss.runtime", runtime)
        .unwrap();
    linker.allow_shadowing(true);
    for name in ["number-add", "number-multiply", "number-divide"] {
        let function = runtime.get_func(&mut store, name).unwrap();
        let ty = function.ty(&store);
        let wrapper = Func::new(&mut store, ty, move |mut caller, args, results| {
            caller.data_mut().push(name.into());
            function.call(&mut caller, args, results)
        });
        linker
            .define(&store, "suss.runtime", name, wrapper)
            .unwrap();
    }
    // Unary identities must return each already-evaluated operand without
    // replaying it. The ordered import trace catches duplicate evaluation;
    // assigning an identical literal to a binding twice would not.
    let bytes = portable::compile("(+ (+ (if (* 2 3) (/ 8 4) (* 9 9))) (* (* 3 4)))").unwrap();
    let fragment = linker
        .instantiate(&mut store, &Module::new(&engine, bytes).unwrap())
        .unwrap();
    let mut result = [Val::null_any_ref()];
    fragment
        .get_func(&mut store, "eval")
        .unwrap()
        .call(&mut store, &[], &mut result)
        .unwrap();
    assert_eq!(
        store.data(),
        &[
            "number-multiply",
            "number-divide",
            "number-multiply",
            "number-add"
        ]
    );
    store.gc(None).unwrap();
    let value = result[0]
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap()
        .field(&mut store, 0)
        .unwrap()
        .unwrap_f64();
    assert_eq!(value, 14.0);
}
#[test]
fn hir_retains_binding_identity_metadata_and_located_diagnostics() {
    use portable::hir::Expression;
    let source = "(let [^:first x 1] (let [x 2] x) ^:last x)";
    let hir = portable::analyze(source).unwrap();
    let Expression::Do(top) = hir.kind else {
        panic!()
    };
    let Expression::Let { bindings, body } = &top[0].kind else {
        panic!()
    };
    assert_eq!(bindings[0].metadata.len(), 1);
    let outer = bindings[0].id;
    let Expression::Do(items) = &body.kind else {
        panic!()
    };
    let Expression::Let {
        bindings: inner, ..
    } = &items[0].kind
    else {
        panic!()
    };
    assert_ne!(outer, inner[0].id);
    assert!(matches!(items[1].kind,Expression::Local(id) if id==outer));
    assert_eq!(items[1].metadata.len(), 1);
    assert_eq!(&source[items[1].span.clone()], "^:last x");
    for (source, needle) in [
        ("(let [x missing] x)", "missing"),
        ("(+ 1 (fn [] 2))", "(fn [] 2)"),
        ("(let [+ (fn [x] x)] (+ 2 3))", "(+ 2 3)"),
        ("(other.core/+ 1 2)", "other.core/+"),
    ] {
        let error = portable::compile(source).unwrap_err();
        assert_eq!(&source[error.span], needle);
    }
    for source in ["(/)", "(-)", "(if 1)", "(let [x] x)", "(recur 1)"] {
        assert!(portable::compile(source).is_err(), "{source}");
    }
}
#[test]
fn verifier_rejects_undefined_non_dominating_wrong_type_and_arity_values() {
    use portable::{
        hir::Type,
        ir::{self, Operation, Terminator, ValueId},
    };
    let source = "(+ (if true 1 2) 3)";
    let function = ir::lower(&portable::analyze(source).unwrap()).unwrap();
    ir::verify(&function).unwrap();
    let mut bad = function.clone();
    if let Terminator::Branch { condition, .. } = &mut bad.blocks[0].terminator {
        *condition = ValueId(usize::MAX);
    }
    assert!(
        ir::verify(&bad)
            .unwrap_err()
            .message
            .contains("no definition")
    );
    let mut bad = function.clone();
    let then_value = bad.blocks[1].instructions[0].result;
    if let Terminator::Branch { condition, .. } = &mut bad.blocks[0].terminator {
        *condition = then_value;
    }
    assert!(ir::verify(&bad).unwrap_err().message.contains("dominate"));
    let mut bad = function.clone();
    bad.values[then_value.0].ty = Type::String;
    assert!(
        ir::verify(&bad)
            .unwrap_err()
            .message
            .contains("literal result")
    );
    let mut bad = function.clone();
    if let Terminator::Jump { arguments, .. } = &mut bad.blocks[1].terminator {
        arguments.clear();
    }
    assert!(ir::verify(&bad).unwrap_err().message.contains("edge arity"));
    let mut bad = function.clone();
    for block in &mut bad.blocks {
        for instruction in &mut block.instructions {
            if let Operation::Arithmetic { arguments, .. } = &mut instruction.operation {
                arguments.clear();
            }
        }
    }
    assert!(
        ir::verify(&bad)
            .unwrap_err()
            .message
            .contains("intrinsic arity")
    );
    assert!(portable::compile_ir(&bad).is_err());

    // A forged Number result would let a subsequent intrinsic use an unchecked
    // Number cast even though addition can produce a string or dynamic value.
    for source in ["(+ \"1\" 2)", "(+ (if true 1 nil) 2)"] {
        let mut bad = ir::lower(&portable::analyze(source).unwrap()).unwrap();
        ir::verify(&bad).unwrap();
        let result = bad
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .find(|instruction| matches!(instruction.operation, Operation::Arithmetic { .. }))
            .unwrap()
            .result;
        assert_ne!(bad.values[result.0].ty, Type::Number);
        bad.values[result.0].ty = Type::Number;
        assert!(
            ir::verify(&bad)
                .unwrap_err()
                .message
                .contains("arithmetic operand/result type mismatch"),
            "{source}"
        );
        assert!(portable::compile_ir(&bad).is_err(), "{source}");
    }
}

#[test]
fn malformed_hir_returns_a_located_lowering_diagnostic() {
    use portable::hir::{BindingId, Expression, Hir, Type};
    let hir = Hir {
        span: 3..9,
        metadata: Vec::new(),
        ty: Type::Number,
        kind: Expression::Local(BindingId(99)),
    };
    let error = portable::ir::lower(&hir).unwrap_err();
    assert_eq!(error.span, 3..9);
    assert!(error.message.contains("undefined binding"));
}
#[test]
fn edge_parameter_replacements_are_parallel_in_executed_ir() {
    use portable::{
        hir::{Arithmetic, Literal, Type},
        ir::{Block, Function, Instruction, Operation, Terminator, Value, ValueId},
    };
    let types = [
        Type::Number,
        Type::Number,
        Type::Bool,
        Type::Number,
        Type::Number,
        Type::Bool,
        Type::Bool,
        Type::Number,
    ];
    let literal = |result, value| Instruction {
        result: ValueId(result),
        operation: Operation::Literal(value),
        span: 0..1,
    };
    let function = Function {
        span: 0..1,
        values: types
            .into_iter()
            .map(|ty| Value { ty, span: 0..1 })
            .collect(),
        blocks: vec![
            Block {
                parameters: vec![],
                instructions: vec![
                    literal(0, Literal::Number(10.0)),
                    literal(1, Literal::Number(20.0)),
                    literal(2, Literal::Bool(true)),
                ],
                terminator: Terminator::Jump {
                    target: 1,
                    arguments: vec![ValueId(0), ValueId(1), ValueId(2)],
                },
            },
            Block {
                parameters: vec![ValueId(3), ValueId(4), ValueId(5)],
                instructions: vec![],
                terminator: Terminator::Branch {
                    condition: ValueId(5),
                    consequent: 2,
                    alternative: 3,
                },
            },
            Block {
                parameters: vec![],
                instructions: vec![literal(6, Literal::Bool(false))],
                terminator: Terminator::Jump {
                    target: 1,
                    arguments: vec![ValueId(4), ValueId(3), ValueId(6)],
                },
            },
            Block {
                parameters: vec![],
                instructions: vec![Instruction {
                    result: ValueId(7),
                    operation: Operation::Arithmetic {
                        operator: Arithmetic::Subtract,
                        arguments: vec![ValueId(3), ValueId(4)],
                    },
                    span: 0..1,
                }],
                terminator: Terminator::Return(ValueId(7)),
            },
        ],
    };
    let (mut store, value) = execute_bytes(portable::compile_ir(&function).unwrap());
    let result = value
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap()
        .field(&mut store, 0)
        .unwrap()
        .unwrap_f64();
    assert_eq!(result, 10.0); // sequential replacement would incorrectly produce zero
}
fn tagged(store: &mut Store<()>, value: &Val) -> serde_json::Value {
    let reference = value
        .unwrap_anyref()
        .expect("unexpected Wasm null, not language nil");
    if let Some(i31) = reference.as_i31(&*store).unwrap() {
        return match i31.get_u32() {
            0 | 6 => serde_json::json!({"tag":"nil"}),
            2 => serde_json::json!({"tag":"bool","value":false}),
            4 => serde_json::json!({"tag":"bool","value":true}),
            _ => panic!("unknown i31 runtime value"),
        };
    }
    if let Some(object) = reference.as_struct(&*store).unwrap() {
        let fields = object.fields(&mut *store).unwrap().collect::<Vec<_>>();
        let [Val::F64(bits)] = fields.as_slice() else {
            panic!("unknown GC object layout")
        };
        return serde_json::json!({"tag":"f64","bits":format!("{bits:016x}")});
    }
    let array = reference
        .as_array(&*store)
        .unwrap()
        .expect("unknown GC value");
    assert!(matches!(
        array.ty(&*store).unwrap().element_type(),
        wasmtime::StorageType::I16
    ));
    let units = array
        .elems(&mut *store)
        .unwrap()
        .map(|unit| unit.unwrap_i32() as u16)
        .collect::<Vec<_>>();
    serde_json::json!({"tag":"string","units":units})
}
#[test]
fn compiled_source_cases_match_the_pinned_compiler_observations() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/oracle/portable-cases.json")).unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let mut ids = std::collections::HashSet::new();
    for case in corpus["cases"].as_array().unwrap() {
        assert!(ids.insert(case["id"].as_str().unwrap()));
        let source = case["source"].as_str().unwrap();
        let (mut store, value) = execute(source);
        assert_eq!(tagged(&mut store, &value), case["expected"], "{source}");
    }
    assert_eq!(ids.len(), 397);
}
#[test]
fn independently_compiled_fragment_values_remain_live_across_gc() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let mut linker = Linker::new(&engine);
    linker
        .instance(&mut store, "suss.runtime", runtime)
        .unwrap();
    let run = |store: &mut Store<()>, source: &str| {
        let bytes = portable::compile(source).unwrap();
        runtime_abi::verify_artifact(&bytes, &runtime_abi::Manifest::default()).unwrap();
        let module = Module::new(&engine, bytes).unwrap();
        let instance = linker.instantiate(&mut *store, &module).unwrap();
        let mut result = [Val::null_any_ref()];
        instance
            .get_func(&mut *store, "eval")
            .unwrap()
            .call(&mut *store, &[], &mut result)
            .unwrap();
        result[0].clone()
    };
    let old = run(&mut store, r#""\uD800😀""#);
    store.gc(None).unwrap();
    let new = run(&mut store, "(let [x 9007199254740992] (+ x 1))");
    store.gc(None).unwrap();
    assert_eq!(
        tagged(&mut store, &old),
        serde_json::json!({"tag":"string","units":[0xd800,0xd83d,0xde00]})
    );
    assert_eq!(
        tagged(&mut store, &new),
        serde_json::json!({"tag":"f64","bits":"4340000000000000"})
    );
}

#[test]
fn lexical_bindings_hide_the_bootstrap_let_macro() {
    // The local callee now lowers to a universal call rather than a bootstrap macro.
    assert_eq!(number("(let [let (fn [x] x)] (let 7))"), 7.0);
    // True special forms remain special even when their names are locals.
    assert_eq!(number("(let [if 7 do 8] (if true (do 1 2) 3))"), 2.0);
    // A nested local hiding let does not escape its lexical scope.
    assert_eq!(number("(do (let [let 7] let) (let [x 3] x))"), 3.0);
}

#[test]
fn public_hir_negation_executes_and_malformed_arithmetic_is_rejected() {
    use portable::hir::{Arithmetic, Expression, Hir, Literal, Type};
    let make = |operator, count| Hir {
        span: 4..12,
        metadata: Vec::new(),
        ty: Type::Number,
        kind: Expression::Arithmetic {
            operator,
            arguments: (0..count)
                .map(|_| Hir {
                    span: 7..8,
                    metadata: Vec::new(),
                    ty: Type::Number,
                    kind: Expression::Literal(Literal::Number(0.0)),
                })
                .collect(),
        },
    };
    let ir = portable::ir::lower(&make(Arithmetic::Negate, 1)).unwrap();
    let (mut store, value) = execute_bytes(portable::compile_ir(&ir).unwrap());
    assert_eq!(
        tagged(&mut store, &value),
        serde_json::json!({"tag":"f64","bits":"8000000000000000"})
    );
    for (operator, count) in [
        (Arithmetic::Negate, 0),
        (Arithmetic::Negate, 2),
        (Arithmetic::Subtract, 0),
        (Arithmetic::Divide, 0),
    ] {
        let error = portable::ir::lower(&make(operator, count)).unwrap_err();
        assert_eq!(error.span, 4..12);
        assert!(error.message.contains("arity"));
    }
}

#[test]
fn recurrence_rejects_non_tail_wrong_arity_and_cross_function_targets_with_spans() {
    for (source, message) in [
        ("(recur)", "enclosing"),
        ("(loop [x 1] (recur))", "arity"),
        ("(loop [x 1] (+ (recur 2) 1))", "tail position"),
        ("(loop [x 1] (if (recur 2) x x))", "tail position"),
        ("(loop [x 1] (do (recur 2) x))", "tail position"),
        ("(loop [x 1] (let [y (recur 2)] y))", "tail position"),
        ("(loop [x 1] (fn [] (recur 2)))", "arity"),
        ("(loop [x 1] ((recur 2)))", "tail position"),
        ("(loop [x 1] (recur (recur 2)))", "tail position"),
    ] {
        let errors = portable::compile(source).unwrap_err();
        assert!(errors.message.contains(message), "{source}: {errors:?}");
        assert!(
            source[errors.span.clone()].starts_with("(recur"),
            "{source}: {errors:?}"
        );
    }
    assert_eq!(number("(let [loop (fn [x] x)] (loop 7))"), 7.0);
    assert_eq!(
        number("(cljs.core/loop [flag true x 1] (if flag (recur false 2) x))"),
        2.0
    );
    assert_eq!(number("(loop* [x 3] x)"), 3.0);
}

#[test]
fn malformed_public_hir_cannot_recur_from_operands_or_to_outer_targets() {
    use portable::hir::{Expression, Hir, LoopId, Type};
    let recur = Hir {
        span: 8..15,
        metadata: vec![],
        ty: Type::Value,
        kind: Expression::Recur {
            target: LoopId(0),
            arguments: vec![],
        },
    };
    let mut root = Hir {
        span: 0..20,
        metadata: vec![],
        ty: Type::Value,
        kind: Expression::Loop {
            target: LoopId(0),
            bindings: vec![],
            body: Box::new(recur.clone()),
        },
    };
    let graph = portable::ir::lower(&root).unwrap();
    portable::ir::verify(&graph).unwrap();
    if let Expression::Loop { body, .. } = &mut root.kind {
        *body = Box::new(Hir {
            span: 8..18,
            metadata: vec![],
            ty: Type::Value,
            kind: Expression::Do(vec![recur.clone(), recur.clone()]),
        });
    }
    assert!(
        portable::ir::lower(&root)
            .unwrap_err()
            .message
            .contains("tail position")
    );
    if let Expression::Loop { body, .. } = &mut root.kind {
        *body = Box::new(Hir {
            span: 8..18,
            metadata: vec![],
            ty: Type::Value,
            kind: Expression::Loop {
                target: LoopId(1),
                bindings: vec![],
                body: Box::new(recur),
            },
        });
    }
    assert!(
        portable::ir::lower(&root)
            .unwrap_err()
            .message
            .contains("target or arity")
    );
}

#[test]
fn nil_test_verifier_checks_result_type_and_operand_definition() {
    use portable::{
        hir::Type,
        ir::{self, Operation, ValueId},
    };
    let hir = portable::analyze("(suss.bootstrap/nil? nil)").unwrap();
    let function = ir::lower(&hir).unwrap();
    ir::verify(&function).unwrap();
    let instruction = function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.operation, Operation::NilTest(_)))
        .unwrap();
    let mut bad = function.clone();
    bad.values[instruction.result.0].ty = Type::Number;
    assert!(ir::verify(&bad).unwrap_err().message.contains("nil-test"));
    let mut bad = function.clone();
    let operation = bad
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.operation, Operation::NilTest(_)))
        .unwrap();
    operation.operation = Operation::NilTest(ValueId(usize::MAX));
    assert!(
        ir::verify(&bad)
            .unwrap_err()
            .message
            .contains("no definition")
    );
    let mut bad = function.clone();
    let instruction = bad
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
        .find(|i| matches!(i.operation, Operation::NilTest(_)))
        .unwrap();
    instruction.operation = Operation::NilTest(instruction.result);
    assert!(
        ir::verify(&bad)
            .unwrap_err()
            .message
            .contains("does not dominate")
    );
    let mut bad_hir = hir;
    let portable::hir::Expression::Do(items) = &mut bad_hir.kind else {
        panic!("top-level forms")
    };
    items[0].ty = Type::Number;
    assert!(
        ir::lower(&bad_hir)
            .unwrap_err()
            .message
            .contains("nil-test HIR")
    );
}
