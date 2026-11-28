use suss_compile::portable::{self, resolve::{Environment, Phase}};
use suss_reader::Symbol;
fn main() {
    let mut resolve = wit_parser::Resolve::new();
    let package = resolve.push_str("api.wit", "package test:named@1.2.3; interface values { type id = u64; type letter = char; echo: func(x: id) -> id; same: func(x: letter) -> letter; } world api { export values; }").unwrap();
    let world = resolve.select_world(&[package], None).unwrap();
    let fragment = portable::prepare_fragment("(def echo (fn [x] x))", &Environment::default(), Phase::Runtime).unwrap();
    let bytes = portable::aot::component(&[fragment], &resolve, world,
        &[("test:named/values@1.2.3#echo".into(), Symbol::new("echo")),
          ("test:named/values@1.2.3#same".into(), Symbol::new("echo"))]).unwrap();
    let mut config = wasmtime::Config::new();
    config.wasm_gc(true).wasm_function_references(true).wasm_tail_call(true).wasm_exceptions(true).wasm_component_model(true);
    let engine = wasmtime::Engine::new(&config).unwrap();
    let component = wasmtime::component::Component::new(&engine, bytes).unwrap();
    let interface = component.get_export_index(None, "test:named/values@1.2.3").unwrap();
    let echo = component.get_export_index(Some(&interface), "echo").unwrap();
    let same = component.get_export_index(Some(&interface), "same").unwrap();
    let mut store = wasmtime::Store::new(&engine, ());
    let instance = wasmtime::component::Linker::new(&engine).instantiate(&mut store, &component).unwrap();
    assert_eq!(instance.get_typed_func::<(u64,), (u64,)>(&mut store, &echo).unwrap().call(&mut store, (u64::MAX,)).unwrap(), (u64::MAX,));
    assert_eq!(instance.get_typed_func::<(char,), (char,)>(&mut store, &same).unwrap().call(&mut store, ('🦀',)).unwrap(), ('🦀',));
}
