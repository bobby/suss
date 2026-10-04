//! Source definitions execute against exact shared live cells, never source replay.
mod support;
use suss_compile::{
    portable::{
        self,
        resolve::{Environment, Phase},
    },
    runtime_abi,
};
use wasmtime::{
    Global, GlobalType, Instance, Linker, Module, Mutability, RefType, Store, Val, ValType,
};

#[test]
fn source_definition_initializes_a_shared_cell() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let runtime = Instance::new(
        &mut store,
        &Module::new(&engine, runtime_abi::module()).unwrap(),
        &[],
    )
    .unwrap();
    let mut env = Environment::new("app").unwrap();
    let identity = env.declare_cell(Phase::Runtime, "app", "value").unwrap();
    let mut result = [Val::null_any_ref()];
    runtime
        .get_func(&mut store, "binding-unbound")
        .unwrap()
        .call(&mut store, &[], &mut result)
        .unwrap();
    let cell = result[0].clone();
    let ty = cell
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap()
        .ty(&store)
        .unwrap();
    let global = Global::new(
        &mut store,
        GlobalType::new(
            ValType::Ref(RefType::new(false, ty.into())),
            Mutability::Const,
        ),
        cell,
    )
    .unwrap();
    let mut linker = Linker::new(&engine);
    linker
        .instance(&mut store, "suss.runtime", runtime)
        .unwrap();
    linker
        .define(
            &store,
            identity.import_module(),
            &identity.import_name(),
            global,
        )
        .unwrap();
    let bytes = portable::compile_in("(def value 7) value", &env, Phase::Runtime).unwrap();
    runtime_abi::verify_artifact(&bytes, &runtime_abi::Manifest::default()).unwrap();
    let fragment = linker
        .instantiate(&mut store, &Module::new(&engine, bytes).unwrap())
        .unwrap();
    fragment
        .get_func(&mut store, "eval")
        .unwrap()
        .call(&mut store, &[], &mut result)
        .unwrap();
    store.gc(None).unwrap();
    let number = result[0]
        .unwrap_anyref()
        .unwrap()
        .as_struct(&store)
        .unwrap()
        .unwrap();
    assert_eq!(
        number.field(&mut store, 0).unwrap().unwrap_f64().to_bits(),
        7.0f64.to_bits()
    );
}

