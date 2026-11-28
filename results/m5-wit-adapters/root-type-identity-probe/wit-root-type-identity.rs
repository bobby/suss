use wasm_encoder::*;
fn main() {
 for staged in [false,true] {
  let mut c=Component::new();
  let mut types=ComponentTypeSection::new(); types.defined_type().flags(["read","write"]); c.section(&types);
  let mut ex=ComponentExportSection::new(); ex.export("permissions", ComponentExportKind::Type,0,None); c.section(&ex);
  let mut types=ComponentTypeSection::new(); types.defined_type().record([("permissions",ComponentValType::Type(if staged{1}else{0}))]);c.section(&types);
  let mut ex=ComponentExportSection::new();ex.export("row",ComponentExportKind::Type,2,None);c.section(&ex);
  let mut types=ComponentTypeSection::new();types.function().params([("x",ComponentValType::Type(if staged{3}else{2}))]).result(Some(ComponentValType::Type(if staged{3}else{2})));c.section(&types);
  let mut m=Module::new();let mut ct=TypeSection::new();ct.ty().function([ValType::I32],[ValType::I32]);let mut fs=FunctionSection::new();fs.function(0);let mut e=ExportSection::new();e.export("echo",ExportKind::Func,0);let mut code=CodeSection::new();let mut f=Function::new([]);f.instruction(&Instruction::LocalGet(0)).instruction(&Instruction::End);code.function(&f);m.section(&ct).section(&fs).section(&e).section(&code);c.section(&ModuleSection(&m));
  let mut instances=InstanceSection::new();instances.instantiate(0,std::iter::empty::<(&str,ModuleArg)>());c.section(&instances);let mut aliases=ComponentAliasSection::new();aliases.alias(Alias::CoreInstanceExport{instance:0,kind:ExportKind::Func,name:"echo"});c.section(&aliases);let mut canon=CanonicalFunctionSection::new();canon.lift(0,4,[]);c.section(&canon);let mut ex=ComponentExportSection::new();ex.export("echo",ComponentExportKind::Func,0,None);c.section(&ex);
  let b=c.finish();let result=wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all()).validate_all(&b).map(|_|());println!("staged={staged}: {result:?}");
  if staged {result.unwrap();let engine=wasmtime::Engine::default();let comp=wasmtime::component::Component::new(&engine,b).unwrap();let mut store=wasmtime::Store::new(&engine,());let i=wasmtime::component::Linker::new(&engine).instantiate(&mut store,&comp).unwrap();let echo=i.get_func(&mut store,"echo").unwrap();let input=wasmtime::component::Val::Record(vec![("permissions".into(),wasmtime::component::Val::Flags(vec!["write".into()]))]);let mut output=[wasmtime::component::Val::Bool(false)];echo.call(&mut store,&[input.clone()],&mut output).unwrap();assert_eq!(output[0],input);println!("actual canonical execution {output:?}");}
 }
}
