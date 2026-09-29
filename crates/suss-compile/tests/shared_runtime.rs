//! M0 feasibility probe: this is not the new compiler/runtime implementation.
mod support;
use wasmtime::{Global, Instance, Linker, Module, Store};

const PRELUDE: &str = include_str!("fixtures/shared-prelude.wat");

fn module(engine: &wasmtime::Engine, body: &str) -> Module {
    Module::new(engine, format!("(module {PRELUDE} {body})")).unwrap()
}

fn require_abi(store: &mut Store<()>, instance: &Instance, expected: i32) -> Result<(), String> {
    let actual = instance
        .get_global(&mut *store, "abi-version")
        .and_then(|g| g.get(&mut *store).i32())
        .ok_or("missing ABI version")?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!("ABI mismatch: expected {expected}, got {actual}"))
    }
}

#[test]
fn fragments_share_closures_roots_and_nominal_descriptors() {
    let engine = support::engine();
    let runtime = module(
        &engine,
        r#"
        (global (export "abi-version") i32 (i32.const 1))
        (global (export "binding") (mut (ref null eq)) (ref.null eq))
        (global (export "descriptor") (mut (ref null $descriptor)) (ref.null $descriptor))
        (global (export "object") (mut (ref null $object)) (ref.null $object))
    "#,
    );
    let a = module(
        &engine,
        r#"
        (import "rt" "binding" (global $binding (mut (ref null eq))))
        (import "rt" "descriptor" (global $descriptor (mut (ref null $descriptor))))
        (import "rt" "object" (global $object (mut (ref null $object))))
        (func $add (type $invoke) (param $env (ref null eq)) (param $args (ref $args)) (result (ref null eq))
            (ref.i31 (i32.add
                (i31.get_s (ref.cast (ref i31) (local.get $env)))
                (i31.get_s (ref.cast (ref i31) (array.get $args (local.get $args) (i32.const 0)))))))
        (elem declare func $add)
        (func (export "initialize")
            (global.set $binding (struct.new $closure (ref.i31 (i32.const 40)) (ref.func $add)))
            (global.set $descriptor (struct.new $descriptor (i32.const 101)))
            (global.set $object (struct.new $object
                (ref.as_non_null (global.get $descriptor))
                (array.new_fixed $args 1 (ref.i31 (i32.const 7))))))
    "#,
    );
    let b = module(
        &engine,
        r#"
        (import "rt" "binding" (global $binding (mut (ref null eq))))
        (import "rt" "descriptor" (global $descriptor (mut (ref null $descriptor))))
        (import "rt" "object" (global $object (mut (ref null $object))))
        (func (export "call") (param $n i32) (result i32) (local $f (ref null $closure))
            (local.set $f (ref.cast (ref $closure) (global.get $binding)))
            (i31.get_s (ref.cast (ref i31)
                (call_ref $invoke
                    (struct.get $closure 0 (local.get $f))
                    (array.new_fixed $args 1 (ref.i31 (local.get $n)))
                    (struct.get $closure 1 (local.get $f))))))
        (func (export "same-type") (result i32)
            (ref.eq (struct.get $object 0 (global.get $object)) (global.get $descriptor)))
        (func (export "new-type")
            ;; Same Wasm layout and numeric payload, different nominal descriptor.
            (global.set $descriptor (struct.new $descriptor (i32.const 101))))
        (func (export "old-field") (result i32)
            (i31.get_s (ref.cast (ref i31)
                (array.get $args (struct.get $object 1 (global.get $object)) (i32.const 0)))))
    "#,
    );
    let mut store = Store::new(&engine, ());
    let rt = Instance::new(&mut store, &runtime, &[]).unwrap();
    require_abi(&mut store, &rt, 1).unwrap();
    assert!(
        require_abi(&mut store, &rt, 2)
            .unwrap_err()
            .contains("ABI mismatch")
    );
    let mut linker = Linker::new(&engine);
    linker.instance(&mut store, "rt", rt).unwrap();
    let first = linker.instantiate(&mut store, &a).unwrap();
    first
        .get_typed_func::<(), ()>(&mut store, "initialize")
        .unwrap()
        .call(&mut store, ())
        .unwrap();
    // There are no Rust roots to the stored closure/object. Only the shared
    // runtime globals retain them across GC and loading the next fragment.
    store.gc(None);
    let second = linker.instantiate(&mut store, &b).unwrap();
    let call = second
        .get_typed_func::<i32, i32>(&mut store, "call")
        .unwrap();
    assert_eq!(call.call(&mut store, 2).unwrap(), 42);
    let same_type = second
        .get_typed_func::<(), i32>(&mut store, "same-type")
        .unwrap();
    assert_eq!(same_type.call(&mut store, ()).unwrap(), 1);
    second
        .get_typed_func::<(), ()>(&mut store, "new-type")
        .unwrap()
        .call(&mut store, ())
        .unwrap();
    store.gc(None);
    assert_eq!(same_type.call(&mut store, ()).unwrap(), 0);
    assert_eq!(
        second
            .get_typed_func::<(), i32>(&mut store, "old-field")
            .unwrap()
            .call(&mut store, ())
            .unwrap(),
        7
    );
    assert_eq!(call.call(&mut store, 3).unwrap(), 43);
    // Keep this name visible in the probe: bindings are shared globals today;
    // the production ABI will expose language-level binding cells and metadata.
    let _: Global = rt.get_global(&mut store, "binding").unwrap();
}
