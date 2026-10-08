//! Execute compiled resumes, cancellation and rooted GC queues.
//! These focused tests do not certify the complete async lifecycle.
mod support;
use std::collections::BTreeMap;
use suss_compile::{
    portable::{
        self,
        resolve::{Environment, Phase},
    },
    runtime_abi,
};
use wasm_encoder as w;
use wasmtime::{
    AsContextMut, Engine, Global, GlobalType, Instance, Linker, Module, Mutability, RefType,
    RootScope, Rooted, Store, StructRef, StructRefPre, Val, ValType,
};

const WRAPPERS: &[(&str, usize)] = &[
    ("producer", 1),
    ("event", 1),
    ("read-slot", 1),
    ("resolve", 2),
    ("reject", 2),
    ("emit", 1),
    ("observe", 1),
    ("run", 0),
    ("event-kind", 1),
    ("complete", 3),
    ("trap", 0),
    ("cancel", 1),
    ("cancel-marker", 1),
    ("cancel-all", 0),
];

// Test-only compiled adapters turn typed runtime functions into universal GC
// closures. Resume bodies themselves are compiled by the real portable pipeline.
fn adapters() -> Vec<u8> {
    use w::Instruction::*;
    let value = w::ValType::Ref(w::RefType::EQREF);
    let args = w::ValType::Ref(w::RefType {
        nullable: false,
        heap_type: w::HeapType::Concrete(2),
    });
    let invoke = w::ValType::Ref(w::RefType {
        nullable: false,
        heap_type: w::HeapType::Concrete(3),
    });
    let mut types = runtime_abi::prelude();
    let signatures = [
        (
            vec![value, invoke, w::ValType::I32, w::ValType::I32],
            vec![value],
        ),
        (vec![w::ValType::F64], vec![value]),
        (vec![value, value], vec![w::ValType::I32]),
        (vec![value], vec![value]),
        (vec![value, value], vec![value]),
        (vec![], vec![w::ValType::I32]),
        (vec![], vec![value]),
        (vec![value, w::ValType::I32, value], vec![w::ValType::I32]),
        (vec![value], vec![w::ValType::I32]),
    ];
    for (params, results) in signatures {
        types.ty().function(params, results);
    }
    let mut imports = w::ImportSection::new();
    for (name, ty) in [
        ("closure-new", 10),
        ("number-box", 11),
        ("future-resolve", 12),
        ("future-reject", 12),
        ("async-continuation-snapshot", 13),
        ("async-array-append", 14),
        ("async-scheduler-run-one", 15),
        ("async-task-complete", 17),
        ("async-task-cancel", 18),
        ("async-cancelled-dependency-is", 18),
        ("async-scheduler-cancel-all", 15),
    ] {
        imports.import("suss.runtime", name, w::EntityType::Function(ty));
    }
    let imported = 11;
    let mut functions = w::FunctionSection::new();
    let mut code = w::CodeSection::new();
    let mut exports = w::ExportSection::new();
    let mut refs = Vec::new();
    for (index, (name, arity)) in WRAPPERS.iter().enumerate() {
        let mut body = vec![];
        let operand = |n| vec![LocalGet(1), I32Const(n), ArrayGet(2)];
        match *name {
            "producer" | "event" | "read-slot" | "event-kind" => {
                body.extend(operand(0));
                body.extend([
                    Call(4),
                    RefCastNonNull(w::HeapType::Concrete(2)),
                    I32Const(if *name == "producer" {
                        4
                    } else if *name == "event" {
                        7
                    } else if *name == "event-kind" {
                        6
                    } else {
                        1
                    }),
                    ArrayGet(2),
                ]);
                if *name == "read-slot" {
                    body.extend([
                        RefCastNonNull(w::HeapType::Concrete(2)),
                        I32Const(0),
                        ArrayGet(2),
                    ]);
                }
                if *name == "event-kind" {
                    body.extend([
                        RefCastNonNull(w::HeapType::I31),
                        I31GetU,
                        F64ConvertI32U,
                        Call(1),
                    ]);
                }
            }
            "resolve" | "reject" => {
                body.extend(operand(0));
                body.extend(operand(1));
                body.extend([
                    Call(if *name == "resolve" { 2 } else { 3 }),
                    Drop,
                    I32Const(0),
                    RefI31,
                ]);
            }
            "emit" => {
                body.push(GlobalGet(0));
                body.extend(operand(0));
                body.extend([Call(5), GlobalSet(0), I32Const(0), RefI31]);
            }
            "observe" => {
                body.extend(operand(0));
                body.extend([GlobalSet(1), I32Const(0), RefI31]);
            }
            "run" => body.extend([Call(6), F64ConvertI32S, Call(1)]),
            "cancel-all" => body.extend([Call(10), F64ConvertI32U, Call(1)]),
            "trap" => body.push(Unreachable),
            "cancel" | "cancel-marker" => {
                body.extend(operand(0));
                body.extend([
                    Call(if *name == "cancel" { 8 } else { 9 }),
                    F64ConvertI32U,
                    Call(1),
                ]);
            }
            "complete" => {
                body.extend(operand(0));
                body.extend(operand(1));
                body.extend([
                    RefCastNonNull(w::HeapType::Concrete(0)),
                    StructGet {
                        struct_type_index: 0,
                        field_index: 0,
                    },
                    I32TruncF64S,
                ]);
                body.extend(operand(2));
                body.extend([Call(7), Drop, I32Const(0), RefI31]);
            }
            _ => unreachable!(),
        }
        functions.function(3);
        let resume_index = imported + index as u32 * 2;
        refs.push(resume_index);
        let mut function = w::Function::new([]);
        for instruction in body {
            function.instruction(&instruction);
        }
        function.instruction(&End);
        code.function(&function);
        functions.function(16);
        let mut factory = w::Function::new([]);
        for instruction in [
            I32Const(0),
            RefI31,
            RefFunc(resume_index),
            I32Const(*arity as i32),
            I32Const(*arity as i32),
            Call(0),
            End,
        ] {
            factory.instruction(&instruction);
        }
        code.function(&factory);
        exports.export(
            &format!("make-{name}"),
            w::ExportKind::Func,
            resume_index + 1,
        );
    }
    let mut globals = w::GlobalSection::new();
    globals.global(
        w::GlobalType {
            val_type: value,
            mutable: true,
            shared: false,
        },
        &w::ConstExpr::extended([I32Const(0), ArrayNewDefault(2)]),
    );
    globals.global(
        w::GlobalType {
            val_type: value,
            mutable: true,
            shared: false,
        },
        &w::ConstExpr::extended([I32Const(0), RefI31]),
    );
    exports.export("trace", w::ExportKind::Global, 0);
    exports.export("observed", w::ExportKind::Global, 1);
    let mut elements = w::ElementSection::new();
    elements.declared(w::Elements::Functions(std::borrow::Cow::Owned(refs)));
    let mut module = w::Module::new();
    module
        .section(&types)
        .section(&imports)
        .section(&functions)
        .section(&globals)
        .section(&exports)
        .section(&elements)
        .section(&code);
    let _ = args; // The common invocation type is supplied by the ABI prelude.
    module.finish()
}

