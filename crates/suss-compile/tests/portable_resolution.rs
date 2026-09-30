//! Namespace identities and actual shared-cell imports, independently decoded.
mod support;
use suss_compile::{
    portable::{
        self,
        resolve::{Environment, Global as Identity, Phase},
    },
    runtime_abi,
};
use suss_reader::Symbol;
use wasmtime::{
    Func, Global, GlobalType, Instance, Linker, Module, Mutability, RefType, Store, Val, ValType,
};

fn runtime<T>(store: &mut Store<T>) -> Instance {
    let module = Module::new(store.engine(), runtime_abi::module()).unwrap();
    Instance::new(store, &module, &[]).unwrap()
}
fn call<T>(store: &mut Store<T>, runtime: Instance, name: &str, args: &[Val]) -> Val {
    let func = runtime.get_func(&mut *store, name).unwrap();
    let mut result = [Val::null_any_ref()];
    func.call(store, args, &mut result).unwrap();
    result[0].clone()
}
fn number<T>(store: &mut Store<T>, runtime: Instance, number: f64) -> Val {
    call(store, runtime, "number-box", &[Val::F64(number.to_bits())])
}
fn cell_global<T>(store: &mut Store<T>, cell: &Val) -> Global {
    let object = cell
        .unwrap_anyref()
        .unwrap()
        .as_struct(&*store)
        .unwrap()
        .unwrap();
    let ty = object.ty(&*store).unwrap();
    Global::new(
        store,
        GlobalType::new(
            ValType::Ref(RefType::new(false, ty.into())),
            Mutability::Const,
        ),
        cell.clone(),
    )
    .unwrap()
}
fn define<T>(store: &mut Store<T>, linker: &mut Linker<T>, identity: &Identity, cell: &Val) {
    let global = cell_global(store, cell);
    linker
        .define(
            &*store,
            identity.import_module(),
            &identity.import_name(),
            global,
        )
        .unwrap();
}
fn fragment<T>(store: &mut Store<T>, linker: &Linker<T>, bytes: Vec<u8>) -> Instance {
    runtime_abi::verify_artifact(&bytes, &runtime_abi::Manifest::default()).unwrap();
    let module = Module::new(store.engine(), bytes).unwrap();
    linker.instantiate(store, &module).unwrap()
}
fn eval<T>(store: &mut Store<T>, instance: Instance) -> Val {
    call(store, instance, "eval", &[])
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
        _ => panic!("not a binary64 box: {fields:?}"),
    }
}

#[test]
fn namespace_globals_lower_into_shared_binding_reads() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = runtime(&mut store);
    let mut env = Environment::new("app").unwrap();
    let identity = env.declare_cell(Phase::Runtime, "app", "value").unwrap();
    let old = number(&mut store, runtime, -0.0);
    let cell = call(&mut store, runtime, "binding-new", &[old]);
    let mut linker = Linker::new(&engine);
    linker
        .instance(&mut store, "suss.runtime", runtime)
        .unwrap();
    define(&mut store, &mut linker, &identity, &cell);
    let bytes = portable::compile_in("^:retained app/value", &env, Phase::Runtime).unwrap();
    let first = fragment(&mut store, &linker, bytes);
    let old_value = eval(&mut store, first);
    assert_eq!(bits(&mut store, &old_value), (-0.0f64).to_bits());
    let value = number(&mut store, runtime, 9007199254740992.0);
    runtime
        .get_func(&mut store, "binding-set")
        .unwrap()
        .call(&mut store, &[cell.clone(), value], &mut [])
        .unwrap();
    let second = fragment(
        &mut store,
        &linker,
        portable::compile_in("value", &env, Phase::Runtime).unwrap(),
    );
    store.gc(None).unwrap();
    // Old code reads the current cell, while previously returned values remain rooted.
    for instance in [first, second] {
        let value = eval(&mut store, instance);
        assert_eq!(bits(&mut store, &value), 9007199254740992.0f64.to_bits());
    }
    assert_eq!(bits(&mut store, &old_value), (-0.0f64).to_bits());
    let hir = portable::analyze_in("^:retained app/value", &env, Phase::Runtime).unwrap();
    let portable::hir::Expression::Do(body) = hir.kind else {
        panic!()
    };
    let portable::hir::Expression::Global(global) = &body[0].kind else {
        panic!()
    };
    assert_eq!(global, &identity);
    assert_eq!(body[0].metadata.len(), 1);
    assert_eq!(body[0].span, 0..20);
}

