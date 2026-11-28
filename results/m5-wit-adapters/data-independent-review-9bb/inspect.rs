use wasmtime::{Config, Engine, Store, component::{Component,Linker,Val}};
fn main()->wasmtime::Result<()> {
 let mut config=Config::new();
 config.wasm_gc(true).wasm_function_references(true).wasm_tail_call(true).wasm_exceptions(true).wasm_component_model(true).wasm_component_model_fixed_length_lists(true).consume_fuel(true).cranelift_opt_level(wasmtime::OptLevel::None);
 let engine=Engine::new(&config)?;
 let component=Component::from_file(&engine,"target/probe/review-524/fixed-packed.wasm")?;
 let mut store=Store::new(&engine,());store.set_fuel(100_000_000)?;
 let instance=Linker::new(&engine).instantiate(&mut store,&component)?;
 let f=instance.get_func(&mut store,"run").unwrap();
 let result=f.call(&mut store,&[],&mut[Val::Bool(false)]);
 println!("result={result:?}");
 if let Some(exception)=store.take_pending_exception(){
  let fields=exception.fields(&mut store)?.collect::<Vec<_>>();
  println!("fields={fields:?}");
  let string=fields[0].unwrap_anyref().unwrap().as_array(&store)?.unwrap();
  let units=string.elems(&mut store)?.map(|v|v.unwrap_i32()as u16).collect::<Vec<_>>();
  println!("payload={}",String::from_utf16_lossy(&units));
 }
 Ok(())
}