struct Harness {
    store: Store<()>,
    runtime: Instance,
    adapter: Instance,
    linker: Linker<()>,
    environment: Environment,
    cells: BTreeMap<String, Global>,
}
impl Harness {
    fn new() -> Self {
        Self::with_engine(&support::engine(), false)
    }
    fn with_engine(engine: &Engine, fuel: bool) -> Self {
        let mut store = Store::new(engine, ());
        if fuel {
            store.set_fuel(1_000_000).unwrap();
        }
        let bytes = runtime_abi::module();
        runtime_abi::verify_artifact(&bytes, &runtime_abi::Manifest::default()).unwrap();
        let runtime = Instance::new(&mut store, &Module::new(engine, bytes).unwrap(), &[]).unwrap();
        let mut linker = Linker::new(engine);
        linker
            .instance(&mut store, "suss.runtime", runtime)
            .unwrap();
        let adapter = linker
            .instantiate(&mut store, &Module::new(engine, adapters()).unwrap())
            .unwrap();
        let mut environment = Environment::default();
        let mut cells = BTreeMap::new();
        for (name, _) in WRAPPERS {
            let identity = environment.declare_cell(Phase::Runtime, "t", name).unwrap();
            let mut function = [Val::null_any_ref()];
            adapter
                .get_func(&mut store, &format!("make-{name}"))
                .unwrap()
                .call(&mut store, &[], &mut function)
                .unwrap();
            let mut cell = [Val::null_any_ref()];
            runtime
                .get_func(&mut store, "binding-new")
                .unwrap()
                .call(&mut store, &function, &mut cell)
                .unwrap();
            let ty = cell[0]
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
                cell[0].clone(),
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
            cells.insert(format!("t/{name}"), global);
        }
        Self {
            store,
            runtime,
            adapter,
            linker,
            environment,
            cells,
        }
    }
    fn value(&mut self, name: &str, args: &[Val]) -> Val {
        let mut result = [Val::null_any_ref()];
        self.runtime
            .get_func(&mut self.store, name)
            .unwrap()
            .call(&mut self.store, args, &mut result)
            .unwrap();
        result[0].clone()
    }
    fn integer(&mut self, name: &str, args: &[Val]) -> i32 {
        let mut result = [Val::I32(-1)];
        self.runtime
            .get_func(&mut self.store, name)
            .unwrap()
            .call(&mut self.store, args, &mut result)
            .unwrap();
        result[0].unwrap_i32()
    }
    fn counts(&mut self) -> (i32, i32) {
        let mut result = [Val::I32(-1), Val::I32(-1)];
        self.runtime
            .get_func(&mut self.store, "async-scheduler-counts")
            .unwrap()
            .call(&mut self.store, &[], &mut result)
            .unwrap();
        (result[0].unwrap_i32(), result[1].unwrap_i32())
    }
    fn scalar(&mut self, value: u32) -> Val {
        Val::AnyRef(Some(wasmtime::AnyRef::from_i31(
            &mut self.store,
            wasmtime::I31::new_u32(value).unwrap(),
        )))
    }
    fn number(&mut self, n: f64) -> Val {
        self.value("number-box", &[Val::F64(n.to_bits())])
    }
    fn decode(&mut self, value: &Val) -> f64 {
        value
            .unwrap_anyref()
            .unwrap()
            .as_struct(&self.store)
            .unwrap()
            .unwrap()
            .field(&mut self.store, 0)
            .unwrap()
            .unwrap_f64()
    }
    fn module(&self, source: &str) -> Module {
        let prepared =
            portable::prepare_fragment(source, &self.environment, Phase::Runtime).unwrap();
        Module::new(self.store.engine(), prepared.wasm).unwrap()
    }
    fn closure(&mut self, source: &str) -> Val {
        let module = self.module(source);
        let instance = self.linker.instantiate(&mut self.store, &module).unwrap();
        let mut value = [Val::null_any_ref()];
        instance
            .get_func(&mut self.store, "eval")
            .unwrap()
            .call(&mut self.store, &[], &mut value)
            .unwrap();
        value[0].clone()
    }
    fn install(&mut self, source: &str) {
        self.source(source);
    }
    fn source(&mut self, source: &str) -> Val {
        let prepared =
            portable::prepare_fragment(source, &self.environment, Phase::Runtime).unwrap();
        for identity in &prepared.cells {
            if self.cells.contains_key(&identity.import_name()) {
                continue;
            }
            let cell = self.value("binding-unbound", &[]);
            let ty = cell
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
                cell,
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
            self.cells.insert(identity.import_name(), global);
        }
        let module = Module::new(self.store.engine(), prepared.wasm).unwrap();
        let instance = self.linker.instantiate(&mut self.store, &module).unwrap();
        let mut result = [Val::null_any_ref()];
        instance
            .get_func(&mut self.store, "eval")
            .unwrap()
            .call(&mut self.store, &[], &mut result)
            .unwrap();
        self.environment = prepared.environment;
        result[0].clone()
    }
    fn bind(&mut self, name: &str, value: Val) {
        self.install(&format!("(def {name} nil)"));
        let cell = self.cells[&format!("user/{name}")].get(&mut self.store);
        self.runtime
            .get_func(&mut self.store, "binding-set")
            .unwrap()
            .call(&mut self.store, &[cell, value], &mut [])
            .unwrap();
    }
    fn dynamic_push(&mut self, n: f64) -> Val {
        let cell = self.cells["user/*x*"].get(&mut self.store);
        let old = self.value("binding-get", &[cell.clone()]);
        let current = self.number(n);
        let entries = self.value("args-new", &[Val::I32(3)]);
        let array = entries
            .unwrap_anyref()
            .unwrap()
            .as_array(&self.store)
            .unwrap()
            .unwrap();
        for (index, value) in [cell, old, current].into_iter().enumerate() {
            array.set(&mut self.store, index as u32, value).unwrap();
        }
        self.value("dynamic-push", &[entries])
    }
    fn state(&mut self, producer: Val, slots: Val, frame: Val, generation: u32) -> Val {
        let state = self.value("args-new", &[Val::I32(8)]);
        let pc = self.scalar(0);
        let unwind = self.value("args-new", &[Val::I32(0)]);
        let generation = self.scalar(generation);
        let event = self.scalar(0);
        let nil = self.value("nil", &[]);
        let array = state
            .unwrap_anyref()
            .unwrap()
            .as_array(&self.store)
            .unwrap()
            .unwrap();
        for (index, value) in [pc, slots, unwind, frame, producer, generation, event, nil]
            .into_iter()
            .enumerate()
        {
            array.set(&mut self.store, index as u32, value).unwrap();
        }
        state
    }
    fn continuation(&mut self, producer: Val, frame: Val, generation: u32) -> Val {
        let slots = self.value("args-new", &[Val::I32(0)]);
        let state = self.state(producer, slots, frame, generation);
        self.value("async-continuation-new", &[state])
    }
    fn snapshot_slot(&mut self, continuation: &Val, index: u32) -> Val {
        let snapshot = self.value("async-continuation-snapshot", &[continuation.clone()]);
        snapshot
            .unwrap_anyref()
            .unwrap()
            .as_array(&self.store)
            .unwrap()
            .unwrap()
            .get(&mut self.store, index)
            .unwrap()
    }
    fn advance(&mut self, continuation: &Val, generation: u32) {
        let state = self.value("async-continuation-snapshot", &[continuation.clone()]);
        let scalar = self.scalar(generation);
        state
            .unwrap_anyref()
            .unwrap()
            .as_array(&self.store)
            .unwrap()
            .unwrap()
            .set(&mut self.store, 5, scalar)
            .unwrap();
        self.value("async-continuation-commit", &[continuation.clone(), state]);
    }
    fn trace(&mut self) -> Vec<f64> {
        let value = self
            .adapter
            .get_global(&mut self.store, "trace")
            .unwrap()
            .get(&mut self.store);
        let values: Vec<_> = value
            .unwrap_anyref()
            .unwrap()
            .as_array(&self.store)
            .unwrap()
            .unwrap()
            .elems(&mut self.store)
            .unwrap()
            .collect();
        values.iter().map(|value| self.decode(value)).collect()
    }
    fn rejected(&mut self, name: &str, args: &[Val]) {
        let error = self
            .runtime
            .get_func(&mut self.store, name)
            .unwrap()
            .call(&mut self.store, args, &mut [Val::null_any_ref()])
            .unwrap_err();
        assert!(
            error.is::<wasmtime::ThrownException>() && !error.is::<wasmtime::Trap>(),
            "{error:#}"
        );
        let exception = self.store.take_pending_exception().unwrap();
        let expected = self
            .runtime
            .get_tag(&mut self.store, "language-exception")
            .unwrap();
        assert!(wasmtime::Tag::eq(
            &exception.tag(&mut self.store).unwrap(),
            &expected,
            &self.store
        ));
    }
    fn same(&self, a: &Val, b: &Val) -> bool {
        Rooted::ref_eq(
            &self.store,
            a.unwrap_anyref().unwrap(),
            b.unwrap_anyref().unwrap(),
        )
        .unwrap()
    }
}