#[test]
fn namespace_aliases_refers_exclusions_and_shadowing_share_resolution() {
    use portable::resolve::Binding;
    let mut env = Environment::new("app").unwrap();
    let target = env
        .declare_cell(Phase::Runtime, "dependency", "value")
        .unwrap();
    env.alias(Phase::Runtime, "dep", "dependency").unwrap();
    env.refer(Phase::Runtime, "renamed", "dependency", "value")
        .unwrap();
    for source in ["dep/value", "dependency/value", "renamed"] {
        assert_eq!(
            env.resolve(Phase::Runtime, &Symbol::parse(source), 4..8)
                .unwrap()
                .global(),
            &target
        );
    }
    let suss = env
        .resolve(Phase::Runtime, &Symbol::parse("suss.core/+"), 0..1)
        .unwrap();
    assert_eq!(
        suss,
        env.resolve(Phase::Runtime, &Symbol::parse("cljs.core/+"), 0..1)
            .unwrap()
    );
    env.alias(Phase::Runtime, "core", "cljs.core").unwrap();
    assert_eq!(
        suss,
        env.resolve(Phase::Runtime, &Symbol::parse("core/+"), 0..1)
            .unwrap()
    );
    env.refer(Phase::Runtime, "sum", "cljs.core", "+").unwrap();
    assert!(matches!(
        env.resolve(Phase::Runtime, &Symbol::new("sum"), 0..3)
            .unwrap(),
        Binding::Arithmetic { .. }
    ));
    let expected = 7.0f64;
    let (mut store, result) = execute_in("(sum (core/+ 1 2) 4)", &env, Phase::Runtime);
    assert_eq!(bits(&mut store, &result), expected.to_bits());
    env.exclude_core(Phase::Runtime, "+").unwrap();
    let error = portable::compile_in("(+ 1 2)", &env, Phase::Runtime).unwrap_err();
    assert_eq!(error.span, 1..2);
    assert!(error.message.contains("Unresolved Runtime"));
    assert!(portable::compile_in("(cljs.core/+ 1 2)", &env, Phase::Runtime).is_ok());
    let (mut store, result) = execute_in("(let [renamed 9] renamed)", &env, Phase::Runtime);
    assert_eq!(bits(&mut store, &result), 9.0f64.to_bits());
    env.refer(Phase::Runtime, "+", "dependency", "value")
        .unwrap();
    let error = portable::compile_in("(+ 1 2)", &env, Phase::Runtime).unwrap_err();
    assert!(error.message.contains("global binding"));
    assert_eq!(error.span, 1..2);
    env.declare_cell(Phase::Runtime, "app", "let").unwrap();
    // Runtime vars do not hide core macros; only a lexical local does.
    let (mut store, result) = execute_in("(let [x 8] x)", &env, Phase::Runtime);
    assert_eq!(bits(&mut store, &result), 8.0f64.to_bits());
    env.refer(Phase::Runtime, "bindingForm", "cljs.core", "let")
        .unwrap();
    env.declare_cell(Phase::Runtime, "cljs.core", "let")
        .unwrap();
    let (mut store, result) = execute_in("(bindingForm [x 12] x)", &env, Phase::Runtime);
    assert_eq!(bits(&mut store, &result), 12.0f64.to_bits());
    // The same resolver selects qualified/aliased bootstrap macros.
    let (mut store, result) = execute_in("(core/let [x 6] x)", &env, Phase::Runtime);
    assert_eq!(bits(&mut store, &result), 6.0f64.to_bits());
}
fn execute_in(source: &str, env: &Environment, phase: Phase) -> (Store<()>, Val) {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = runtime(&mut store);
    let mut linker = Linker::new(&engine);
    linker
        .instance(&mut store, "suss.runtime", runtime)
        .unwrap();
    let instance = fragment(
        &mut store,
        &linker,
        portable::compile_in(source, env, phase).unwrap(),
    );
    let value = eval(&mut store, instance);
    store.gc(None).unwrap();
    (store, value)
}