struct Session {
    store: Store<()>,
    runtime: Instance,
    linker: Linker<()>,
    environment: Environment,
    cells: std::collections::BTreeMap<portable::resolve::Global, Global>,
}
impl Session {
    fn new() -> Self {
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
        Self {
            store,
            runtime,
            linker,
            environment: Environment::default(),
            cells: Default::default(),
        }
    }
    fn compile(&mut self, source: &str, phase: Phase) -> Result<Instance, portable::Diagnostic> {
        let prepared = portable::prepare_fragment(source, &self.environment, phase)?;
        runtime_abi::verify_artifact(&prepared.wasm, &runtime_abi::Manifest::default()).unwrap();
        let module = Module::new(self.store.engine(), &prepared.wasm).unwrap();
        for identity in prepared.cells {
            if !self.cells.contains_key(&identity) {
                let mut result = [Val::null_any_ref()];
                self.runtime
                    .get_func(&mut self.store, "binding-unbound")
                    .unwrap()
                    .call(&mut self.store, &[], &mut result)
                    .unwrap();
                let ty = result[0]
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&self.store)
                    .unwrap()
                    .unwrap()
                    .ty(&self.store)
                    .unwrap();
                let global = Global::new(
                    &mut self.store,
                    GlobalType::new(
                        ValType::Ref(RefType::new(false, ty.into())),
                        Mutability::Const,
                    ),
                    result[0].clone(),
                )
                .unwrap();
                self.linker
                    .define(
                        &self.store,
                        identity.import_module(),
                        &identity.import_name(),
                        global,
                    )
                    .unwrap();
                self.cells.insert(identity, global);
            }
        }
        let instance = self.linker.instantiate(&mut self.store, &module).unwrap();
        self.environment = prepared.environment;
        Ok(instance)
    }
    fn call(&mut self, fragment: Instance) -> Result<Val, wasmtime::Error> {
        let mut result = [Val::null_any_ref()];
        fragment.get_func(&mut self.store, "eval").unwrap().call(
            &mut self.store,
            &[],
            &mut result,
        )?;
        self.store.gc(None).unwrap();
        Ok(result[0].clone())
    }
    fn catch(&mut self, fragment: Instance, descriptor: i64, message: &str) {
        let mut linker = Linker::new(self.store.engine());
        linker
            .instance(&mut self.store, "suss.runtime", self.runtime)
            .unwrap();
        linker
            .instance(&mut self.store, "target", fragment)
            .unwrap();
        let module = Module::new(self.store.engine(), catcher(0, "target", "eval")).unwrap();
        let instance = linker.instantiate(&mut self.store, &module).unwrap();
        let mut result = [Val::null_any_ref()];
        instance
            .get_func(&mut self.store, "call")
            .unwrap()
            .call(&mut self.store, &[], &mut result)
            .unwrap();
        self.store.gc(None).unwrap();
        let fields = result[0]
            .unwrap_anyref()
            .unwrap()
            .as_struct(&self.store)
            .unwrap()
            .unwrap()
            .fields(&mut self.store)
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(fields.len(), 5);
        let kind = fields[0]
            .unwrap_anyref()
            .unwrap()
            .as_struct(&self.store)
            .unwrap()
            .unwrap();
        assert_eq!(
            kind.field(&mut self.store, 0).unwrap().i64(),
            Some(descriptor)
        );
        let string = fields[1]
            .unwrap_anyref()
            .unwrap()
            .as_array(&self.store)
            .unwrap()
            .unwrap();
        assert!(matches!(
            string.ty(&self.store).unwrap().element_type(),
            wasmtime::StorageType::I16
        ));
        let units = string
            .elems(&mut self.store)
            .unwrap()
            .map(|value| value.unwrap_i32() as u16)
            .collect::<Vec<_>>();
        assert_eq!(String::from_utf16(&units).unwrap(), message);
        // Fresh errors have nil data, cause and ABI2 identity UID.
        for nil in &fields[2..] {
            assert_eq!(self.sentinel(nil), 0);
        }
    }
    fn eval(&mut self, source: &str) -> Val {
        let fragment = self.compile(source, Phase::Runtime).unwrap();
        self.call(fragment).unwrap()
    }
    fn bits(&mut self, value: &Val) -> u64 {
        let number = value
            .unwrap_anyref()
            .unwrap()
            .as_struct(&self.store)
            .unwrap()
            .unwrap();
        number
            .field(&mut self.store, 0)
            .unwrap()
            .unwrap_f64()
            .to_bits()
    }
    fn sentinel(&self, value: &Val) -> u32 {
        value
            .unwrap_anyref()
            .unwrap()
            .as_i31(&self.store)
            .unwrap()
            .unwrap()
            .get_u32()
    }
}

#[test]
fn source_definitions_rebind_live_globals_without_changing_old_captures() {
    let mut session = Session::new();
    let value = session.eval("(ns app) (def value 7) (def function (fn [] value)) (def captured (let [old value] (fn [] old))) (def call (fn [] (function))) (call)");
    assert_eq!(session.bits(&value), 7.0f64.to_bits());
    let old_code = session.compile("(function)", Phase::Runtime).unwrap();
    let old_function = session.eval("(def original function)");
    assert!(old_function
        .unwrap_anyref()
        .unwrap()
        .is_struct(&session.store)
        .unwrap());
    session.eval("(def value 9) (def function (fn [] 11))");
    for (source, expected) in [
        ("(function)", 11.0f64),
        ("(captured)", 7.0),
        ("(original)", 9.0),
        ("(call)", 11.0),
    ] {
        let value = session.eval(source);
        assert_eq!(session.bits(&value), expected.to_bits(), "{source}");
    }
    let value = session.call(old_code).unwrap();
    assert_eq!(session.bits(&value), 11.0f64.to_bits());
}

