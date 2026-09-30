mod support;
use suss_compile::{portable, runtime_abi};
use wasmtime::{Instance, Linker, Module, Store, Val};
fn execute(source: &str) -> (Store<()>, Val) {
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
    let bytes = portable::compile(source).unwrap();
    runtime_abi::verify_artifact(&bytes, &runtime_abi::Manifest::default()).unwrap();
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
fn bits<T>(store: &mut Store<T>, value: &Val) -> u64 {
    let object = value
        .unwrap_anyref()
        .unwrap()
        .as_struct(&*store)
        .unwrap()
        .unwrap();
    let fields = object.fields(store).unwrap().collect::<Vec<_>>();
    match fields.as_slice() {
        [Val::F64(bits)] => *bits,
        _ => panic!("not a number: {fields:?}"),
    }
}
#[test]
fn source_closures_capture_values_and_execute_universal_calls() {
    let (mut store, value) = execute("(let [x 7 f (fn [] x) x 9] (f))");
    assert_eq!(bits(&mut store, &value), 7.0f64.to_bits());
}

fn runtime<T>(store: &mut Store<T>) -> Instance {
    let module = Module::new(store.engine(), runtime_abi::module()).unwrap();
    Instance::new(store, &module, &[]).unwrap()
}
fn call<T>(store: &mut Store<T>, instance: Instance, name: &str, args: &[Val]) -> Val {
    let func = instance.get_func(&mut *store, name).unwrap();
    let mut result = [Val::null_any_ref()];
    func.call(store, args, &mut result).unwrap();
    result[0].clone()
}
fn fragment<T>(
    store: &mut Store<T>,
    linker: &Linker<T>,
    source: &str,
    env: &portable::resolve::Environment,
) -> Instance {
    let bytes = portable::compile_in(source, env, portable::resolve::Phase::Runtime).unwrap();
    runtime_abi::verify_artifact(&bytes, &runtime_abi::Manifest::default()).unwrap();
    let module = Module::new(store.engine(), bytes).unwrap();
    linker.instantiate(store, &module).unwrap()
}
fn define<T>(
    store: &mut Store<T>,
    linker: &mut Linker<T>,
    id: &portable::resolve::Global,
    cell: &Val,
) {
    use wasmtime::{Global, GlobalType, Mutability, RefType, ValType};
    let object = cell
        .unwrap_anyref()
        .unwrap()
        .as_struct(&*store)
        .unwrap()
        .unwrap();
    let ty = object.ty(&*store).unwrap();
    let global = Global::new(
        &mut *store,
        GlobalType::new(
            ValType::Ref(RefType::new(false, ty.into())),
            Mutability::Const,
        ),
        cell.clone(),
    )
    .unwrap();
    linker
        .define(&*store, id.import_module(), &id.import_name(), global)
        .unwrap();
}
fn catcher(arity: usize, module: &str, name: &str) -> Vec<u8> {
    use std::borrow::Cow;
    use wasm_encoder::*;
    let value = ValType::Ref(RefType::EQREF);
    let mut types = runtime_abi::prelude();
    types.ty().function(vec![value; arity], [value]);
    types.ty().function([value], []);
    let mut imports = ImportSection::new();
    imports.import(module, name, EntityType::Function(10));
    imports.import(
        "suss.runtime",
        "language-exception",
        EntityType::Tag(TagType {
            kind: TagKind::Exception,
            func_type_idx: 11,
        }),
    );
    let mut functions = FunctionSection::new();
    functions.function(10);
    let mut exports = ExportSection::new();
    exports.export("call", ExportKind::Func, 1);
    let mut function = Function::new([]);
    function
        .instruction(&Instruction::Block(BlockType::Result(value)))
        .instruction(&Instruction::TryTable(
            BlockType::Result(value),
            Cow::Borrowed(&[Catch::One { tag: 0, label: 0 }]),
        ));
    for index in 0..arity {
        function.instruction(&Instruction::LocalGet(index as u32));
    }
    function
        .instruction(&Instruction::Call(0))
        .instruction(&Instruction::End)
        .instruction(&Instruction::End)
        .instruction(&Instruction::End);
    let mut code = CodeSection::new();
    code.function(&function);
    let mut module = wasm_encoder::Module::new();
    module
        .section(&runtime_abi::Manifest::default().section())
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&exports)
        .section(&code);
    module.finish()
}
fn load_catcher<T>(store: &mut Store<T>, linker: &Linker<T>, bytes: Vec<u8>) -> Instance {
    let module = Module::new(store.engine(), bytes).unwrap();
    linker.instantiate(store, &module).unwrap()
}
fn exception<T>(store: &mut Store<T>, value: &Val, descriptor: i64, expected: &str) {
    let fields = value
        .unwrap_anyref()
        .unwrap()
        .as_struct(&*store)
        .unwrap()
        .unwrap()
        .fields(&mut *store)
        .unwrap()
        .collect::<Vec<_>>();
    assert_eq!(fields.len(), 4);
    let kind = fields[0]
        .unwrap_anyref()
        .unwrap()
        .as_struct(&*store)
        .unwrap()
        .unwrap();
    assert_eq!(kind.field(&mut *store, 0).unwrap().i64(), Some(descriptor));
    let message = fields[1]
        .unwrap_anyref()
        .unwrap()
        .as_array(&*store)
        .unwrap()
        .unwrap();
    assert!(matches!(
        message.ty(&*store).unwrap().element_type(),
        wasmtime::StorageType::I16
    ));
    let units = message
        .elems(&mut *store)
        .unwrap()
        .map(|unit| unit.unwrap_i32() as u16)
        .collect::<Vec<_>>();
    assert_eq!(String::from_utf16(&units).unwrap(), expected);
    for nil in &fields[2..] {
        assert_eq!(
            nil.unwrap_anyref()
                .unwrap()
                .as_i31(&*store)
                .unwrap()
                .unwrap()
                .get_i32(),
            0
        );
    }
}

