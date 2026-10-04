//! Execute emitted initialization and source fragments without native Session setup.
mod support;
use suss_compile::{
    portable::{
        self,
        resolve::{Environment, Phase},
    },
    runtime_abi,
};
use wasmtime::{Instance, Linker, Module, Store, Val};

#[test]
fn emitted_source_cells_initialize_once_and_keep_live_closures_across_fragments() {
    let engine = support::engine();
    for phase in [Phase::Runtime, Phase::Macro] {
        let first = portable::prepare_fragment(
            "(def visits 0) (def f (fn [] (set! visits (+ visits 1)))) (f)",
            &Environment::default(),
            phase,
        )
        .unwrap();
        // Retained arithmetic source-callee analysis materializes canonical +
        // alongside the two source definitions. Only source cells start unbound.
        assert_eq!(
            first
                .cells
                .iter()
                .map(|cell| (cell.namespace(), cell.name()))
                .collect::<Vec<_>>(),
            vec![("suss.core", "+"), ("user", "f"), ("user", "visits")]
        );
        assert!(first.cells.iter().all(|cell| cell.phase() == phase));
        let emitted = portable::core_bindings::compile_with_cells(phase, &first.cells).unwrap();
        // Repeated requests cannot create duplicate exports or cells.
        let repeated = first
            .cells
            .iter()
            .chain(first.cells.iter())
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            emitted.wasm,
            portable::core_bindings::compile_with_cells(phase, &repeated)
                .unwrap()
                .wasm
        );
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
        let initialized = linker
            .instantiate(&mut store, &Module::new(&engine, emitted.wasm).unwrap())
            .unwrap();
        for cell in emitted.cells {
            let global = initialized
                .get_global(&mut store, &cell.import_name())
                .unwrap();
            linker
                .define(&store, cell.import_module(), &cell.import_name(), global)
                .unwrap();
            if first.cells.contains(&cell) {
                let mut result = [Val::null_any_ref()];
                let value = global.get(&mut store);
                runtime
                    .get_func(&mut store, "binding-bound")
                    .unwrap()
                    .call(&mut store, &[value], &mut result)
                    .unwrap();
                assert_eq!(
                    result[0]
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&store)
                        .unwrap()
                        .unwrap()
                        .get_i32(),
                    if cell.namespace() == "suss.core" {
                        4
                    } else {
                        2
                    },
                    "canonical core is initialized; source definitions remain unbound: {cell:?}"
                );
            }
        }
        let first_module = linker
            .instantiate(&mut store, &Module::new(&engine, first.wasm).unwrap())
            .unwrap();
        assert_eq!(evaluate_number(&mut store, first_module), 1.0_f64.to_bits());
        store.gc(None).unwrap();
        let second = portable::prepare_fragment("(f)", &first.environment, phase).unwrap();
        // Preparation carries the complete catalog; the second fragment adds no cells.
        assert_eq!(second.cells, first.cells);
        let second_module = linker
            .instantiate(&mut store, &Module::new(&engine, second.wasm).unwrap())
            .unwrap();
        assert_eq!(
            evaluate_number(&mut store, second_module),
            2.0_f64.to_bits()
        );
    }
}

fn evaluate_number(store: &mut Store<()>, instance: Instance) -> u64 {
    let mut result = [Val::null_any_ref()];
    instance
        .get_func(&mut *store, "eval")
        .unwrap()
        .call(&mut *store, &[], &mut result)
        .unwrap();
    let number = result[0]
        .unwrap_anyref()
        .unwrap()
        .as_struct(&*store)
        .unwrap()
        .unwrap();
    let fields = number.fields(store).unwrap().collect::<Vec<_>>();
    match fields.as_slice() {
        [Val::F64(bits)] => *bits,
        _ => panic!("not a boxed number: {fields:?}"),
    }
}

#[test]
fn emitted_cells_reject_foreign_phase_before_emission() {
    let mut environment = Environment::default();
    let foreign = environment
        .declare_cell(Phase::Macro, "user", "owned")
        .unwrap();
    assert!(
        portable::core_bindings::compile_with_cells(Phase::Runtime, &[foreign])
            .err()
            .unwrap()
            .message
            .contains("foreign phase")
    );
}

