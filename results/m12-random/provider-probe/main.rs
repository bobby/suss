// Feasibility fixture only: typed callback installation and official RNG
// component interop. This does not implement the Suss rand API or error policy.
use wasm_encoder::*;
use wasmtime::{Config, Engine, Func, Instance, Module, Store, Val};
use wasmtime::component::{Component as LoadedComponent, Linker, ResourceTable};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

const RUNTIME: &str = r#"(module
  (type $source (func (result i64)))
  (global $source (mut (ref null $source)) (ref.null $source))
  (func (export "install") (param (ref null $source))
    local.get 0 global.set $source)
  (func (export "sample") (result f64)
    global.get $source ref.as_non_null call_ref $source
    i64.const 11 i64.shr_u f64.convert_i64_u f64.const 0x1p-53 f64.mul))"#;
const WIRE: &str = r#"(module
  (type $source (func (result i64)))
  (import "source" "word" (func $word (type $source)))
  (import "runtime" "install" (func $install (param (ref null $source))))
  (elem declare func $word)
  (func $start ref.func $word call $install)
  (start $start))"#;

fn component() -> Vec<u8> {
    let mut component = wasm_encoder::Component::new();
    let mut types = ComponentTypeSection::new();
    types.defined_type().list(PrimitiveValType::U8); //0
    types.function().params([("max-len", ComponentValType::Primitive(PrimitiveValType::U64))])
        .result(Some(ComponentValType::Type(0))); //1
    types.function().params(std::iter::empty::<(&str, ComponentValType)>())
        .result(Some(ComponentValType::Primitive(PrimitiveValType::U64))); //2
    let mut provider = InstanceType::new();
    provider.alias(Alias::Outer { kind: ComponentOuterAliasKind::Type, count: 1, index: 1 });
    provider.alias(Alias::Outer { kind: ComponentOuterAliasKind::Type, count: 1, index: 2 });
    provider.export("get-random-bytes", ComponentTypeRef::Func(0));
    provider.export("get-random-u64", ComponentTypeRef::Func(1));
    types.instance(&provider); //3
    types.function().params(std::iter::empty::<(&str, ComponentValType)>())
        .result(Some(ComponentValType::Primitive(PrimitiveValType::F64))); //4
    component.section(&types);
    let mut imports = ComponentImportSection::new();
    imports.import("wasi:random/random@0.3.1", ComponentTypeRef::Instance(3));
    component.section(&imports);
    let mut aliases = ComponentAliasSection::new();
    aliases.alias(Alias::InstanceExport { instance: 0, kind: ComponentExportKind::Func,
        name: "get-random-u64" });
    component.section(&aliases);
    let mut canonical = CanonicalFunctionSection::new();
    canonical.lower(0, std::iter::empty::<CanonicalOption>());
    component.section(&canonical);
    for text in [RUNTIME, WIRE] {
        let module = wat::parse_str(text).unwrap();
        component.section(&RawSection { id: ComponentSectionId::CoreModule.into(), data: &module });
    }
    let mut instances = InstanceSection::new();
    instances.export_items([("word", ExportKind::Func, 0)]); //0
    instances.instantiate(0, std::iter::empty::<(&str, ModuleArg)>()); //1
    instances.instantiate(1, [("source", ModuleArg::Instance(0)),
                              ("runtime", ModuleArg::Instance(1))]); //2
    component.section(&instances);
    let mut aliases = ComponentAliasSection::new();
    aliases.alias(Alias::CoreInstanceExport { instance: 1, kind: ExportKind::Func, name: "sample" });
    component.section(&aliases);
    let mut canonical = CanonicalFunctionSection::new();
    canonical.lift(1, 4, std::iter::empty::<CanonicalOption>());
    component.section(&canonical);
    let mut exports = ComponentExportSection::new();
    exports.export("sample", ComponentExportKind::Func, 1, None);
    component.section(&exports);
    component.finish()
}
struct State { ctx: WasiCtx, table: ResourceTable }
impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> { WasiCtxView { ctx: &mut self.ctx, table: &mut self.table } }
}
fn main() {
    let mut config = Config::new();
    config.wasm_component_model(true).wasm_component_model_async(true)
        .wasm_gc(true).wasm_function_references(true);
    let engine = Engine::new(&config).unwrap();
    let runtime = Module::new(&engine, RUNTIME).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &runtime, &[]).unwrap();
    let install = instance.get_func(&mut store, "install").unwrap();
    let sample = instance.get_typed_func::<(), f64>(&mut store, "sample").unwrap();
    let mut bits = Vec::new();
    for word in [0_u64, 2048, 1_u64 << 63, u64::MAX] {
        let source = Func::wrap(&mut store, move || word as i64);
        install.call(&mut store, &[Val::FuncRef(Some(source))], &mut []).unwrap();
        bits.push(sample.call(&mut store, ()).unwrap().to_bits());
    }
    assert_eq!(bits, [0, 0x3ca0000000000000, 0x3fe0000000000000, 0x3fefffffffffffff]);
    println!("controlled_native_bits={bits:?}");
    let bytes = component();
    let loaded = LoadedComponent::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, State { ctx: WasiCtxBuilder::new().build(), table: ResourceTable::new() });
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::p3::add_to_linker(&mut linker).unwrap();
    let instance = linker.instantiate(&mut store, &loaded).unwrap();
    let sample = instance.get_typed_func::<(), (f64,)>(&mut store, "sample").unwrap();
    let mut observed = Vec::new();
    for _ in 0..16 {
        let (value,) = sample.call(&mut store, ()).unwrap();
        assert!(value >= 0.0 && value < 1.0);
        observed.push(value.to_bits());
    }
    assert!(observed.iter().copied().collect::<std::collections::BTreeSet<_>>().len() > 1);
    println!("component_real_sample_bits={observed:?}");
    println!("component_bytes={}", bytes.len());
}