#[test]
fn nested_closures_parameters_shadowing_and_computed_callees_execute() {
    for (source, expected) in [
        ("((fn [x] x) 9)", 9.0f64),
        ("(((fn [x] (fn [] x)) -0.0))", -0.0),
        ("((fn [f] (f 7)) (fn [x] x))", 7.0),
        ("(let [x 5 f (fn [] (+ x 1))] (f))", 6.0),
        ("(let [x 1] (let [x 2] ((fn [] x))))", 2.0),
        ("((if false (fn [] 1) (fn [] 2)))", 2.0),
        ("(let [fn (fn [x] x)] (fn 3))", 3.0),
        ("((cljs.core/fn [x] x) 6)", 6.0),
        ("((fn* [x] x) 8)", 8.0),
    ] {
        let (mut store, value) = execute(source);
        assert_eq!(bits(&mut store, &value), expected.to_bits(), "{source}");
    }
    let (mut store, value) = execute("((fn [x] x) \"\\uD800\")");
    let units = value
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap()
        .elems(&mut store)
        .unwrap()
        .map(|unit| unit.unwrap_i32() as u16)
        .collect::<Vec<_>>();
    assert_eq!(units, [0xd800]);
    let (store, value) = execute("((fn []))");
    assert_eq!(
        value
            .unwrap_anyref()
            .unwrap()
            .as_i31(&store)
            .unwrap()
            .unwrap()
            .get_i32(),
        0
    );
}

