//! Execute generated runtime switches and compiled source against the same cells.
//! These are context-storage gates, not continuation/scheduler acceptance.
mod support;
use std::collections::BTreeMap;
use suss_compile::{
    portable::{
        self,
        resolve::{Environment, Phase},
    },
    runtime_abi,
};
use wasmtime::{
    Global, GlobalType, Instance, Linker, Module, Mutability, RefType, Rooted, Store, StructRef,
    StructRefPre, Val, ValType,
};

struct Harness {
    store: Store<()>,
    runtime: Instance,
    linker: Linker<()>,
    environment: Environment,
    cells: BTreeMap<String, Global>,
}
impl Harness {
    fn new() -> Self {
        let engine = support::engine();
        let mut store = Store::new(&engine, ());
        let bytes = runtime_abi::module();
        runtime_abi::verify_artifact(&bytes, &runtime_abi::Manifest::default()).unwrap();
        let runtime =
            Instance::new(&mut store, &Module::new(&engine, bytes).unwrap(), &[]).unwrap();
        let mut linker = Linker::new(&engine);
        linker
            .instance(&mut store, "suss.runtime", runtime)
            .unwrap();
        let prepared = portable::prepare_fragment(
            "(def ^:dynamic *x* 1) (def ^:dynamic *y* 2)",
            &Environment::default(),
            Phase::Runtime,
        )
        .unwrap();
        let mut cells = BTreeMap::new();
        for identity in &prepared.cells {
            let mut output = [Val::null_any_ref()];
            runtime
                .get_func(&mut store, "binding-unbound")
                .unwrap()
                .call(&mut store, &[], &mut output)
                .unwrap();
            let ty = output[0]
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
                output[0].clone(),
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
            cells.insert(identity.name().to_owned(), global);
        }
        let instance = linker
            .instantiate(&mut store, &Module::new(&engine, prepared.wasm).unwrap())
            .unwrap();
        instance
            .get_func(&mut store, "eval")
            .unwrap()
            .call(&mut store, &[], &mut [Val::null_any_ref()])
            .unwrap();
        Self {
            store,
            runtime,
            linker,
            environment: prepared.environment,
            cells,
        }
    }
    fn call(&mut self, name: &str, args: &[Val]) -> Val {
        let function = self.runtime.get_func(&mut self.store, name).unwrap();
        let mut output = [Val::null_any_ref()];
        function.call(&mut self.store, args, &mut output).unwrap();
        output[0].clone()
    }
    fn nil(&mut self) -> Val {
        self.call("nil", &[])
    }
    fn cell(&mut self, name: &str) -> Val {
        self.cells[name].get(&mut self.store)
    }
    fn number(&mut self, value: f64) -> Val {
        self.call("number-box", &[Val::F64(value.to_bits())])
    }
    fn decode(&mut self, value: &Val) -> f64 {
        let object = value
            .unwrap_anyref()
            .unwrap()
            .as_struct(&self.store)
            .unwrap()
            .unwrap();
        object.field(&mut self.store, 0).unwrap().unwrap_f64()
    }
    fn read(&mut self, name: &str) -> f64 {
        let cell = self.cell(name);
        let value = self.call("binding-get", &[cell]);
        self.decode(&value)
    }
    fn eval(&mut self, source: &str) -> f64 {
        let prepared =
            portable::prepare_fragment(source, &self.environment, Phase::Runtime).unwrap();
        let module = Module::new(self.store.engine(), prepared.wasm).unwrap();
        let instance = self.linker.instantiate(&mut self.store, &module).unwrap();
        let mut output = [Val::null_any_ref()];
        instance
            .get_func(&mut self.store, "eval")
            .unwrap()
            .call(&mut self.store, &[], &mut output)
            .unwrap();
        self.decode(&output[0])
    }
    fn push(&mut self, bindings: &[(&str, f64)]) -> Val {
        let entries = self.call("args-new", &[Val::I32(bindings.len() as i32 * 3)]);
        let array = entries
            .unwrap_anyref()
            .unwrap()
            .as_array(&self.store)
            .unwrap()
            .unwrap();
        for (index, (name, value)) in bindings.iter().enumerate() {
            let cell = self.cell(name);
            let old = self.call("binding-get", &[cell.clone()]);
            let current = self.number(*value);
            for (offset, item) in [cell, old, current].into_iter().enumerate() {
                array
                    .set(&mut self.store, (index * 3 + offset) as u32, item)
                    .unwrap();
            }
        }
        self.call("dynamic-push", &[entries])
    }
    fn same(&self, a: &Val, b: &Val) -> bool {
        Rooted::ref_eq(
            &self.store,
            a.unwrap_anyref().unwrap(),
            b.unwrap_anyref().unwrap(),
        )
        .unwrap()
    }
    fn forge(&mut self, exemplar: &Val, parent: Val, entries: Val) -> Val {
        let ty = exemplar
            .unwrap_anyref()
            .unwrap()
            .as_struct(&self.store)
            .unwrap()
            .unwrap()
            .ty(&self.store)
            .unwrap();
        let pre = StructRefPre::new(&mut self.store, ty);
        Val::AnyRef(Some(
            StructRef::new(&mut self.store, &pre, &[parent, entries])
                .unwrap()
                .to_anyref(),
        ))
    }
    fn rejected(&mut self, operation: &str, args: &[Val]) {
        let function = self.runtime.get_func(&mut self.store, operation).unwrap();
        let error = function
            .call(&mut self.store, args, &mut [Val::null_any_ref()])
            .unwrap_err();
        assert!(error.is::<wasmtime::ThrownException>(), "{error:#}");
        assert!(!error.is::<wasmtime::Trap>());
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
        assert!(!self.store.has_pending_exception());
    }
}