#[test]
fn source_defonce_checks_bound_state_and_skips_initializer_effects() {
    let mut session = Session::new();
    for (name, initializer, sentinel) in [("nil-value", "nil", 0), ("false-value", "false", 2)] {
        let value = session.eval(&format!("(defonce {name} {initializer})"));
        assert_eq!(session.sentinel(&value), sentinel);
        let value = session.eval(&format!("(defonce {name} (def forbidden 99))"));
        assert_eq!(session.sentinel(&value), 0);
        let value = session.eval(name);
        assert_eq!(session.sentinel(&value), sentinel);
        let value = session.eval("forbidden");
        assert_eq!(session.sentinel(&value), 6);
        // Reading undefined must not mark the cell initialized.
    }
    let value = session.eval("(defonce forbidden 17) forbidden");
    assert_eq!(session.bits(&value), 17.0f64.to_bits());
    let value = session.eval("(def declaration) (defonce declaration 5) declaration");
    assert_eq!(session.bits(&value), 5.0f64.to_bits());
    let value = session.eval("(defonce declaration 8)");
    assert_eq!(session.sentinel(&value), 0);
    let value = session.eval("declaration");
    assert_eq!(session.bits(&value), 5.0f64.to_bits());
}

#[test]
fn failed_compile_and_initializer_preserve_existing_cells_and_prior_effects() {
    let mut session = Session::new();
    session.eval("(def value 7)");
    let count = session.cells.len();
    let error = session
        .compile("(ns failed) (def value unresolved)", Phase::Runtime)
        .unwrap_err();
    assert!(error.message.contains("Unresolved"));
    assert_eq!(
        session.environment.current_namespace(Phase::Runtime),
        "user"
    );
    assert!(!session.environment.has_namespace(Phase::Runtime, "failed"));
    assert_eq!(session.cells.len(), count);
    let fragment = session
        .compile("(def value (do (def effect 3) (1)))", Phase::Runtime)
        .unwrap();
    session.catch(fragment, 3, "Not callable");
    let value = session.eval("value");
    assert_eq!(session.bits(&value), 7.0f64.to_bits());
    let value = session.eval("effect");
    assert_eq!(session.bits(&value), 3.0f64.to_bits());
    let value = session.eval("(def value 12) value");
    assert_eq!(session.bits(&value), 12.0f64.to_bits());
}

#[test]
fn source_namespaces_resolve_requires_renames_core_aliases_and_exclusions() {
    let mut session = Session::new();
    session.eval("(ns library) (def value -0.0) (def choose (fn [value] value))");
    let value = session.eval("(ns app \"docs\" {:retained true} (:require [library :as lib :refer [value choose] :rename {value renamed}] [cljs.core :as core]) (:refer-clojure :exclude [+] :rename {* product})) (def own (lib/choose renamed)) own");
    assert_eq!(session.bits(&value), (-0.0f64).to_bits());
    for (source, expected) in [
        ("(core/+ 1 2)", 3.0f64),
        ("(product 2 3)", 6.0),
        ("(choose 9)", 9.0),
    ] {
        let value = session.eval(source);
        assert_eq!(session.bits(&value), expected.to_bits());
    }
    assert!(session.compile("(+ 1 2)", Phase::Runtime).is_err());
    session.eval("(ns library) (def value 10)");
    let value = session
        .eval("(ns app (:require [library :refer [value] :rename {value renamed}])) renamed");
    assert_eq!(session.bits(&value), 10.0f64.to_bits());
}

#[test]
fn definitions_inside_functions_capture_lexicals_and_preserve_name_metadata() {
    let mut session = Session::new();
    let value = session.eval("(let [local 3] ((fn [] (def captured-value local)))) captured-value");
    assert_eq!(session.bits(&value), 3.0f64.to_bits());
    let hir = portable::analyze("(def ^:retained value \"doc\" 7)").unwrap();
    let portable::hir::Expression::Do(body) = hir.kind else {
        panic!()
    };
    let portable::hir::Expression::Definition {
        name_metadata,
        name_span,
        docstring,
        ..
    } = &body[0].kind
    else {
        panic!()
    };
    assert_eq!(name_metadata.len(), 1);
    assert_eq!(name_span, &(5..21));
    assert_eq!(docstring.as_deref(), Some(&[100u16, 111, 99][..]));
    let prepared = portable::prepare_fragment(
        "(ns ^:retained app \"doc\") (def value 1)",
        &Environment::default(),
        Phase::Runtime,
    )
    .unwrap();
    let directive = prepared.namespace_directive.unwrap();
    let suss_reader::forms::Kind::List(items) = directive.kind else {
        panic!()
    };
    assert_eq!(items[1].metadata.len(), 1);
}

