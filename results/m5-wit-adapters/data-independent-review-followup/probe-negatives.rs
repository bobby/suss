use wasmtime::{Config,Engine,Store,component::{Component,Linker,Val}};
fn instantiate(engine:&Engine,component:&Component)->(Store<()>,wasmtime::component::Instance){
 let mut store=Store::new(engine,());store.set_fuel(100_000_000).unwrap();
 let instance=Linker::new(engine).instantiate(&mut store,component).unwrap();(store,instance)
}
fn main(){
 let mut config=Config::new();config.wasm_gc(true).gc_heap_initial_size(8*1024*1024).wasm_function_references(true).wasm_tail_call(true).wasm_exceptions(true).wasm_component_model(true).wasm_component_model_implements(true).wasm_component_model_map(true).wasm_component_model_fixed_length_lists(true).consume_fuel(true).cranelift_opt_level(wasmtime::OptLevel::None);
 let engine=Engine::new(&config).unwrap();
 let component=Component::new(&engine,std::fs::read("target/probe/review-524-followup/fixed-negatives.wasm").unwrap()).unwrap();
 for name in ["bad-length","bad-kind","bad-tuple"]{
  let(mut store,instance)=instantiate(&engine,&component);let f=instance.get_func(&mut store,name).unwrap();let error=f.call(&mut store,&[],&mut[Val::Bool(false)]).unwrap_err();
  assert_eq!(error.downcast_ref::<wasmtime::Trap>(),Some(&wasmtime::Trap::UncaughtException));
  let exception=store.take_pending_exception().unwrap();let fields=exception.fields(&mut store).unwrap().collect::<Vec<_>>();let payload=fields[0].unwrap_anyref().unwrap();let text=if let Some(array)=payload.as_array(&store).unwrap(){array}else{payload.as_struct(&store).unwrap().unwrap().field(&mut store,1).unwrap().unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap()};let units=text.elems(&mut store).unwrap().map(|v|v.unwrap_i32()as u16).collect::<Vec<_>>();println!("{name} expected rejection: {}",String::from_utf16_lossy(&units));
 }
 println!("ALL THREE INDEPENDENT NEGATIVE CONTROLS PASSED");
}