#[test]
fn namespace_phases_resolve_and_execute_distinct_cells() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = runtime(&mut store);
    let mut env = Environment::new("app").unwrap();
    let mut linker = Linker::new(&engine);
    linker
        .instance(&mut store, "suss.runtime", runtime)
        .unwrap();
    for (phase, expected) in [(Phase::Runtime, 11.0), (Phase::Macro, 22.0)] {
        let id = env.declare_cell(phase, "dependency", "value").unwrap();
        env.alias(phase, "dep", "dependency").unwrap();
        env.refer(phase, "x", "dependency", "value").unwrap();
        let value = number(&mut store, runtime, expected);
        let cell = call(&mut store, runtime, "binding-new", &[value]);
        define(&mut store, &mut linker, &id, &cell);
        let instance = fragment(
            &mut store,
            &linker,
            portable::compile_in("(do dep/value x)", &env, phase).unwrap(),
        );
        let value = eval(&mut store, instance);
        assert_eq!(bits(&mut store, &value), expected.to_bits());
    }
    env.declare_cell(Phase::Macro, "macro.only", "secret")
        .unwrap();
    env.alias(Phase::Macro, "m", "macro.only").unwrap();
    let error = portable::analyze_in("m/secret", &env, Phase::Runtime).unwrap_err();
    assert_eq!(error.span, 0..8);
    assert!(error.message.contains("Runtime"));
    assert!(env.alias(Phase::Runtime, "m", "macro.only").is_err());
}

#[test]
fn namespace_global_reads_preserve_once_only_source_order_and_short_circuit() {
    let engine = support::engine();
    let mut store = Store::new(&engine, Vec::<u64>::new());
    let runtime = runtime(&mut store);
    let mut env = Environment::new("app").unwrap();
    let mut linker = Linker::new(&engine);
    linker
        .instance(&mut store, "suss.runtime", runtime)
        .unwrap();
    linker.allow_shadowing(true);
    let get = runtime.get_func(&mut store, "binding-get").unwrap();
    let ty = get.ty(&store);
    let wrapper = Func::new(&mut store, ty, move |mut caller, args, results| {
        get.call(&mut caller, args, results)?;
        let object = results[0]
            .unwrap_anyref()
            .unwrap()
            .as_struct(&caller)?
            .unwrap();
        let fields = object.fields(&mut caller)?.collect::<Vec<_>>();
        let [Val::F64(bits)] = fields.as_slice() else {
            panic!("not a number")
        };
        caller.data_mut().push(*bits);
        Ok(())
    });
    linker
        .define(&store, "suss.runtime", "binding-get", wrapper)
        .unwrap();
    for (name, expected) in [("condition", 0.0), ("left", 11.0), ("right", 22.0)] {
        let id = env.declare_cell(Phase::Runtime, "app", name).unwrap();
        let value = number(&mut store, runtime, expected);
        let cell = call(&mut store, runtime, "binding-new", &[value]);
        define(&mut store, &mut linker, &id, &cell);
    }
    let instance = fragment(
        &mut store,
        &linker,
        portable::compile_in(
            "(do (if condition left right) left right)",
            &env,
            Phase::Runtime,
        )
        .unwrap(),
    );
    let result = eval(&mut store, instance);
    store.gc(None).unwrap();
    assert_eq!(
        store.data(),
        &[
            0.0f64.to_bits(),
            11.0f64.to_bits(),
            11.0f64.to_bits(),
            22.0f64.to_bits()
        ]
    );
    assert_eq!(bits(&mut store, &result), 22.0f64.to_bits());
}