#[test]
fn captures_use_lexical_identity_and_retain_parameter_metadata() {
    use portable::hir::Expression;
    let source = "(let [unused 1 x 2] (fn [^:argument a] (fn [] (if a x 3))))";
    let hir = portable::analyze(source).unwrap();
    let Expression::Do(top) = hir.kind else {
        panic!()
    };
    let Expression::Let { bindings, body } = &top[0].kind else {
        panic!()
    };
    let Expression::Do(items) = &body.kind else {
        panic!()
    };
    let Expression::Function {
        captures,
        parameters,
        ..
    } = &items[0].kind
    else {
        panic!()
    };
    assert_eq!(captures, &[bindings[1].id]);
    assert_eq!(parameters[0].metadata.len(), 1);
    assert_eq!(&source[parameters[0].span.clone()], "^:argument a");
    // A hint is not an unchecked cast: actual primitive values are coerced.
    for (source, expected) in [
        ("((fn [^number x] (+ x 1)) 7)", 8.0f64),
        ("((fn [^number x] (+ x 1)) false)", 1.0f64),
    ] {
        let (mut store, value) = execute(source);
        assert_eq!(bits(&mut store, &value), expected.to_bits());
    }
}

#[test]
fn source_wrong_arity_and_unimplemented_signatures_are_located_diagnostics() {
    for source in ["((fn [x] x))", "(let [f (fn [x] x)] (f 1 2))"] {
        let error = portable::compile(source).unwrap_err();
        assert!(error.message.contains("Wrong arity"));
        assert!(source[error.span].starts_with('('));
    }
    for (source, needle) in [("(fn [& xs] xs)", "&"), ("(fn [[x]] x)", "[x]")] {
        let error = portable::compile(source).unwrap_err();
        assert_eq!(&source[error.span], needle);
    }
}

#[test]
fn bootstrap_fn_rejects_parameter_metadata_pre_and_post_conditions() {
    for source in [
        "((fn ^{:pre [false]} [] 1))",
        "((cljs.core/fn ^{:post [false]} [] 1))",
        "((fn ^:pre [] 1))",
    ] {
        let error = portable::compile(source).unwrap_err();
        assert!(error.message.contains("pre/post"));
        assert!(source[error.span].starts_with('^'));
    }
    let mut env = portable::resolve::Environment::default();
    env.refer(
        portable::resolve::Phase::Runtime,
        "function",
        "cljs.core",
        "fn",
    )
    .unwrap();
    let error = portable::compile_in(
        "((function ^{:pre [false]} [] 1))",
        &env,
        portable::resolve::Phase::Runtime,
    )
    .unwrap_err();
    assert!(error.message.contains("pre/post"));
    // Only the fn macro interprets signature conditions; fn* is already expanded.
    let (mut store, value) = execute("((fn* ^{:pre [false]} [] 1))");
    assert_eq!(bits(&mut store, &value), 1.0f64.to_bits());
    let (mut store, value) = execute("((fn ^:signature [] 2))");
    assert_eq!(bits(&mut store, &value), 2.0f64.to_bits());
}

