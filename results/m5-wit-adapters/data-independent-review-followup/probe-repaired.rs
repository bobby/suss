use suss_compile::Compiler;
use wasmtime::{Config,Engine,Store,component::{Component,Linker,Val}};
fn build(engine:&Engine,label:&str,source:&str,wit:&str)->Component{
 let bytes=Compiler::new().compile(source,wit).unwrap();
 std::fs::write(format!("target/probe/review-524-followup/{label}.wasm"),&bytes).unwrap();
 let component=Component::new(engine,bytes).unwrap();
 let imports=component.component_type().imports(engine).map(|(n,_)|n.to_owned()).collect::<Vec<_>>();
 println!("{label} imports={imports:?}");
 component
}
fn instantiate(engine:&Engine,component:&Component)->(Store<()>,wasmtime::component::Instance){
 let mut store=Store::new(engine,());store.set_fuel(100_000_000).unwrap();
 let instance=Linker::new(engine).instantiate(&mut store,component).unwrap();(store,instance)
}
fn run(engine:&Engine,component:&Component,inputs:&[Val])->Val{
 let(mut store,instance)=instantiate(engine,component);let f=instance.get_func(&mut store,"run").unwrap();
 let mut out=[Val::Bool(false)];f.call(&mut store,inputs,&mut out).unwrap();store.gc(None).unwrap();out[0].clone()
}
fn main(){
 let mut config=Config::new();config.wasm_gc(true).gc_heap_initial_size(8*1024*1024).wasm_function_references(true).wasm_tail_call(true).wasm_exceptions(true).wasm_component_model(true).wasm_component_model_implements(true).wasm_component_model_map(true).wasm_component_model_fixed_length_lists(true).consume_fuel(true).cranelift_opt_level(wasmtime::OptLevel::None);
 let engine=Engine::new(&config).unwrap();
 let leaf=|v|Val::Record(vec![("n".into(),Val::S64(v))]);
 let component=build(&engine,"repeated-provider","(defn ^:export run [x] (update x :n inc))","package test:review-repeat@1.2.3; interface data { record row { n:s64 } } world api { import left:data; import right:data; use data.{row}; export run:func(x:row)->row; }");
 assert_eq!(component.component_type().imports(&engine).count(),3);
 assert_eq!(run(&engine,&component,&[leaf(9007199254740993)]),leaf(9007199254740994));println!("repeated-provider PASS");
 let component=build(&engine,"fixed-packed","(defn ^:export run [] [(byte-array [-1 1 2]) (byte-array [-1 -128 127]) (vector-of :byte -1 -128 127)])","package test:review-fixed-followup; world api { type row = tuple<list<u8,3>,list<s8,3>,list<u8,3>>; export run:func()->row; }");
 assert_eq!(run(&engine,&component,&[]),Val::Tuple(vec![Val::FixedLengthList(vec![Val::U8(255),Val::U8(1),Val::U8(2)]),Val::FixedLengthList(vec![Val::S8(-1),Val::S8(-128),Val::S8(127)]),Val::FixedLengthList(vec![Val::U8(255),Val::U8(128),Val::U8(127)])]));println!("fixed-packed PASS");
 let component=build(&engine,"fixed-negatives","(defn ^:export bad-length [] (byte-array [1 2])) (defn ^:export bad-kind [] (int-array [1 2 3])) (defn ^:export bad-tuple [] (byte-array [1 2 3]))","package test:review-fixed-negatives; world api { type bytes=list<u8,3>; export bad-length:func()->bytes; export bad-kind:func()->bytes; export bad-tuple:func()->tuple<u8,u8,u8>; }");
 for name in ["bad-length","bad-kind","bad-tuple"]{
  let(mut store,instance)=instantiate(&engine,&component);let f=instance.get_func(&mut store,name).unwrap();let error=f.call(&mut store,&[],&mut[Val::Bool(false)]).unwrap_err();
  assert_eq!(error.downcast_ref::<wasmtime::Trap>(),Some(&wasmtime::Trap::UncaughtException));
  let exception=store.take_pending_exception().unwrap();let fields=exception.fields(&mut store).unwrap().collect::<Vec<_>>();let text=fields[0].unwrap_anyref().unwrap().as_array(&store).unwrap().unwrap();let units=text.elems(&mut store).unwrap().map(|v|v.unwrap_i32()as u16).collect::<Vec<_>>();println!("{name} expected rejection: {}",String::from_utf16_lossy(&units));
 }
 println!("ALL INDEPENDENT FOLLOW-UP PROBES PASSED");
}