#[test]
fn fifo_completion_queues_compiled_resumes_without_recursive_execution() {
    let mut h = Harness::new();
    let nil = h.value("nil", &[]);
    let mut dependencies = Vec::new();
    for n in [1, 2, 3] {
        let producer = h.value("future-pending-new", &[]);
        let dependency = h.value("future-pending-new", &[]);
        let cont = h.continuation(producer, nil.clone(), 0);
        let resume = h.closure(&format!(
            "(let [n {n}] (fn [c] (do (t/emit n) (t/resolve (t/producer c) (t/event c)))))"
        ));
        h.value(
            "async-registration-new",
            &[cont, dependency.clone(), resume],
        );
        dependencies.push(dependency);
    }
    assert_eq!(h.counts(), (3, 0));
    for index in [2, 0, 1] {
        assert_eq!(
            h.integer(
                "future-resolve",
                &[dependencies[index].clone(), nil.clone()]
            ),
            1
        );
    }
    assert!(h.trace().is_empty());
    assert_eq!(h.counts(), (3, 3));
    for expected in [vec![3.0], vec![3.0, 1.0], vec![3.0, 1.0, 2.0]] {
        h.store.gc(None).unwrap();
        assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
        assert_eq!(h.trace(), expected);
    }
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
    assert_eq!(h.counts(), (0, 0));
}

#[test]
fn generations_duplicates_and_terminal_retirement_cover_all_producer_rows() {
    let mut h = Harness::new();
    let nil = h.value("nil", &[]);
    let producer = h.value("future-pending-new", &[]);
    let dependency = h.value("future-pending-new", &[]);
    h.integer("future-resolve", &[dependency.clone(), nil.clone()]);
    let cont = h.continuation(producer.clone(), nil.clone(), 0);
    let first_resume = h.closure("(fn [c] (t/emit 1))");
    let first = h.value(
        "async-registration-new",
        &[cont.clone(), dependency.clone(), first_resume.clone()],
    );
    h.rejected(
        "async-registration-new",
        &[cont.clone(), dependency.clone(), first_resume],
    );
    assert_eq!(
        h.integer("async-registration-enqueue", &[first.clone(), Val::I32(99)]),
        0
    );
    assert_eq!(
        h.integer("async-registration-enqueue", &[first.clone(), Val::I32(0)]),
        0
    );
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.counts(), (1, 0));
    assert_eq!(h.integer("future-status", &[producer.clone()]), 0);
    h.advance(&cont, 1);
    let resume = h.closure("(fn [c] (do (t/emit 2) (t/resolve (t/producer c) (t/event c))))");
    h.value(
        "async-registration-new",
        &[cont.clone(), dependency.clone(), resume],
    );
    let other = h.continuation(producer.clone(), nil, 0);
    let obsolete = h.closure("(fn [c] (t/emit 99))");
    h.value("async-registration-new", &[other, dependency, obsolete]);
    assert_eq!(
        h.counts(),
        (2, 2),
        "obsolete consumed generation is retired"
    );
    assert_eq!(
        h.integer("async-registration-enqueue", &[first, Val::I32(0)]),
        0
    );
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.trace(), [1.0, 2.0]);
    assert_eq!(h.counts(), (0, 0));
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
    assert_eq!(h.integer("future-status", &[producer]), 1);
}

#[test]
fn failed_and_cancelled_dependencies_deliver_distinct_events_not_task_cancellation() {
    let mut h = Harness::new();
    let nil = h.value("nil", &[]);
    for (event, cancelled) in [("future-reject", false), ("future-cancel", true)] {
        let producer = h.value("future-pending-new", &[]);
        let dependency = h.value("future-pending-new", &[]);
        let cont = h.continuation(producer.clone(), nil.clone(), 0);
        let resume = h.closure("(fn [c] (t/emit 1))");
        h.value(
            "async-registration-new",
            &[cont.clone(), dependency.clone(), resume],
        );
        let payload = h.number(17.0);
        let args = if cancelled {
            vec![dependency]
        } else {
            vec![dependency, payload.clone()]
        };
        assert_eq!(h.integer(event, &args), 1);
        assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
        let status = h.snapshot_slot(&cont, 6);
        assert_eq!(
            status
                .unwrap_anyref()
                .unwrap()
                .as_i31(&h.store)
                .unwrap()
                .unwrap()
                .get_u32(),
            2,
            "cancelled dependency is a catchable failure, not owner cancellation"
        );
        let result = h.snapshot_slot(&cont, 7);
        if cancelled {
            assert_eq!(
                h.integer("async-cancelled-dependency-is", &[result.clone()]),
                1
            );
            assert_eq!(h.integer("async-runtime-trap-is", &[result]), 0);
        } else {
            assert!(h.same(&result, &payload));
            assert_eq!(h.integer("async-cancelled-dependency-is", &[result]), 0);
        }
        assert_eq!(
            h.integer("async-cancelled-dependency-is", &[nil.clone()]),
            0
        );
        assert_eq!(
            h.integer("future-status", &[producer.clone()]),
            0,
            "no premature cancellation/cleanup completion"
        );
        h.integer("future-resolve", &[producer, nil.clone()]);
        assert_eq!(h.counts(), (0, 0));
    }
}

#[test]
fn pending_producer_live_slots_and_captured_closure_survive_without_external_handles() {
    let mut h = Harness::new();
    let dependency = h.value("future-pending-new", &[]);
    let module = h.module("(let [captured 41] (fn [c] (do (t/observe (t/producer c)) (t/resolve (t/producer c) (+ captured (t/read-slot c))))))");
    {
        let mut scope = RootScope::new(&mut h.store);
        let runtime = h.runtime;
        let call = |scope: &mut RootScope<&mut Store<()>>, name: &str, args: &[Val]| {
            let mut value = [Val::null_any_ref()];
            runtime
                .get_func(&mut *scope, name)
                .unwrap()
                .call(&mut *scope, args, &mut value)
                .unwrap();
            value[0].clone()
        };
        let producer = call(&mut scope, "future-pending-new", &[]);
        let nil = call(&mut scope, "nil", &[]);
        let slots = call(&mut scope, "args-new", &[Val::I32(1)]);
        let one = call(&mut scope, "number-box", &[Val::F64(1.0f64.to_bits())]);
        slots
            .unwrap_anyref()
            .unwrap()
            .as_array(&scope)
            .unwrap()
            .unwrap()
            .set(&mut scope, 0, one)
            .unwrap();
        let unwind = call(&mut scope, "args-new", &[Val::I32(0)]);
        let state = call(&mut scope, "args-new", &[Val::I32(8)]);
        let array = state
            .unwrap_anyref()
            .unwrap()
            .as_array(&scope)
            .unwrap()
            .unwrap();
        for (index, value) in [
            nil.clone(),
            slots,
            unwind,
            nil.clone(),
            producer,
            nil.clone(),
            nil.clone(),
            nil,
        ]
        .into_iter()
        .enumerate()
        {
            array.set(&mut scope, index as u32, value).unwrap();
        }
        let cont = call(&mut scope, "async-continuation-new", &[state]);
        let instance = h.linker.instantiate(&mut scope, &module).unwrap();
        let mut resume = [Val::null_any_ref()];
        instance
            .get_func(&mut scope, "eval")
            .unwrap()
            .call(&mut scope, &[], &mut resume)
            .unwrap();
        call(
            &mut scope,
            "async-registration-new",
            &[cont, dependency.clone(), resume[0].clone()],
        );
    }
    for _ in 0..3 {
        h.store.gc(None).unwrap();
    }
    assert_eq!(h.counts(), (1, 0));
    let nil = h.value("nil", &[]);
    h.integer("future-resolve", &[dependency, nil]);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    let producer = h
        .adapter
        .get_global(&mut h.store, "observed")
        .unwrap()
        .get(&mut h.store);
    assert_eq!(h.integer("future-status", &[producer.clone()]), 1);
    let result = h.value("future-result", &[producer]);
    assert_eq!(h.decode(&result), 42.0);
    assert_eq!(h.counts(), (0, 0));
}