#[test]
fn calls_evaluate_computed_callee_then_each_argument_exactly_once() {
    use portable::resolve::{Environment, Phase};
    use wasmtime::Func;
    let engine = support::engine();
    let mut store = Store::new(&engine, Vec::<String>::new());
    let rt = runtime(&mut store);
    let mut linker = Linker::new(&engine);
    linker.instance(&mut store, "suss.runtime", rt).unwrap();
    let mut env = Environment::default();
    let maker = fragment(&mut store, &linker, "(fn [] (fn [a b] a))", &env);
    let maker = call(&mut store, maker, "eval", &[]);
    linker.allow_shadowing(true);
    for name in ["binding-get", "invoke"] {
        let func = rt.get_func(&mut store, name).unwrap();
        let ty = func.ty(&store);
        let wrapper = Func::new(&mut store, ty, move |mut caller, args, results| {
            if name == "invoke" {
                let array = args[1].unwrap_anyref().unwrap().as_array(&caller)?.unwrap();
                let count = array.len(&caller)?;
                caller.data_mut().push(format!("invoke:{count}"));
            }
            func.call(&mut caller, args, results)?;
            if name == "binding-get" {
                let object = results[0]
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&caller)?
                    .unwrap();
                let fields = object.fields(&mut caller)?.collect::<Vec<_>>();
                let text = if let [Val::F64(bits)] = fields.as_slice() {
                    format!("read:{}", f64::from_bits(*bits))
                } else {
                    "read:closure".into()
                };
                caller.data_mut().push(text);
            }
            Ok(())
        });
        linker
            .define(&store, "suss.runtime", name, wrapper)
            .unwrap();
    }
    for (name, value) in [
        ("maker", maker),
        (
            "a",
            call(&mut store, rt, "number-box", &[Val::F64(1.0f64.to_bits())]),
        ),
        (
            "b",
            call(&mut store, rt, "number-box", &[Val::F64(2.0f64.to_bits())]),
        ),
    ] {
        let id = env.declare_cell(Phase::Runtime, "app", name).unwrap();
        let cell = call(&mut store, rt, "binding-new", &[value]);
        define(&mut store, &mut linker, &id, &cell);
    }
    let target = fragment(&mut store, &linker, "((app/maker) app/a app/b)", &env);
    let value = call(&mut store, target, "eval", &[]);
    assert_eq!(
        store.data(),
        &["read:closure", "invoke:0", "read:1", "read:2", "invoke:2"]
    );
    store.gc(None).unwrap();
    assert_eq!(bits(&mut store, &value), 1.0f64.to_bits());
}

#[test]
fn old_closure_values_and_live_global_calls_survive_rebinding_and_gc() {
    use portable::resolve::{Environment, Phase};
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let rt = runtime(&mut store);
    let mut linker = Linker::new(&engine);
    linker.instance(&mut store, "suss.runtime", rt).unwrap();
    let mut env = Environment::default();
    let id = env.declare_cell(Phase::Runtime, "app", "f").unwrap();
    let first = fragment(&mut store, &linker, "(let [x 7] (fn [] x))", &env);
    let first = call(&mut store, first, "eval", &[]);
    let cell = call(&mut store, rt, "binding-new", &[first]);
    define(&mut store, &mut linker, &id, &cell);
    let old = fragment(&mut store, &linker, "(let [f app/f] (fn [] (f)))", &env);
    let old = call(&mut store, old, "eval", &[]);
    let live = fragment(&mut store, &linker, "(fn [] (app/f))", &env);
    let live = call(&mut store, live, "eval", &[]);
    let caller_id = env.declare_cell(Phase::Runtime, "app", "caller").unwrap();
    let caller_cell = call(&mut store, rt, "binding-new", &[old.clone()]);
    define(&mut store, &mut linker, &caller_id, &caller_cell);
    let caller = fragment(&mut store, &linker, "(app/caller)", &env);
    let second = fragment(&mut store, &linker, "(fn [] 9)", &env);
    let second = call(&mut store, second, "eval", &[]);
    rt.get_func(&mut store, "binding-set")
        .unwrap()
        .call(&mut store, &[cell, second], &mut [])
        .unwrap();
    store.gc(None).unwrap();
    let value = call(&mut store, caller, "eval", &[]);
    assert_eq!(bits(&mut store, &value), 7.0f64.to_bits());
    rt.get_func(&mut store, "binding-set")
        .unwrap()
        .call(&mut store, &[caller_cell, live], &mut [])
        .unwrap();
    store.gc(None).unwrap();
    let value = call(&mut store, caller, "eval", &[]);
    assert_eq!(bits(&mut store, &value), 9.0f64.to_bits());
}