#[test]
fn dynamic_switch_retains_nested_effective_values_writes_and_source_caller_after_gc() {
    let mut h = Harness::new();
    let nil = h.call("dynamic-save", &[]);
    let outer = h.push(&[("*x*", 10.0), ("*y*", 20.0)]);
    let inner = h.push(&[("*x*", 30.0)]);
    let saved = h.call("dynamic-save", &[]);
    assert!(h.same(&saved, &inner));
    let old = h.call("dynamic-switch", &[nil.clone()]);
    assert!(h.same(&old, &inner));
    h.store.gc(None).unwrap();
    assert_eq!((h.read("*x*"), h.read("*y*")), (1.0, 2.0));
    let other = h.push(&[("*y*", 90.0)]);
    let old = h.call("dynamic-switch", &[saved.clone()]);
    assert!(h.same(&old, &other));
    h.store.gc(None).unwrap();
    // *y* must come from the ancestor, not the inner frame or inactive task.
    assert_eq!((h.eval("*x*"), h.eval("*y*")), (30.0, 20.0));
    assert_eq!(h.eval("(set! *x* 31)"), 31.0);
    assert_eq!(h.eval("(set! *y* 21)"), 21.0);
    h.call("dynamic-switch", &[other.clone()]);
    assert_eq!((h.read("*x*"), h.read("*y*")), (1.0, 90.0));
    h.call("dynamic-switch", &[saved.clone()]);
    assert_eq!((h.read("*x*"), h.read("*y*")), (31.0, 21.0));
    assert_eq!(h.eval("(binding [*x* 100] (binding [*y* 200] *x*))"), 100.0);
    let current = h.call("dynamic-save", &[]);
    assert!(h.same(&current, &saved));
    assert_eq!((h.read("*x*"), h.read("*y*")), (31.0, 21.0));
    // A switch did not pop or modify the old snapshots. Ordinary scope exit is
    // still explicit and restores its original caller values.
    let pop = h.runtime.get_func(&mut h.store, "dynamic-pop").unwrap();
    pop.call(&mut h.store, &[inner], &mut []).unwrap();
    let current = h.call("dynamic-save", &[]);
    assert!(h.same(&current, &outer));
    assert_eq!((h.read("*x*"), h.read("*y*")), (10.0, 21.0));
    pop.call(&mut h.store, &[outer], &mut []).unwrap();
    assert_eq!((h.read("*x*"), h.read("*y*")), (1.0, 2.0));
    let old = h.call("dynamic-switch", &[nil.clone()]);
    assert!(h.same(&old, &nil));
}