#[test]
fn snapshot_validation_and_nominal_identity_fail_before_commit() {
    let mut h = Harness::new();
    let nil = h.value("nil", &[]);
    let producer = h.value("future-pending-new", &[]);
    let cont = h.continuation(producer.clone(), nil, 3);
    let original = h.value("async-continuation-snapshot", &[cont.clone()]);
    for (index, invalid) in [
        (0, Val::null_any_ref()),
        (1, producer.clone()),
        (2, producer.clone()),
        (3, producer.clone()),
        (4, Val::null_any_ref()),
        (5, Val::null_any_ref()),
        (6, Val::null_any_ref()),
    ] {
        let next = h.value("async-continuation-snapshot", &[cont.clone()]);
        next.unwrap_anyref()
            .unwrap()
            .as_array(&h.store)
            .unwrap()
            .unwrap()
            .set(&mut h.store, index, invalid)
            .unwrap();
        h.rejected("async-continuation-commit", &[cont.clone(), next]);
    }
    let next = h.value("async-continuation-snapshot", &[cont.clone()]);
    let old_generation = h.scalar(2);
    next.unwrap_anyref()
        .unwrap()
        .as_array(&h.store)
        .unwrap()
        .unwrap()
        .set(&mut h.store, 5, old_generation)
        .unwrap();
    h.rejected("async-continuation-commit", &[cont.clone(), next]);
    let current = h.value("async-continuation-snapshot", &[cont.clone()]);
    let a: Vec<_> = original
        .unwrap_anyref()
        .unwrap()
        .as_array(&h.store)
        .unwrap()
        .unwrap()
        .elems(&mut h.store)
        .unwrap()
        .collect();
    let b: Vec<_> = current
        .unwrap_anyref()
        .unwrap()
        .as_array(&h.store)
        .unwrap()
        .unwrap()
        .elems(&mut h.store)
        .unwrap()
        .collect();
    for (a, b) in a.iter().zip(&b) {
        assert!(h.same(a, b));
    }
    let object = cont
        .unwrap_anyref()
        .unwrap()
        .as_struct(&h.store)
        .unwrap()
        .unwrap();
    let mut fields: Vec<_> = object.fields(&mut h.store).unwrap().collect();
    let descriptor = fields[0]
        .unwrap_anyref()
        .unwrap()
        .as_struct(&h.store)
        .unwrap()
        .unwrap();
    let descriptor_fields: Vec<_> = descriptor.fields(&mut h.store).unwrap().collect();
    let descriptor_type = descriptor.ty(&h.store).unwrap();
    let pre = StructRefPre::new(&mut h.store, descriptor_type);
    fields[0] = Val::AnyRef(Some(
        StructRef::new(&mut h.store, &pre, &descriptor_fields)
            .unwrap()
            .to_anyref(),
    ));
    let ty = object.ty(&h.store).unwrap();
    let pre = StructRefPre::new(&mut h.store, ty);
    let fake = Val::AnyRef(Some(
        StructRef::new(&mut h.store, &pre, &fields)
            .unwrap()
            .to_anyref(),
    ));
    h.rejected("async-continuation-snapshot", &[fake]);
}

#[test]
fn recursive_pump_is_a_language_failure_not_recursive_resumption() {
    let mut h = Harness::new();
    let nil = h.value("nil", &[]);
    let producer = h.value("future-pending-new", &[]);
    let dependency = h.value("future-pending-new", &[]);
    h.integer("future-resolve", &[dependency.clone(), nil.clone()]);
    let cont = h.continuation(producer.clone(), nil, 0);
    let resume = h.closure("(fn [c] (t/run))");
    h.value("async-registration-new", &[cont, dependency, resume]);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.integer("future-status", &[producer]), 2);
    assert_eq!(h.counts(), (0, 0));
    assert!(!h.store.has_pending_exception());
}

#[test]
fn initial_start_is_queued_and_delivers_zero_nil_event_to_compiled_resume() {
    let mut h = Harness::new();
    let nil = h.value("nil", &[]);
    let producer = h.value("future-pending-new", &[]);
    let cont = h.continuation(producer.clone(), nil, 0);
    let resume = h.closure(
        "(fn [c] (do (t/emit (t/event-kind c)) (t/observe (t/event c)) (t/complete c 1 42)))",
    );
    h.value("async-task-start", &[cont, resume]);
    assert_eq!(h.counts(), (1, 1));
    assert!(h.trace().is_empty());
    assert_eq!(h.integer("future-status", &[producer.clone()]), 0);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.trace(), [0.0]);
    let event = h
        .adapter
        .get_global(&mut h.store, "observed")
        .unwrap()
        .get(&mut h.store);
    assert_eq!(
        event
            .unwrap_anyref()
            .unwrap()
            .as_i31(&h.store)
            .unwrap()
            .unwrap()
            .get_u32(),
        0
    );
    assert_eq!(h.integer("future-status", &[producer.clone()]), 1);
    let result = h.value("future-result", &[producer]);
    assert_eq!(h.decode(&result), 42.0);
    assert_eq!(h.counts(), (0, 0));
}

#[test]
fn owner_completion_requires_empty_unwind_and_preserves_first_terminal_outcome() {
    let mut h = Harness::new();
    let nil = h.value("nil", &[]);
    let producer = h.value("future-pending-new", &[]);
    let cont = h.continuation(producer.clone(), nil.clone(), 0);
    let state = h.value("async-continuation-snapshot", &[cont.clone()]);
    let unwind = h.value("args-new", &[Val::I32(1)]);
    state
        .unwrap_anyref()
        .unwrap()
        .as_array(&h.store)
        .unwrap()
        .unwrap()
        .set(&mut h.store, 2, unwind)
        .unwrap();
    h.value("async-continuation-commit", &[cont.clone(), state]);
    let resume = h.closure("(fn [c] (t/emit 1))");
    h.value("async-task-start", &[cont.clone(), resume]);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    h.rejected(
        "async-task-complete",
        &[cont.clone(), Val::I32(3), nil.clone()],
    );
    assert_eq!(h.integer("future-status", &[producer.clone()]), 0);
    // This checks the completion boundary, not execution of compiler unwind.
    let state = h.value("async-continuation-snapshot", &[cont.clone()]);
    let empty = h.value("args-new", &[Val::I32(0)]);
    state
        .unwrap_anyref()
        .unwrap()
        .as_array(&h.store)
        .unwrap()
        .unwrap()
        .set(&mut h.store, 2, empty)
        .unwrap();
    h.value("async-continuation-commit", &[cont.clone(), state]);
    h.rejected(
        "async-task-complete",
        &[cont.clone(), Val::I32(0), nil.clone()],
    );
    assert_eq!(
        h.integer(
            "async-task-complete",
            &[cont.clone(), Val::I32(3), nil.clone()]
        ),
        1
    );
    assert_eq!(h.integer("future-status", &[producer]), 3);
    assert_eq!(h.counts(), (0, 0));
    let payload = h.number(99.0);
    assert_eq!(
        h.integer("async-task-complete", &[cont, Val::I32(1), payload]),
        0
    );
}

