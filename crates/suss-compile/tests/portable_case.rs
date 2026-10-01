mod support;
use suss_compile::{
    portable::{
        prepare_fragment,
        resolve::{Environment, Phase},
    },
    runtime_abi,
};
use wasmtime::{Instance, Linker, Module, Store, Val};

#[test]
fn scalar_case_fragments_execute_in_runtime_and_macro_phase() {
    let engine = support::engine();
    for phase in [Phase::Runtime, Phase::Macro] {
        let environment = Environment::default();
        let fragment = prepare_fragment("(cljs.core/case 1 1 17 19)", &environment, phase).unwrap();
        runtime_abi::verify_artifact(&fragment.wasm, &runtime_abi::Manifest::default()).unwrap();
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
        let instance = linker
            .instantiate(&mut store, &Module::new(&engine, &fragment.wasm).unwrap())
            .unwrap();
        let mut value = [Val::null_any_ref()];
        instance
            .get_func(&mut store, "eval")
            .unwrap()
            .call(&mut store, &[], &mut value)
            .unwrap();
        store.gc(None).unwrap();
        let number = value[0]
            .unwrap_anyref()
            .unwrap()
            .as_struct(&store)
            .unwrap()
            .unwrap();
        assert_eq!(
            number.field(&mut store, 0).unwrap().unwrap_f64().to_bits(),
            17.0f64.to_bits()
        );
        assert!(prepare_fragment("(case 1 1 17 1 19 23)", &environment, phase).is_err());
    }
}