#[test]
fn dynamic_switch_validates_whole_chains_before_publication_or_effects() {
    let mut h = Harness::new();
    let nil = h.nil();
    let valid = h.push(&[("*x*", 10.0)]);
    let root = h.runtime.get_global(&mut h.store, "dynamic-frame").unwrap();
    let empty = h.call("args-new", &[Val::I32(0)]);
    let short = h.call("args-new", &[Val::I32(1)]);
    let false_value = h.call("false", &[]);
    let bad_parent = h.forge(&valid, false_value.clone(), empty.clone());
    let bad_shape = h.forge(&valid, nil.clone(), short);
    // Reverse scanning sees the valid final triplet before the invalid earlier
    // one. A switch must reject it even when that final binding hides the error.
    let entries = h.call("args-new", &[Val::I32(6)]);
    let array = entries
        .unwrap_anyref()
        .unwrap()
        .as_array(&h.store)
        .unwrap()
        .unwrap();
    let cell = h.cell("*x*");
    let old = h.number(1.0);
    let value = h.number(99.0);
    for (index, item) in [
        false_value.clone(),
        nil.clone(),
        nil.clone(),
        cell,
        old,
        value,
    ]
    .into_iter()
    .enumerate()
    {
        array.set(&mut h.store, index as u32, item).unwrap();
    }
    let bad_cell = h.forge(&valid, nil.clone(), entries);
    let valid_entries = valid
        .unwrap_anyref()
        .unwrap()
        .as_struct(&h.store)
        .unwrap()
        .unwrap()
        .field(&mut h.store, 1)
        .unwrap();
    let hidden_ancestor = h.forge(&valid, bad_cell.clone(), valid_entries);
    for invalid in [
        Val::null_any_ref(),
        false_value,
        bad_parent,
        bad_shape,
        bad_cell,
        hidden_ancestor,
    ] {
        h.store.gc(None).unwrap();
        h.rejected("dynamic-switch", &[invalid.clone()]);
        let current = root.get(&mut h.store);
        assert!(h.same(&current, &valid));
        assert_eq!((h.eval("*x*"), h.eval("*y*")), (10.0, 2.0));
        // An invalid outgoing context cannot be silently saved or replaced.
        root.set(&mut h.store, invalid.clone()).unwrap();
        h.rejected("dynamic-save", &[]);
        h.rejected("dynamic-fork", &[]);
        h.rejected("dynamic-switch", &[nil.clone()]);
        let current = root.get(&mut h.store);
        if invalid.unwrap_anyref().is_none() {
            assert!(current.unwrap_anyref().is_none());
        } else {
            assert!(h.same(&current, &invalid));
        }
        root.set(&mut h.store, valid.clone()).unwrap();
        assert_eq!((h.read("*x*"), h.read("*y*")), (10.0, 2.0));
    }
    h.call("dynamic-switch", &[nil]);
    assert_eq!((h.eval("*x*"), h.eval("*y*")), (1.0, 2.0));
}

fn frame_fields(h: &mut Harness, value: &Val) -> Vec<Val> {
    value
        .unwrap_anyref()
        .unwrap()
        .as_struct(&h.store)
        .unwrap()
        .unwrap()
        .fields(&mut h.store)
        .unwrap()
        .collect()
}