#[test]
fn compiled_resumes_restore_caller_dynamic_context_on_normal_and_language_exits() {
    for throwing in [false, true] {
        let mut h = Harness::new();
        h.install("(def ^:dynamic *x* 1)");
        let caller = h.dynamic_push(3.0);
        h.dynamic_push(17.0);
        let task = h.value("dynamic-fork", &[]);
        h.value("dynamic-switch", &[caller.clone()]);
        let producer = h.value("future-pending-new", &[]);
        let dependency = h.value("future-pending-new", &[]);
        let nil = h.value("nil", &[]);
        h.integer("future-resolve", &[dependency.clone(), nil]);
        let cont = h.continuation(producer.clone(), task.clone(), 0);
        let source = if throwing {
            "(fn [c] (do (t/emit *x*) (throw *x*)))"
        } else {
            "(fn [c] (do (t/emit *x*) (set! *x* 19) (t/complete c 1 *x*)))"
        };
        let resume = h.closure(source);
        h.value("async-registration-new", &[cont, dependency, resume]);
        h.store.gc(None).unwrap();
        assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
        let current = h.value("dynamic-save", &[]);
        assert!(h.same(&current, &caller));
        let cell = h.cells["user/*x*"].get(&mut h.store);
        let caller_value = h.value("binding-get", &[cell]);
        assert_eq!(h.decode(&caller_value), 3.0);
        assert_eq!(h.trace(), [17.0]);
        assert_eq!(
            h.integer("future-status", &[producer.clone()]),
            if throwing { 2 } else { 1 }
        );
        let result = h.value("future-result", &[producer]);
        assert_eq!(h.decode(&result), if throwing { 17.0 } else { 19.0 });
        assert_eq!(h.counts(), (0, 0));
        assert!(!h.store.has_pending_exception());
    }
}

#[test]
fn trapped_resume_is_consumed_once_and_host_recovery_restores_caller_frame() {
    check_trapped_resume(false);
}

#[test]
fn unreachable_resume_fails_with_runtime_trap_marker_and_allows_next_task() {
    check_trapped_resume(true);
}

fn check_trapped_resume(unreachable: bool) {
    let mut config = wasmtime::Config::new();
    config
        .wasm_gc(true)
        .wasm_function_references(true)
        .wasm_tail_call(true)
        .wasm_exceptions(true)
        .consume_fuel(true)
        .cranelift_opt_level(wasmtime::OptLevel::None);
    let engine = Engine::new(&config).unwrap();
    let mut h = Harness::with_engine(&engine, true);
    h.install("(def ^:dynamic *x* 1)");
    let caller = h.dynamic_push(3.0);
    h.dynamic_push(17.0);
    let task = h.value("dynamic-fork", &[]);
    h.value("dynamic-switch", &[caller.clone()]);
    let producer = h.value("future-pending-new", &[]);
    let dependency = h.value("future-pending-new", &[]);
    let nil = h.value("nil", &[]);
    h.integer("future-resolve", &[dependency.clone(), nil.clone()]);
    let cont = h.continuation(producer.clone(), task, 0);
    let resume = h.closure(if unreachable {
        "(fn [c] (do (t/emit *x*) (t/trap)))"
    } else {
        "(fn [c] (do (t/emit *x*) (loop [x 0] (recur (+ x 1)))))"
    });
    let token = h.value("async-registration-new", &[cont, dependency, resume]);
    h.store.set_fuel(50_000).unwrap();
    let error = h
        .runtime
        .get_func(&mut h.store, "async-scheduler-run-one")
        .unwrap()
        .call(&mut h.store, &[], &mut [Val::I32(-1)])
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<wasmtime::Trap>(),
        Some(&if unreachable {
            wasmtime::Trap::UnreachableCodeReached
        } else {
            wasmtime::Trap::OutOfFuel
        }),
        "{error:#}"
    );
    assert!(!error.is::<wasmtime::ThrownException>());
    assert!(!h.store.has_pending_exception());
    h.store.set_fuel(1_000_000).unwrap();
    assert_eq!(
        h.trace(),
        [17.0],
        "must enter the compiled resume before trapping"
    );
    h.runtime
        .get_func(&mut h.store, "async-scheduler-recover")
        .unwrap()
        .call(&mut h.store, &[], &mut [])
        .unwrap();
    let restored = h.value("dynamic-save", &[]);
    assert!(h.same(&restored, &caller));
    assert_eq!(
        h.integer("future-status", &[producer.clone()]),
        2,
        "host recovery publishes a distinct runtime-trap failure"
    );
    assert_eq!(
        h.integer("async-registration-enqueue", &[token, Val::I32(0)]),
        0
    );
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
    assert_eq!(h.trace(), [17.0]);
    let result = h.value("future-result", &[producer]);
    assert_eq!(h.integer("async-runtime-trap-is", &[result]), 1);
    assert_eq!(h.integer("async-runtime-trap-is", &[nil]), 0);
    assert_eq!(h.counts(), (0, 0));
    let next = h.value("future-pending-new", &[]);
    let frame = h.value("dynamic-save", &[]);
    let cont = h.continuation(next.clone(), frame, 0);
    let resume = h.closure("(fn [c] (do (t/emit 23) (t/complete c 1 42)))");
    h.value("async-task-start", &[cont, resume]);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.integer("future-status", &[next]), 1);
    assert_eq!(h.trace(), [17.0, 23.0]);
    assert_eq!(h.counts(), (0, 0));
}

#[test]
fn interrupted_trap_recovery_retries_root_retirement_without_replay() {
    let mut config = wasmtime::Config::new();
    config
        .wasm_gc(true)
        .wasm_function_references(true)
        .wasm_tail_call(true)
        .wasm_exceptions(true)
        .consume_fuel(true)
        .cranelift_opt_level(wasmtime::OptLevel::None);
    let engine = Engine::new(&config).unwrap();
    let mut h = Harness::with_engine(&engine, true);
    h.install("(def ^:dynamic *x* 1)");
    let caller = h.dynamic_push(3.0);
    h.dynamic_push(17.0);
    let task = h.value("dynamic-fork", &[]);
    h.value("dynamic-switch", &[caller.clone()]);
    let resume = h.closure("(fn [c] (do (t/emit *x*) (t/trap)))");
    let mut interrupted = 0;
    let mut finished = 0;
    // Retain dense interruption checkpoints and include completed recovery
    // after the integrated bounded stream retirement scan.
    for (iteration, budget) in (0..=2048)
        .step_by(16)
        .chain([4096, 8192, 16384, 32768, 65536, 131072])
        .enumerate()
    {
        h.store.set_fuel(1_000_000).unwrap();
        let owner = h.value("future-pending-new", &[]);
        let cont = h.continuation(owner.clone(), task.clone(), 0);
        let token = h.value("async-task-start", &[cont, resume.clone()]);
        let error = h
            .runtime
            .get_func(&mut h.store, "async-scheduler-run-one")
            .unwrap()
            .call(&mut h.store, &[], &mut [Val::I32(-1)])
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<wasmtime::Trap>(),
            Some(&wasmtime::Trap::UnreachableCodeReached)
        );
        h.store.set_fuel(budget).unwrap();
        let recovery = h
            .runtime
            .get_func(&mut h.store, "async-scheduler-recover")
            .unwrap();
        match recovery.call(&mut h.store, &[], &mut []) {
            Ok(()) => finished += 1,
            Err(error) => {
                assert_eq!(
                    error.downcast_ref::<wasmtime::Trap>(),
                    Some(&wasmtime::Trap::OutOfFuel),
                    "budget {budget}: {error:#}"
                );
                interrupted += 1;
            }
        }
        h.store.set_fuel(1_000_000).unwrap();
        // Retry twice, including the inactive path after a completed recovery.
        recovery.call(&mut h.store, &[], &mut []).unwrap();
        recovery.call(&mut h.store, &[], &mut []).unwrap();
        let restored = h.value("dynamic-save", &[]);
        assert!(h.same(&restored, &caller), "budget {budget}");
        assert_eq!(h.integer("future-status", &[owner.clone()]), 2);
        let result = h.value("future-result", &[owner]);
        assert_eq!(h.integer("async-runtime-trap-is", &[result]), 1);
        assert_eq!(h.counts(), (0, 0), "budget {budget}");
        assert_eq!(
            h.integer("async-registration-enqueue", &[token, Val::I32(0)]),
            0
        );
        assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
        assert_eq!(h.trace(), vec![17.0; iteration + 1], "budget {budget}");
    }
    // Also interrupt terminal publication with no active turn. This creates
    // the precise inactive + terminal rows state that recovery must prune.
    let expected_trace = h.trace();
    let mut inactive_terminal_roots = 0;
    for budget in (0..=2048).step_by(16) {
        h.store.set_fuel(1_000_000).unwrap();
        let owner = h.value("future-pending-new", &[]);
        let cont = h.continuation(owner.clone(), task.clone(), 0);
        h.value("async-task-start", &[cont, resume.clone()]);
        let nil = h.value("nil", &[]);
        h.store.set_fuel(budget).unwrap();
        let outcome = h
            .runtime
            .get_func(&mut h.store, "future-reject")
            .unwrap()
            .call(
                &mut h.store,
                &[owner.clone(), nil.clone()],
                &mut [Val::I32(-1)],
            );
        if let Err(error) = outcome {
            assert_eq!(
                error.downcast_ref::<wasmtime::Trap>(),
                Some(&wasmtime::Trap::OutOfFuel)
            );
        }
        h.store.set_fuel(1_000_000).unwrap();
        let terminal = h.integer("future-status", &[owner.clone()]) == 2;
        if terminal && h.counts().0 > 0 {
            inactive_terminal_roots += 1;
        }
        if !terminal {
            h.integer("future-reject", &[owner, nil]);
        }
        h.runtime
            .get_func(&mut h.store, "async-scheduler-recover")
            .unwrap()
            .call(&mut h.store, &[], &mut [])
            .unwrap();
        assert_eq!(h.counts(), (0, 0), "inactive budget {budget}");
        let restored = h.value("dynamic-save", &[]);
        assert!(h.same(&restored, &caller));
        assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
        assert_eq!(
            h.trace(),
            expected_trace,
            "inactive recovery must not invoke a callback"
        );
    }
    assert!(
        inactive_terminal_roots > 0,
        "must cover inactive terminal rows before recovery"
    );
    assert!(interrupted > 0, "sweep must exercise interrupted recovery");
    assert!(finished > 0, "sweep must also reach completed recovery");
}