#[test]
fn namespace_cell_imports_reject_missing_or_incompatible_objects_before_eval() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = runtime(&mut store);
    let mut env = Environment::new("app").unwrap();
    let id = env.declare_cell(Phase::Runtime, "app", "value").unwrap();
    let bytes = portable::compile_in("value", &env, Phase::Runtime).unwrap();
    let module = Module::new(&engine, &bytes).unwrap();
    let mut linker = Linker::new(&engine);
    linker
        .instance(&mut store, "suss.runtime", runtime)
        .unwrap();
    assert!(linker.instantiate(&mut store, &module).is_err());
    let wrong = number(&mut store, runtime, 1.0);
    // Both are structs, but a numeric box is not the exact shared cell type.
    let global = cell_global(&mut store, &wrong);
    linker
        .define(&store, id.import_module(), &id.import_name(), global)
        .unwrap();
    assert!(linker.instantiate(&mut store, &module).is_err());
    let hir = portable::analyze_in("value", &env, Phase::Runtime).unwrap();
    let mut ir = portable::ir::lower(&hir).unwrap();
    ir.values[0].ty = portable::hir::Type::Number;
    assert!(
        portable::compile_ir(&ir)
            .unwrap_err()
            .message
            .contains("dynamic Value")
    );
    // Mutable bindings carry no stale scalar facts into unchecked arithmetic.
    let error = portable::compile_in("(+ value 1)", &env, Phase::Runtime).unwrap_err();
    assert_eq!(error.span, 3..8);
    assert!(error.message.contains("dynamic checking"));
}

#[test]
fn namespace_configuration_and_compile_failures_do_not_change_resolution() {
    let mut env = Environment::new("app").unwrap();
    env.declare_cell(Phase::Runtime, "one", "value").unwrap();
    env.declare_cell(Phase::Runtime, "two", "value").unwrap();
    env.alias(Phase::Runtime, "dep", "one").unwrap();
    env.refer(Phase::Runtime, "x", "one", "value").unwrap();
    assert!(env.declare_namespace(Phase::Runtime, "dep").is_err());
    assert!(env.declare_cell(Phase::Runtime, "dep", "value").is_err());
    for invalid in ["", "7", "nil", "a/b", ":keyword", "x y", "x;comment"] {
        assert!(env.declare_cell(Phase::Runtime, "one", invalid).is_err());
    }
    let before = env
        .resolve(Phase::Runtime, &Symbol::parse("dep/value"), 0..9)
        .unwrap();
    assert!(env.alias(Phase::Runtime, "dep", "two").is_err());
    assert!(env.refer(Phase::Runtime, "x", "two", "value").is_err());
    assert!(env.declare_cell(Phase::Runtime, "app", "x").is_err());
    assert!(env.alias(Phase::Runtime, "cljs.core", "two").is_err());
    assert!(env.declare_cell(Phase::Runtime, "../escape", "x").is_err());
    let error = portable::compile_in("(do dep/value unknown)", &env, Phase::Runtime).unwrap_err();
    assert_eq!(error.span, 14..21);
    assert_eq!(
        before,
        env.resolve(Phase::Runtime, &Symbol::parse("dep/value"), 0..9)
            .unwrap()
    );
    assert!(portable::compile_in("dep/value", &env, Phase::Runtime).is_ok());
}