#[test]
fn definition_and_namespace_diagnostics_are_located_and_transactional() {
    let environment = Environment::default();
    for source in [
        "(def)",
        "(def 1 2)",
        "(def foreign/value 3)",
        "(def value 1 2)",
        "(defonce value)",
        "(def ^:const value 1)",
        "(ns app (:require missing))",
        "(ns app (:require [cljs.core :as a :as b]))",
        "(ns app (:require [cljs.core :rename {+ sum}]))",
        "(ns app (:require-macros [cljs.core :as core]))",
        "(ns app (:refer-clojure :exclude [] ) (:refer-clojure :exclude []))",
    ] {
        let error = match portable::prepare_fragment(source, &environment, Phase::Runtime) {
            Err(error) => error,
            Ok(_) => panic!("compiled unsupported {source}"),
        };
        assert!(error.span.start < error.span.end, "{source}: {error}");
        assert!(error.span.end <= source.len());
        assert_eq!(environment.current_namespace(Phase::Runtime), "user");
        assert!(environment.cells().is_empty());
    }
    let mut session = Session::new();
    let value = session.eval("(def user/value 7) (def value) value");
    assert_eq!(session.bits(&value), 7.0f64.to_bits());
    let value = session.eval("(let [defonce (fn [value init] value)] (defonce 3 4))");
    assert_eq!(session.bits(&value), 3.0f64.to_bits());
    let value =
        session.eval("(ns cljs.core) (def cljs.core/portable-value 7) suss.core/portable-value");
    assert_eq!(session.bits(&value), 7.0f64.to_bits());
}

#[test]
fn definition_cells_remain_separate_by_phase() {
    let mut session = Session::new();
    let runtime = session
        .compile("(ns app) (def value 7) value", Phase::Runtime)
        .unwrap();
    let macro_phase = session
        .compile("(ns app) (def value 9) value", Phase::Macro)
        .unwrap();
    let a = session.call(runtime).unwrap();
    let b = session.call(macro_phase).unwrap();
    assert_eq!(session.bits(&a), 7.0f64.to_bits());
    assert_eq!(session.bits(&b), 9.0f64.to_bits());
    assert_eq!(session.cells.len(), 2);
}