#[test]
fn cancellation_before_initial_turn_skips_body_and_is_not_dependency_failure() {
    let mut h = Harness::new();
    let owner = h.source("(suss.async/future* (do (t/emit 99) 42))");
    assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 1);
    assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 0);
    assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
    assert!(h.trace().is_empty());
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.integer("future-status", &[owner.clone()]), 3);
    assert_eq!(h.integer("async-task-cancel", &[owner]), 0);
    assert!(h.trace().is_empty());
    assert_eq!(h.counts(), (0, 0));
    let producerless = h.value("future-pending-new", &[]);
    h.rejected("async-task-cancel", &[producerless.clone()]);
    assert_eq!(h.integer("future-status", &[producerless]), 0);
    let nil = h.value("nil", &[]);
    h.rejected("async-task-cancel", &[nil]);
    assert_eq!(h.counts(), (0, 0));
}

#[test]
fn cancelled_dependency_is_catchable_marker_and_does_not_cancel_owner() {
    let mut h = Harness::new();
    let dependency = h.value("future-pending-new", &[]);
    h.bind("dependency", dependency.clone());
    let owner = h.source("(suss.async/future* (try (suss.async/await* dependency) (catch :default e (do (t/emit (t/cancel-marker e)) 42)) (finally (t/emit 7))))");
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.integer("future-cancel", &[dependency]), 1);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.integer("future-status", &[owner.clone()]), 1);
    let result = h.value("future-result", &[owner]);
    assert_eq!(h.decode(&result), 42.0);
    assert_eq!(h.trace(), [1.0, 7.0]);
    assert_eq!(h.counts(), (0, 0));
}

#[test]
fn owner_cancellation_bypasses_catch_and_stays_pending_through_cleanup_await() {
    let mut h = Harness::new();
    h.install("(def ^:dynamic *x* 1)");
    let caller = h.dynamic_push(3.0);
    let dependency = h.value("future-pending-new", &[]);
    let cleanup = h.value("future-pending-new", &[]);
    h.bind("dependency", dependency.clone());
    h.bind("cleanup", cleanup.clone());
    let owner = h.source("(suss.async/future* (binding [*x* 17] (try (suss.async/await* dependency) (catch :default e (t/emit 99)) (finally (t/emit *x*) (suss.async/await* cleanup) (t/emit *x*)))))");
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 1);
    assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 0);
    let nil = h.value("nil", &[]);
    h.integer("future-resolve", &[dependency, nil.clone()]);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
    assert_eq!(h.trace(), [17.0]);
    let restored = h.value("dynamic-save", &[]);
    assert!(h.same(&restored, &caller));
    assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 0);
    h.store.gc(None).unwrap();
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
    h.integer("future-resolve", &[cleanup, nil]);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.integer("future-status", &[owner]), 3);
    assert_eq!(h.trace(), [17.0, 17.0]);
    let restored = h.value("dynamic-save", &[]);
    assert!(h.same(&restored, &caller));
    assert_eq!(h.counts(), (0, 0));
}

#[test]
fn active_cancellation_is_deferred_and_new_await_cannot_replay_body() {
    let mut h = Harness::new();
    let dependency = h.value("future-pending-new", &[]);
    let cleanup = h.value("future-pending-new", &[]);
    h.bind("dependency", dependency.clone());
    h.bind("cleanup", cleanup.clone());
    let nil = h.value("nil", &[]);
    h.bind("owner", nil);
    let owner = h.source("(suss.async/future* (try (do (t/emit (t/cancel owner)) (t/emit (t/cancel owner)) (suss.async/await* dependency) (t/emit 99)) (finally (t/emit 7) (suss.async/await* cleanup) (t/emit 8))))");
    h.bind("owner", owner.clone());
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.trace(), [1.0, 0.0], "no recursive cancellation resume");
    assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
    let nil = h.value("nil", &[]);
    h.integer("future-resolve", &[dependency, nil.clone()]);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.trace(), [1.0, 0.0, 7.0]);
    assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
    assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 0);
    h.store.gc(None).unwrap();
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
    h.integer("future-resolve", &[cleanup, nil]);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.integer("future-status", &[owner]), 3);
    assert_eq!(h.trace(), [1.0, 0.0, 7.0, 8.0]);
    assert_eq!(h.counts(), (0, 0));
}

#[test]
fn active_cancellation_then_normal_return_finishes_only_after_cleanup() {
    let mut h = Harness::new();
    let nil = h.value("nil", &[]);
    h.bind("owner", nil);
    let owner = h.source(
        "(suss.async/future* (try (do (t/emit (t/cancel owner)) 42) (finally (t/emit 7))))",
    );
    h.bind("owner", owner.clone());
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.trace(), [1.0, 7.0]);
    assert_eq!(h.integer("future-status", &[owner.clone()]), 3);
    assert_eq!(h.integer("async-task-cancel", &[owner]), 0);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
    assert_eq!(h.counts(), (0, 0));
}

#[test]
fn cancellation_does_not_abandon_already_suspended_finally() {
    let mut h = Harness::new();
    let cleanup = h.value("future-pending-new", &[]);
    h.bind("cleanup", cleanup.clone());
    let owner = h.source(
        "(suss.async/future* (try 42 (finally (t/emit 7) (suss.async/await* cleanup) (t/emit 8))))",
    );
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.trace(), [7.0]);
    assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 1);
    assert_eq!(
        h.integer("async-scheduler-run-one", &[]),
        1,
        "defer request, retain cleanup waiter"
    );
    assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
    assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 0);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
    h.store.gc(None).unwrap();
    let nil = h.value("nil", &[]);
    h.integer("future-resolve", &[cleanup, nil]);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.trace(), [7.0, 8.0]);
    assert_eq!(h.integer("future-status", &[owner]), 3);
    assert_eq!(h.counts(), (0, 0));
}