#[test]
fn namespace_sources_reject_extension_and_root_ambiguity() {
    use portable::resolve::locate_source;
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    std::fs::create_dir(first.path().join("app")).unwrap();
    std::fs::create_dir(second.path().join("app")).unwrap();
    std::fs::create_dir(first.path().join("suss")).unwrap();
    let core = first.path().join("suss/core.sus");
    std::fs::write(&core, "(ns suss.core)").unwrap();
    for name in ["suss.core", "cljs.core"] {
        assert_eq!(
            locate_source(name, &[first.path()], 0..1).unwrap(),
            core.canonicalize().unwrap()
        );
    }
    let sus = first.path().join("app/hello_world.sus");
    std::fs::write(&sus, "(ns app.hello-world)").unwrap();
    assert_eq!(
        locate_source("app.hello-world", &[first.path(), first.path()], 2..8).unwrap(),
        sus.canonicalize().unwrap()
    );
    for extension in ["cljs", "cljc"] {
        let duplicate = first.path().join(format!("app/hello_world.{extension}"));
        std::fs::write(&duplicate, "(ns app.hello-world)").unwrap();
        let error = locate_source("app.hello-world", &[first.path()], 2..8).unwrap_err();
        assert_eq!(error.span, 2..8);
        assert!(error.message.contains("Ambiguous source"));
        std::fs::remove_file(duplicate).unwrap();
    }
    std::fs::write(
        second.path().join("app/hello_world.sus"),
        "(ns app.hello-world)",
    )
    .unwrap();
    assert!(
        locate_source("app.hello-world", &[first.path(), second.path()], 0..1)
            .unwrap_err()
            .message
            .contains("Ambiguous source")
    );
    assert!(locate_source("../escape", &[first.path()], 0..1).is_err());
    assert!(
        locate_source("app.missing", &[first.path()], 0..1)
            .unwrap_err()
            .message
            .contains("No source")
    );
}