#[test]
fn dynamic_arity_errors_are_language_exceptions_and_store_remains_usable() {
    use portable::resolve::{Environment, Phase};
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let rt = runtime(&mut store);
    let mut linker = Linker::new(&engine);
    linker.instance(&mut store, "suss.runtime", rt).unwrap();
    let mut env = Environment::default();
    let id = env.declare_cell(Phase::Runtime, "app", "f").unwrap();
    let function = fragment(&mut store, &linker, "(fn [x] x)", &env);
    let function = call(&mut store, function, "eval", &[]);
    let cell = call(&mut store, rt, "binding-new", &[function]);
    define(&mut store, &mut linker, &id, &cell);
    let target = fragment(&mut store, &linker, "(app/f)", &env);
    linker.instance(&mut store, "target", target).unwrap();
    let catch = load_catcher(&mut store, &linker, catcher(0, "target", "eval"));
    let error = call(&mut store, catch, "call", &[]);
    store.gc(None).unwrap();
    exception(&mut store, &error, 1, "Wrong arity");
    let replacement = fragment(&mut store, &linker, "(fn [] 11)", &env);
    let replacement = call(&mut store, replacement, "eval", &[]);
    rt.get_func(&mut store, "binding-set")
        .unwrap()
        .call(&mut store, &[cell, replacement], &mut [])
        .unwrap();
    let value = call(&mut store, target, "eval", &[]);
    assert_eq!(bits(&mut store, &value), 11.0f64.to_bits());
}

#[test]
fn invocation_checks_non_callable_and_malformed_arrays_without_wasm_traps() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let rt = runtime(&mut store);
    let mut linker = Linker::new(&engine);
    linker.instance(&mut store, "suss.runtime", rt).unwrap();
    let catch = load_catcher(&mut store, &linker, catcher(2, "suss.runtime", "invoke"));
    let number = call(&mut store, rt, "number-box", &[Val::F64(1.0f64.to_bits())]);
    let args = call(&mut store, rt, "args-new", &[Val::I32(0)]);
    for non_callable in [number.clone(), Val::null_any_ref()] {
        let error = call(&mut store, catch, "call", &[non_callable, args.clone()]);
        exception(&mut store, &error, 3, "Not callable");
    }
    let function = fragment(
        &mut store,
        &linker,
        "(fn [] 1)",
        &portable::resolve::Environment::default(),
    );
    let function = call(&mut store, function, "eval", &[]);
    let error = call(&mut store, catch, "call", &[function, number]);
    exception(&mut store, &error, 4, "Invalid argument array");
}

#[test]
fn verifier_rejects_malformed_closure_shapes_calls_and_capture_facts() {
    use portable::{
        hir::Type,
        ir::{self, Operation, ValueId},
    };
    let original = ir::lower(&portable::analyze("(let [x 1] (fn [a] x))").unwrap()).unwrap();
    for mutation in 0..4 {
        let mut bad = original.clone();
        let closure = bad.blocks[0]
            .instructions
            .iter_mut()
            .find(|instruction| matches!(instruction.operation, Operation::MakeClosure { .. }))
            .unwrap();
        let Operation::MakeClosure { body, captures } = &mut closure.operation else {
            unreachable!()
        };
        match mutation {
            0 => {
                body.capture_types[0] = Type::String;
            }
            1 => {
                body.arity += 1;
            }
            2 => {
                captures[0] = ValueId(99);
            }
            _ => {
                body.function.blocks[0].parameters.clear();
            }
        }
        assert!(portable::compile_ir(&bad).is_err());
    }
    let original = ir::lower(&portable::analyze("((fn [x] x) 1)").unwrap()).unwrap();
    for mutation in 0..3 {
        let mut bad = original.clone();
        let call = bad.blocks[0]
            .instructions
            .iter_mut()
            .find(|instruction| matches!(instruction.operation, Operation::Call { .. }))
            .unwrap();
        let Operation::Call { operands } = &mut call.operation else {
            unreachable!()
        };
        match mutation {
            0 => operands.clear(),
            1 => {
                operands[0] = ValueId(99);
            }
            _ => {
                operands.pop();
            }
        }
        assert!(portable::compile_ir(&bad).is_err());
    }
}