#[test]
fn cancellation_publication_fuel_sweep_keeps_old_waiter_or_complete_request() {
    let mut config = wasmtime::Config::new();
    config
        .wasm_gc(true)
        .wasm_function_references(true)
        .wasm_tail_call(true)
        .wasm_exceptions(true)
        .consume_fuel(true)
        .cranelift_opt_level(wasmtime::OptLevel::None);
    let engine = Engine::new(&config).unwrap();
    let mut h = Harness::with_engine(&engine, true);
    let resume = h.closure("(fn [c] (do (t/emit (t/event-kind c)) (t/complete c 3 nil)))");
    let mut trapped = 0;
    let mut published = 0;
    for budget in (0..=2048).step_by(16) {
        h.store.set_fuel(1_000_000).unwrap();
        let owner = h.value("future-pending-new", &[]);
        let dependency = h.value("future-pending-new", &[]);
        let nil = h.value("nil", &[]);
        let cont = h.continuation(owner.clone(), nil.clone(), 0);
        let old = h.value(
            "async-registration-new",
            &[cont, dependency.clone(), resume.clone()],
        );
        h.store.set_fuel(budget).unwrap();
        let outcome = h
            .runtime
            .get_func(&mut h.store, "async-task-cancel")
            .unwrap()
            .call(&mut h.store, &[owner.clone()], &mut [Val::I32(-1)]);
        if let Err(error) = outcome {
            assert_eq!(
                error.downcast_ref::<wasmtime::Trap>(),
                Some(&wasmtime::Trap::OutOfFuel)
            );
            trapped += 1;
        }
        h.store.set_fuel(1_000_000).unwrap();
        assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
        let counts = h.counts();
        assert!(
            counts == (1, 0) || counts == (1, 1),
            "budget {budget}: {counts:?}"
        );
        if counts.1 == 1 {
            published += 1;
        }
        assert_eq!(
            h.integer("async-task-cancel", &[owner.clone()]),
            if counts.1 == 1 { 0 } else { 1 }
        );
        assert_eq!(h.integer("future-resolve", &[dependency, nil]), 1);
        assert_eq!(
            h.integer("async-registration-enqueue", &[old, Val::I32(0)]),
            0
        );
        assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
        assert_eq!(h.integer("future-status", &[owner]), 3);
        assert_eq!(h.counts(), (0, 0));
        assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
    }
    assert!(
        trapped > 0 && published > 0,
        "sweep must cover both publication sides"
    );
    assert!(h.trace().iter().all(|event| *event == 3.0));
    assert_eq!(h.trace().len(), (0..=2048).step_by(16).count());
}

#[test]
fn yield_is_fifo_zero_nil_event_and_preserves_committed_continuation_state() {
    let mut h = Harness::new();
    let nil = h.value("nil", &[]);
    let other = h.value("future-pending-new", &[]);
    let ready = h.value("future-pending-new", &[]);
    h.integer("future-resolve", &[ready.clone(), nil.clone()]);
    let other_cont = h.continuation(other.clone(), nil.clone(), 0);
    let other_resume = h.closure("(fn [c] (do (t/emit 7) (t/complete c 1 nil)))");
    h.value("async-registration-new", &[other_cont, ready, other_resume]);

    let owner = h.value("future-pending-new", &[]);
    let slots = h.value("args-new", &[Val::I32(1)]);
    let saved = h.number(42.0);
    slots
        .unwrap_anyref()
        .unwrap()
        .as_array(&h.store)
        .unwrap()
        .unwrap()
        .set(&mut h.store, 0, saved)
        .unwrap();
    let state = h.state(owner.clone(), slots, nil.clone(), 7);
    let pc = h.scalar(17);
    let unwind = h.value("args-new", &[Val::I32(1)]);
    let state_array = state
        .unwrap_anyref()
        .unwrap()
        .as_array(&h.store)
        .unwrap()
        .unwrap();
    state_array.set(&mut h.store, 0, pc).unwrap();
    state_array.set(&mut h.store, 2, unwind).unwrap();
    let cont = h.value("async-continuation-new", &[state]);
    let before = (0..6)
        .map(|slot| h.snapshot_slot(&cont, slot))
        .collect::<Vec<_>>();
    let resume = h.closure(
        "(fn [c] (do (t/emit (t/event-kind c)) (t/observe (t/event c)) (t/emit (t/read-slot c))))",
    );
    let token = h.value("async-task-yield", &[cont.clone(), resume.clone()]);
    assert_eq!(h.counts(), (2, 2));
    assert!(
        h.trace().is_empty(),
        "yield must not invoke the resume inline"
    );
    h.rejected("async-task-yield", &[cont.clone(), resume]);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.trace(), [7.0], "completed waiter precedes FIFO yield");
    h.store.gc(None).unwrap();
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.trace(), [7.0, 0.0, 42.0]);
    let observed = h
        .adapter
        .get_global(&mut h.store, "observed")
        .unwrap()
        .get(&mut h.store);
    assert!(h.same(&observed, &nil));
    for (index, expected) in before.iter().enumerate() {
        let actual = h.snapshot_slot(&cont, index as u32);
        assert!(
            h.same(&actual, expected),
            "yield altered state slot {index}"
        );
    }
    assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
    assert_eq!(
        h.integer("async-registration-enqueue", &[token, Val::I32(7)]),
        0
    );
    // The opaque nonempty unwind above exercises preservation, not compiler
    // cleanup. Empty it explicitly before checking terminal root retirement.
    let state = h.value("async-continuation-snapshot", &[cont.clone()]);
    let empty = h.value("args-new", &[Val::I32(0)]);
    state
        .unwrap_anyref()
        .unwrap()
        .as_array(&h.store)
        .unwrap()
        .unwrap()
        .set(&mut h.store, 2, empty)
        .unwrap();
    h.value("async-continuation-commit", &[cont.clone(), state]);
    assert_eq!(
        h.integer("async-task-complete", &[cont, Val::I32(1), nil]),
        1
    );
    assert_eq!(h.counts(), (0, 0));
}

#[test]
fn repeated_yields_bound_consumed_rows_and_preserve_cancellation_latch() {
    let mut h = Harness::new();
    let nil = h.value("nil", &[]);
    let owner = h.value("future-pending-new", &[]);
    let cont = h.continuation(owner.clone(), nil.clone(), 0);
    let resume = h.closure("(fn [c] (t/emit (t/event-kind c)))");
    h.value("async-task-yield", &[cont.clone(), resume.clone()]);
    assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 1);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.trace(), [3.0]);
    assert_eq!(h.counts(), (1, 0), "consumed cancellation latch survives");
    // Storage-level generation advancement models compiler checkpoints. Root's
    // source fairness gates execute the actual loop/backedge lowering.
    for generation in 2..258 {
        h.advance(&cont, generation);
        let token = h.value("async-task-yield", &[cont.clone(), resume.clone()]);
        assert_eq!(
            h.counts(),
            (2, 1),
            "generation {generation} retains obsolete rows"
        );
        assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 0);
        assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
        assert_eq!(h.counts(), (2, 0));
        assert_eq!(
            h.integer(
                "async-registration-enqueue",
                &[token, Val::I32(generation as i32)]
            ),
            0
        );
        if generation % 32 == 0 {
            h.store.gc(None).unwrap();
        }
    }
    h.rejected("async-task-yield", &[cont.clone(), resume]);
    assert_eq!(h.integer("future-status", &[owner]), 0);
    assert_eq!(
        h.integer("async-task-complete", &[cont, Val::I32(3), nil]),
        1
    );
    assert_eq!(h.counts(), (0, 0));
    assert_eq!(h.trace().len(), 257);
    assert!(h.trace()[1..].iter().all(|event| *event == 0.0));
}