fn catch_eval() -> Vec<u8> {
    use std::borrow::Cow;
    use wasm_encoder::*;
    let value = ValType::Ref(RefType::EQREF);
    let mut types = runtime_abi::prelude();
    types.ty().function([], [value]);
    types.ty().function([value], []);
    let mut imports = ImportSection::new();
    imports.import("fragment", "eval", EntityType::Function(10));
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
    exports.export("eval", ExportKind::Func, 1);
    let mut function = Function::new([]);
    function
        .instruction(&Instruction::Block(BlockType::Result(value)))
        .instruction(&Instruction::TryTable(
            BlockType::Result(value),
            Cow::Borrowed(&[Catch::One { tag: 0, label: 0 }]),
        ))
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
#[test]
fn namespace_unbound_cells_raise_language_errors_and_nil_is_a_binding() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = runtime(&mut store);
    let mut linker = Linker::new(&engine);
    linker
        .instance(&mut store, "suss.runtime", runtime)
        .unwrap();
    let nil_instance = fragment(&mut store, &linker, portable::compile("nil").unwrap());
    let nil = eval(&mut store, nil_instance);
    let cell = call(&mut store, runtime, "binding-unbound", &[]);
    let unbound_fields = cell
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap()
        .fields(&mut store)
        .unwrap()
        .collect::<Vec<_>>();
    assert_eq!(unbound_fields[1].i32(), Some(0));
    let mut env = Environment::new("app").unwrap();
    let id = env
        .declare_cell(Phase::Runtime, "app", "not-yet-defined")
        .unwrap();
    define(&mut store, &mut linker, &id, &cell);
    let instance = fragment(
        &mut store,
        &linker,
        portable::compile_in("not-yet-defined", &env, Phase::Runtime).unwrap(),
    );
    linker.instance(&mut store, "fragment", instance).unwrap();
    let catcher = fragment(&mut store, &linker, catch_eval());
    let error = eval(&mut store, catcher);
    store.gc(None).unwrap();
    let fields = error
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap()
        .fields(&mut store)
        .unwrap()
        .collect::<Vec<_>>();
    assert_eq!(fields.len(), 4);
    let descriptor = fields[0]
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap();
    assert_eq!(descriptor.field(&mut store, 0).unwrap().i64(), Some(2));
    for field in &fields[2..] {
        assert_eq!(
            field
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)
                .unwrap()
                .unwrap()
                .get_i32(),
            0
        );
    }
    let message = fields[1]
        .unwrap_anyref()
        .unwrap()
        .as_array(&store)
        .unwrap()
        .unwrap();
    assert!(matches!(
        message.ty(&store).unwrap().element_type(),
        wasmtime::StorageType::I16
    ));
    let units = message
        .elems(&mut store)
        .unwrap()
        .map(|unit| unit.unwrap_i32() as u16)
        .collect::<Vec<_>>();
    assert_eq!(String::from_utf16(&units).unwrap(), "Unbound binding");
    runtime
        .get_func(&mut store, "binding-set")
        .unwrap()
        .call(&mut store, &[cell, nil], &mut [])
        .unwrap();
    let value = eval(&mut store, instance);
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
fn namespace_scopes_persist_independently_when_reentered() {
    let mut env = Environment::new("app").unwrap();
    let value = env
        .declare_cell(Phase::Runtime, "dependency", "value")
        .unwrap();
    env.alias(Phase::Runtime, "dep", "dependency").unwrap();
    env.refer(Phase::Runtime, "x", "dependency", "value")
        .unwrap();
    env.exclude_core(Phase::Runtime, "+").unwrap();
    env.enter_namespace(Phase::Runtime, "other").unwrap();
    assert!(
        env.resolve(Phase::Runtime, &Symbol::new("x"), 0..1)
            .is_err()
    );
    assert!(portable::compile_in("(+ 1 2)", &env, Phase::Runtime).is_ok());
    env.enter_namespace(Phase::Runtime, "app").unwrap();
    assert_eq!(
        env.resolve(Phase::Runtime, &Symbol::parse("dep/value"), 0..9)
            .unwrap()
            .global(),
        &value
    );
    assert_eq!(
        env.resolve(Phase::Runtime, &Symbol::new("x"), 0..1)
            .unwrap()
            .global(),
        &value
    );
    assert!(portable::compile_in("(+ 1 2)", &env, Phase::Runtime).is_err());
    // A dormant scope also rejects declarations that would make its refer ambiguous.
    env.enter_namespace(Phase::Runtime, "other").unwrap();
    assert!(env.declare_cell(Phase::Runtime, "app", "x").is_err());
}

#[test]
fn namespace_core_aliases_import_one_canonical_live_cell() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = runtime(&mut store);
    let mut env = Environment::default();
    let id = env
        .declare_cell(Phase::Runtime, "cljs.core", "shared")
        .unwrap();
    assert_eq!(id.namespace(), "suss.core");
    let value = number(&mut store, runtime, 42.0);
    let cell = call(&mut store, runtime, "binding-new", &[value]);
    let mut linker = Linker::new(&engine);
    linker
        .instance(&mut store, "suss.runtime", runtime)
        .unwrap();
    define(&mut store, &mut linker, &id, &cell);
    let bytes = portable::compile_in(
        "(do cljs.core/shared suss.core/shared)",
        &env,
        Phase::Runtime,
    )
    .unwrap();
    let module = Module::new(&engine, &bytes).unwrap();
    let imports = module
        .imports()
        .filter(|import| matches!(import.ty(), wasmtime::ExternType::Global(_)))
        .map(|import| (import.module().to_owned(), import.name().to_owned()))
        .collect::<Vec<_>>();
    assert_eq!(
        imports,
        [("suss.bindings.runtime".into(), "suss.core/shared".into())]
    );
    let instance = fragment(&mut store, &linker, bytes);
    let value = eval(&mut store, instance);
    store.gc(None).unwrap();
    assert_eq!(bits(&mut store, &value), 42.0f64.to_bits());
}