#[test]
fn emitted_core_cells_execute_in_both_phases_and_survive_gc() {
    let engine = support::engine();
    for phase in [Phase::Runtime, Phase::Macro] {
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
        let emitted = portable::core_bindings::compile(phase).unwrap();
        runtime_abi::verify_artifact(&emitted.wasm, &runtime_abi::Manifest::default()).unwrap();
        portable::artifact_identity::verify(
            &emitted.wasm,
            portable::artifact_identity::Expected {
                phase: Some(phase),
                macro_dependencies: Some(&[]),
                ..Default::default()
            },
        )
        .unwrap();
        let initialized = linker
            .instantiate(&mut store, &Module::new(&engine, &emitted.wasm).unwrap())
            .unwrap();
        for identity in &emitted.cells {
            let global = initialized
                .get_global(&mut store, &identity.import_name())
                .unwrap();
            linker
                .define(
                    &store,
                    identity.import_module(),
                    &identity.import_name(),
                    global,
                )
                .unwrap();
            let mut bound = [Val::null_any_ref()];
            let cell = global.get(&mut store);
            runtime
                .get_func(&mut store, "binding-bound")
                .unwrap()
                .call(&mut store, &[cell], &mut bound)
                .unwrap();
            assert_eq!(
                bound[0]
                    .unwrap_anyref()
                    .unwrap()
                    .as_i31(&store)
                    .unwrap()
                    .unwrap()
                    .get_i32(),
                4,
                "uninitialized {identity:?}"
            );
        }
        store.gc(None).unwrap();
        for (source, expected) in [
            ("(let [sum +] (sum 19 23))", 42.0_f64),
            (
                "(let [shift unsigned-bit-shift-right] (shift -1 1))",
                2147483647.0,
            ),
            (
                "(let [shift bit-shift-right-zero-fill] (shift -1 1))",
                2147483647.0,
            ),
            (
                "(if (identical? unsigned-bit-shift-right bit-shift-right-zero-fill) 1 2)",
                2.0,
            ),
            ("(let [bits bit-and] (bits 15 7 3))", 3.0),
            ("(let [bits bit-or] (bits 1 2 4))", 7.0),
            ("(let [bits bit-xor] (bits 1 3 7))", 5.0),
            ("(let [bits bit-and-not] (bits 15 1 2))", 12.0),
            ("(ex-data (ex-info \"retained\" 17))", 17.0),
            ("(ex-cause (ex-info \"retained\" nil 23))", 23.0),
            (
                "(if (instance? ExceptionInfo (ex-info \"retained\" nil)) 1 2)",
                1.0,
            ),
        ] {
            let wasm = portable::compile_in(source, &Environment::default(), phase).unwrap();
            let fragment = linker
                .instantiate(&mut store, &Module::new(&engine, wasm).unwrap())
                .unwrap();
            let mut result = [Val::null_any_ref()];
            fragment
                .get_func(&mut store, "eval")
                .unwrap()
                .call(&mut store, &[], &mut result)
                .unwrap();
            let number = result[0]
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)
                .unwrap()
                .unwrap();
            let fields = number.fields(&mut store).unwrap().collect::<Vec<_>>();
            assert!(
                matches!(fields.as_slice(), [Val::F64(bits)] if *bits == expected.to_bits()),
                "{phase:?} {source}: {fields:?}"
            );
        }
        // A phase's cells cannot resolve imports for an artifact of the other phase.
        let other = if phase == Phase::Runtime {
            Phase::Macro
        } else {
            Phase::Runtime
        };
        let wasm =
            portable::compile_in("(let [sum +] (sum 19 23))", &Environment::default(), other)
                .unwrap();
        assert!(
            linker
                .instantiate(&mut store, &Module::new(&engine, wasm).unwrap())
                .is_err()
        );
    }
}

#[test]
fn emitted_core_cells_embed_and_initialize_in_components() {
    use wasm_encoder::{ComponentSectionId, InstanceSection, ModuleArg, RawSection};
    let engine = support::engine();
    for phase in [Phase::Runtime, Phase::Macro] {
        let emitted = portable::core_bindings::compile(phase).unwrap();
        let mut component = wasm_encoder::Component::new();
        for bytes in [runtime_abi::module(), emitted.wasm] {
            component.section(&RawSection {
                id: ComponentSectionId::CoreModule.into(),
                data: &bytes,
            });
        }
        let mut instances = InstanceSection::new();
        instances.instantiate(0, std::iter::empty::<(&str, ModuleArg)>());
        instances.instantiate(1, [("suss.runtime", ModuleArg::Instance(0))]);
        component.section(&instances);
        let component = wasmtime::component::Component::new(&engine, component.finish()).unwrap();
        let mut store = Store::new(&engine, ());
        wasmtime::component::Linker::new(&engine)
            .instantiate(&mut store, &component)
            .unwrap();
        store.gc(None).unwrap();
    }
}