#[test]
fn source_yields_preserve_shadowed_frames_and_cancelled_cleanup_across_gc() {
    let mut config = wasmtime::Config::new();
    config
        .wasm_gc(true)
        .wasm_function_references(true)
        .wasm_tail_call(true)
        .wasm_exceptions(true)
        .consume_fuel(true)
        .cranelift_opt_level(wasmtime::OptLevel::None);
    let engine = Engine::new(&config).unwrap();
    let mut h = Harness::with_engine(&engine, true);
    h.install("(def ^:dynamic *x* 1)");
    let caller = h.dynamic_push(3.0);
    let owner = h.source("(suss.async/future* (binding [*x* 17] (try (loop [n 0] (do (t/emit *x*) (recur (+ n 1)))) (finally (binding [*x* 23] (loop [n 0] (if (< n 4) (do (t/emit *x*) (recur (+ n 1))) nil))) (t/emit *x*)))))");
    for _ in 0..3 {
        h.store.set_fuel(1_000_000).unwrap();
        assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
        let restored = h.value("dynamic-save", &[]);
        assert!(h.same(&restored, &caller));
        assert_eq!(h.integer("future-status", &[owner.clone()]), 0);
        h.store.gc(None).unwrap();
    }
    assert_eq!(h.trace(), [17.0, 17.0, 17.0]);
    assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 1);
    h.store.set_fuel(1_000_000).unwrap();
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(
        h.integer("future-status", &[owner.clone()]),
        0,
        "cleanup's first backedge yields before cancellation is terminal"
    );
    assert_eq!(h.trace(), [17.0, 17.0, 17.0, 23.0]);
    for _ in 0..8 {
        h.store.set_fuel(1_000_000).unwrap();
        if h.integer("future-status", &[owner.clone()]) == 3 {
            break;
        }
        assert_eq!(h.integer("async-task-cancel", &[owner.clone()]), 0);
        assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
        let restored = h.value("dynamic-save", &[]);
        assert!(h.same(&restored, &caller));
        let (rows, queue) = h.counts();
        assert!(
            rows <= 3 && queue <= 1,
            "cleanup yield roots grew: {rows}/{queue}"
        );
        h.store.gc(None).unwrap();
    }
    assert_eq!(h.integer("future-status", &[owner]), 3);
    assert_eq!(h.trace(), [17.0, 17.0, 17.0, 23.0, 23.0, 23.0, 23.0, 17.0]);
    let restored = h.value("dynamic-save", &[]);
    assert!(h.same(&restored, &caller));
    let cell = h.cells["user/*x*"].get(&mut h.store);
    let current = h.value("binding-get", &[cell]);
    assert_eq!(h.decode(&current), 3.0);
    assert_eq!(h.counts(), (0, 0));
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 0);
}

#[test]
fn cancel_all_snapshots_unique_owners_and_preserves_pending_host_cleanup() {
    let mut h = Harness::new();
    let dependency = h.value("future-pending-new", &[]);
    let cleanup = h.value("future-pending-new", &[]);
    h.bind("dependency", dependency.clone());
    h.bind("cleanup", cleanup.clone());
    let first = h.source("(suss.async/future* (try (suss.async/await* dependency) (finally (t/emit 1) (suss.async/await* cleanup) (t/emit 2))))");
    let second = h.source("(suss.async/future* (try (suss.async/await* dependency) (finally (t/emit 3) (suss.async/await* cleanup) (t/emit 4))))");
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.integer("async-scheduler-pending-task-count", &[]), 2);
    assert_eq!(h.integer("async-task-cancel", &[first.clone()]), 1);
    assert_eq!(h.integer("async-scheduler-cancel-all", &[]), 1);
    assert_eq!(h.integer("async-scheduler-cancel-all", &[]), 0);
    assert!(
        h.trace().is_empty(),
        "reset preparation never resumes inline"
    );
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.trace(), [1.0, 3.0]);
    assert_eq!(h.integer("future-status", &[first.clone()]), 0);
    assert_eq!(h.integer("future-status", &[second.clone()]), 0);
    assert_eq!(
        h.integer("async-scheduler-pending-task-count", &[]),
        2,
        "consumed cancellation rows plus cleanup rows count each owner only once"
    );
    assert_eq!(h.integer("async-scheduler-cancel-all", &[]), 0);
    let nil = h.value("nil", &[]);
    assert_eq!(h.integer("future-resolve", &[dependency, nil.clone()]), 1);
    assert_eq!(
        h.integer("async-scheduler-run-one", &[]),
        0,
        "no ready work does not mean suspended cleanup is drained"
    );
    assert_eq!(
        h.integer("future-status", &[cleanup.clone()]),
        0,
        "producerless host I/O is not silently cancelled by task cancellation"
    );
    h.store.gc(None).unwrap();
    assert_eq!(h.integer("future-resolve", &[cleanup, nil]), 1);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.trace(), [1.0, 3.0, 2.0, 4.0]);
    assert_eq!(h.integer("future-status", &[first]), 3);
    assert_eq!(h.integer("future-status", &[second]), 3);
    assert_eq!(h.integer("async-scheduler-pending-task-count", &[]), 0);
    assert_eq!(h.integer("async-scheduler-cancel-all", &[]), 0);
    assert_eq!(h.counts(), (0, 0));
}

#[test]
fn cancel_all_from_active_callback_is_deferred_and_does_not_skip_snapshot_owners() {
    let mut h = Harness::new();
    let active =
        h.source("(suss.async/future* (do (t/emit (t/cancel-all)) (t/emit (t/cancel-all)) 42))");
    let queued = h.source("(suss.async/future* (do (t/emit 99) 42))");
    assert_eq!(h.integer("async-scheduler-pending-task-count", &[]), 2);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(
        h.trace(),
        [2.0, 0.0],
        "requests don't recursively invoke callbacks"
    );
    assert_eq!(h.integer("future-status", &[active]), 3);
    assert_eq!(h.integer("future-status", &[queued.clone()]), 0);
    assert_eq!(h.integer("async-scheduler-pending-task-count", &[]), 1);
    assert_eq!(h.integer("async-scheduler-cancel-all", &[]), 0);
    h.store.gc(None).unwrap();
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.integer("future-status", &[queued]), 3);
    assert_eq!(h.trace(), [2.0, 0.0]);
    assert_eq!(h.integer("async-scheduler-pending-task-count", &[]), 0);
    assert_eq!(h.counts(), (0, 0));
}

#[test]
fn cancel_all_captures_owners_once_and_drives_each_cleanup() {
    let mut h = Harness::new();
    let dependency = h.value("future-pending-new", &[]);
    h.bind("dependency", dependency);
    let first = h.source("(suss.async/future* (try (suss.async/await* dependency) (finally (t/emit 1))))");
    let second = h.source("(suss.async/future* (try (suss.async/await* dependency) (finally (t/emit 2))))");
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.integer("async-scheduler-pending-task-count", &[]), 2);
    assert_eq!(h.integer("async-scheduler-cancel-all", &[]), 2);
    assert_eq!(h.integer("async-scheduler-cancel-all", &[]), 0);
    h.store.gc(None).unwrap();
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.integer("async-scheduler-run-one", &[]), 1);
    assert_eq!(h.integer("future-status", &[first]), 3);
    assert_eq!(h.integer("future-status", &[second]), 3);
    assert_eq!(h.trace(), [1.0, 2.0]);
    assert_eq!(h.integer("async-scheduler-pending-task-count", &[]), 0);
    assert_eq!(h.counts(), (0, 0));
    assert_eq!(h.integer("async-scheduler-cancel-all", &[]), 0);
}