#[test]
fn verifier_rejects_malformed_global_writes_and_bound_checks() {
    use portable::{
        hir::{Literal, Type},
        ir::{Block, Function, Instruction, Operation, Terminator, Value, ValueId},
    };
    let mut env = Environment::default();
    let global = env.declare_cell(Phase::Runtime, "user", "value").unwrap();
    let make = |operation, ty| Function {
        span: 0..1,
        values: vec![
            Value {
                ty: Type::Number,
                span: 0..1,
            },
            Value { ty, span: 0..1 },
        ],
        blocks: vec![Block {
            parameters: vec![],
            instructions: vec![
                Instruction {
                    result: ValueId(0),
                    operation: Operation::Literal(Literal::Number(7.0)),
                    span: 0..1,
                },
                Instruction {
                    result: ValueId(1),
                    operation,
                    span: 0..1,
                },
            ],
            terminator: Terminator::Return(ValueId(1)),
        }],
    };
    for ir in [
        make(
            Operation::GlobalWrite {
                global: global.clone(),
                value: ValueId(0),
            },
            Type::Number,
        ),
        make(Operation::GlobalBound(global.clone()), Type::Value),
        make(
            Operation::GlobalWrite {
                global,
                value: ValueId(2),
            },
            Type::Value,
        ),
    ] {
        assert!(portable::compile_ir(&ir).is_err());
    }
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

#[test]
fn namespace_declarations_replace_import_scopes_but_keep_existing_cells() {
    let mut session = Session::new();
    session.eval(
        "(ns app (:require [cljs.core :as core]) (:refer-clojure :exclude [+])) (def value 7)",
    );
    let fragment = session.compile("(ns app) (+ 1 2)", Phase::Runtime).unwrap();
    let value = session.call(fragment).unwrap();
    assert_eq!(session.bits(&value), 3.0f64.to_bits());
    assert!(session.compile("(core/+ 1 2)", Phase::Runtime).is_err());
    let value = session.eval("value");
    assert_eq!(session.bits(&value), 7.0f64.to_bits());
}

#[test]
fn declaration_expression_contexts_are_explicitly_unsupported() {
    let environment = Environment::default();
    for source in [
        "((fn [x] x) (def value))",
        "(def other (def value))",
        "(let [x (def value)] x)",
        "(fn [] (def value))",
        "(if (def value) 1 2)",
        "((fn [x] x) (do 1 (def value)))",
        "((fn [x] x) (let [] (def value)))",
        "((fn [x] x) (if true (def value) nil))",
    ] {
        let error = match portable::prepare_fragment(source, &environment, Phase::Runtime) {
            Err(error) => error,
            Ok(_) => panic!("compiled uncertified declaration expression: {source}"),
        };
        assert!(error.message.contains("Initializerless def"), "{error}");
        assert!(error.span.start < error.span.end);
        assert!(environment.cells().is_empty());
    }
    let mut session = Session::new();
    let value = session.eval("(def value 7) (def value)");
    assert_eq!(session.sentinel(&value), 0);
    let value = session.eval("((fn [] (def value) value))");
    assert_eq!(session.bits(&value), 7.0f64.to_bits());
    let value = session.eval("(if true (def value) nil)");
    assert_eq!(session.sentinel(&value), 0);
}

#[test]
fn source_require_reload_metadata_is_explicitly_unsupported() {
    let environment = Environment::default();
    for source in [
        "(ns app (:require ^:reload [cljs.core :as core]))",
        "(ns app (:require ^{:reload :reload-all} [cljs.core :as core]))",
        "(ns app (:require ^{:reload false} [cljs.core :as core]))",
        "(ns app (:require ^{:reload true :reload false} [cljs.core :as core]))",
    ] {
        let error = match portable::prepare_fragment(source, &environment, Phase::Runtime) {
            Err(error) => error,
            Ok(_) => panic!("compiled unsupported reload metadata: {source}"),
        };
        assert!(error.message.contains("reload"), "{error}");
        assert!(error.span.start < error.span.end);
        assert!(!environment.has_namespace(Phase::Runtime, "app"));
    }
    let mut session = Session::new();
    let value = session.eval("(ns app (:require ^:retained [cljs.core :as core])) (core/+ 1 2)");
    assert_eq!(session.bits(&value), 3.0f64.to_bits());
}

#[test]
fn reviewed_declaration_preserves_source_metadata_and_phase_identity() {
    use suss_reader::forms::Kind;
    for phase in [Phase::Runtime, Phase::Macro] {
        let environment = Environment::new("review.declaration").unwrap();
        let source = "(declare ^:retained first second first)";
        let hir = portable::analyze_in(source, &environment, phase).unwrap();
        let portable::hir::Expression::Do(body) = hir.kind else {
            panic!()
        };
        let portable::hir::Expression::Do(definitions) = &body[0].kind else {
            panic!()
        };
        assert_eq!(definitions.len(), 3);
        for (index, expected_name) in ["first", "second", "first"].into_iter().enumerate() {
            let portable::hir::Expression::Definition {
                global,
                name_metadata,
                name_span,
                initializer,
                ..
            } = &definitions[index].kind
            else {
                panic!()
            };
            assert_eq!(global.phase(), phase);
            assert_eq!(global.namespace(), "review.declaration");
            assert_eq!(global.name(), expected_name);
            assert!(initializer.is_none());
            assert!(source[name_span.clone()].ends_with(expected_name));
            assert_eq!(name_metadata.len(), if index == 0 { 2 } else { 1 });
            if index == 0 {
                assert!(name_metadata.iter().any(|metadata|
                    matches!(&metadata.kind, Kind::Keyword(key) if key.name == "retained")));
            }
            // Prefix storage order is not the declaration contract. Assert the
            // merged reader data, including both generated and original keys.
            let declaration = suss_reader::forms::Form {
                span: name_span.clone(),
                metadata: name_metadata.clone(),
                kind: Kind::Symbol(suss_reader::Symbol::new(expected_name)),
            };
            let merged = portable::hir::reader_metadata_pairs(&declaration).unwrap();
            assert!(merged.chunks_exact(2).any(|pair|
                matches!(&pair[0].kind, Kind::Keyword(key) if key.name == "declared" && key.namespace.is_none())
                && matches!(pair[1].kind, Kind::Bool(true))));
            if index == 0 {
                assert!(merged.chunks_exact(2).any(|pair|
                    matches!(&pair[0].kind, Kind::Keyword(key) if key.name == "retained" && key.namespace.is_none())
                    && matches!(pair[1].kind, Kind::Bool(true))));
            }
        }
        // Analysis uses a snapshot even on successful declaration expansion.
        assert!(environment
            .resolve(phase, &suss_reader::Symbol::new("first"), 0..1)
            .is_err());
    }
}