#[test]
fn dynamic_forks_clone_every_frame_and_array_with_independent_ancestor_writes_after_gc() {
    let mut h = Harness::new();
    let outer = h.push(&[("*x*", 10.0), ("*y*", 20.0)]);
    let caller = h.push(&[("*x*", 30.0)]);
    // Snapshot the effective value after a write, not just the initializer.
    assert_eq!(h.eval("(set! *y* 21)"), 21.0);
    let first = h.call("dynamic-fork", &[]);
    let second = h.call("dynamic-fork", &[]);
    let current = h.call("dynamic-save", &[]);
    assert!(h.same(&current, &caller));
    assert_eq!((h.read("*x*"), h.read("*y*")), (30.0, 21.0));
    h.store.gc(None).unwrap();

    // Independent layout observations prove every frame and mutable array is
    // fresh, while cell identities and all old/current value references survive.
    let mut source = caller.clone();
    let mut a = first.clone();
    let mut b = second.clone();
    for _ in 0..2 {
        assert!(!h.same(&source, &a));
        assert!(!h.same(&source, &b));
        assert!(!h.same(&a, &b));
        let sf = frame_fields(&mut h, &source);
        let af = frame_fields(&mut h, &a);
        let bf = frame_fields(&mut h, &b);
        assert!(!h.same(&sf[1], &af[1]));
        assert!(!h.same(&sf[1], &bf[1]));
        assert!(!h.same(&af[1], &bf[1]));
        let elements = |h: &mut Harness, value: &Val| -> Vec<Val> {
            value
                .unwrap_anyref()
                .unwrap()
                .as_array(&h.store)
                .unwrap()
                .unwrap()
                .elems(&mut h.store)
                .unwrap()
                .collect()
        };
        let se = elements(&mut h, &sf[1]);
        let ae = elements(&mut h, &af[1]);
        let be = elements(&mut h, &bf[1]);
        assert_eq!(se.len(), ae.len());
        assert_eq!(se.len(), be.len());
        for ((source, a), b) in se.iter().zip(&ae).zip(&be) {
            assert!(h.same(source, a));
            assert!(h.same(source, b));
        }
        source = sf[0].clone();
        a = af[0].clone();
        b = bf[0].clone();
    }
    let nil = h.nil();
    for end in [&source, &a, &b] {
        assert!(h.same(end, &nil));
    }

    let x = h.cell("*x*");
    let y = h.cell("*y*");
    let set = h.runtime.get_func(&mut h.store, "binding-set").unwrap();
    h.call("dynamic-switch", &[first.clone()]);
    let value = h.number(111.0);
    set.call(&mut h.store, &[y.clone(), value], &mut [])
        .unwrap();
    let value = h.number(31.0);
    set.call(&mut h.store, &[x.clone(), value], &mut [])
        .unwrap();
    h.call("dynamic-switch", &[second.clone()]);
    assert_eq!((h.eval("*x*"), h.eval("*y*")), (30.0, 21.0));
    let value = h.number(222.0);
    set.call(&mut h.store, &[y.clone(), value], &mut [])
        .unwrap();
    let value = h.number(32.0);
    set.call(&mut h.store, &[x, value], &mut []).unwrap();
    h.call("dynamic-switch", &[caller.clone()]);
    let value = h.number(23.0);
    set.call(&mut h.store, &[y, value], &mut []).unwrap();
    h.store.gc(None).unwrap();
    assert_eq!((h.eval("*x*"), h.eval("*y*")), (30.0, 23.0));
    for (context, expected) in [(&first, (31.0, 111.0)), (&second, (32.0, 222.0))] {
        h.call("dynamic-switch", &[context.clone()]);
        h.store.gc(None).unwrap();
        assert_eq!((h.eval("*x*"), h.eval("*y*")), expected);
        // Observe the cloned ancestor without popping: outer shadowed *x*
        // remains 10, and the independent ancestor *y* has its task's write.
        let fields = frame_fields(&mut h, context);
        h.call("dynamic-switch", &[fields[0].clone()]);
        assert_eq!((h.read("*x*"), h.read("*y*")), (10.0, expected.1));
    }
    h.call("dynamic-switch", &[caller.clone()]);
    assert_eq!((h.eval("*x*"), h.eval("*y*")), (30.0, 23.0));
    let current = h.call("dynamic-save", &[]);
    assert!(h.same(&current, &caller));
    let fields = frame_fields(&mut h, &caller);
    assert!(h.same(&fields[0], &outer));
}

#[test]
fn dynamic_fork_empty_context_returns_nil_without_publishing_a_frame() {
    let mut h = Harness::new();
    let caller = h.call("dynamic-save", &[]);
    let forked = h.call("dynamic-fork", &[]);
    h.store.gc(None).unwrap();
    assert!(h.same(&forked, &caller));
    let nil = h.nil();
    assert!(h.same(&forked, &nil));
    let current = h.call("dynamic-save", &[]);
    assert!(h.same(&current, &caller));
    assert_eq!((h.eval("*x*"), h.eval("*y*")), (1.0, 2.0));
}