#[test]
fn non_callable_errors_happen_after_argument_effects() {
    use portable::resolve::{Environment, Phase};
    use wasmtime::Func;
    let engine = support::engine();
    let mut store = Store::new(&engine, Vec::<String>::new());
    let rt = runtime(&mut store);
    let mut linker = Linker::new(&engine);
    linker.instance(&mut store, "suss.runtime", rt).unwrap();
    linker.allow_shadowing(true);
    let getter = rt.get_func(&mut store, "binding-get").unwrap();
    let ty = getter.ty(&store);
    let wrapper = Func::new(&mut store, ty, move |mut caller, args, results| {
        caller.data_mut().push("argument".into());
        getter.call(&mut caller, args, results)
    });
    linker
        .define(&store, "suss.runtime", "binding-get", wrapper)
        .unwrap();
    let mut env = Environment::default();
    let id = env.declare_cell(Phase::Runtime, "app", "arg").unwrap();
    let number = call(&mut store, rt, "number-box", &[Val::F64(2.0f64.to_bits())]);
    let cell = call(&mut store, rt, "binding-new", &[number]);
    define(&mut store, &mut linker, &id, &cell);
    let target = fragment(&mut store, &linker, "(1 app/arg)", &env);
    linker.instance(&mut store, "target", target).unwrap();
    let catch = load_catcher(&mut store, &linker, catcher(0, "target", "eval"));
    let error = call(&mut store, catch, "call", &[]);
    assert_eq!(store.data(), &["argument"]);
    exception(&mut store, &error, 3, "Not callable");
}

#[test]
fn malformed_public_closure_arity_does_not_allocate_unbounded_entry_metadata() {
    use portable::{
        hir::Type,
        ir::{self, Operation},
    };
    let mut ir = ir::lower(&portable::analyze("(fn [] 1)").unwrap()).unwrap();
    let instruction = &mut ir.blocks[0].instructions[0];
    let Operation::MakeClosure { body, .. } = &mut instruction.operation else {
        panic!()
    };
    body.arity = i32::MAX as usize;
    ir.values[instruction.result.0].ty = Type::Closure(body.arity);
    assert!(
        portable::compile_ir(&ir)
            .unwrap_err()
            .message
            .contains("entry shape")
    );
}

#[test]
fn multiple_signatures_verify_exact_methods_captures_and_isolated_recur_targets() {
    use portable::{hir::Type, ir::Operation};
    let source = "(let [outside 10] (fn local ([x] (+ x outside)) ([flag x] (if flag (recur false (local x)) x))))";
    let hir = portable::analyze(source).unwrap();
    let ir = portable::ir::lower(&hir).unwrap();
    portable::ir::verify(&ir).unwrap();
    for mutation in 0..5 {
        let mut bad = ir.clone();
        let closure = bad
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.instructions)
            .find(|instruction| {
                matches!(instruction.operation, Operation::MakeGeneralClosure { .. })
            })
            .unwrap();
        let Operation::MakeGeneralClosure { body, .. } = &mut closure.operation else {
            unreachable!()
        };
        match mutation {
            0 => body.methods.clear(),
            1 => body.methods[1].arity = body.methods[0].arity,
            2 => body.methods[0].capture_types[0] = Type::String,
            3 => body.self_capture = false,
            4 => body.methods[0].arity = usize::MAX,
            _ => unreachable!(),
        }
        assert!(portable::compile_ir(&bad).is_err(), "mutation {mutation}");
    }
    for source in [
        "(fn local ([x] (recur)) ([] 1))",
        "(fn local ([flag x] (fn [y] (recur flag x))) ([x] x))",
        "(fn local ([] 1) ([x] (+ (recur x) 1)))",
    ] {
        let error = portable::compile(source).unwrap_err();
        assert!(source[error.span].starts_with("(recur"), "{source}");
    }
}
