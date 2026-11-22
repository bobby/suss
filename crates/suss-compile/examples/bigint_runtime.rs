//! Production-wrapper instruction/allocation probe; serialized modules are local only.
use std::{env,fs,path::Path};
use suss_compile::runtime_abi as abi;
use wasm_encoder::*;
use wasmtime::{Collector,Config,Engine,Instance,Linker,Module,Store,Val};
const VALUE: ValType = ValType::Ref(RefType::EQREF);
fn driver() -> Vec<u8> {
    use Instruction::*;
    let mut types=abi::prelude();
    let mut imports=ImportSection::new();
    types.ty().function([VALUE,VALUE],[VALUE]);
    for name in ["bigint-multiply-gc","bigint-multiply-helper","bigint-multiply"] {
        imports.import("runtime",name,EntityType::Function(abi::TYPE_COUNT));
    }
    types.ty().function([VALUE],[ValType::I64]);
    imports.import("runtime","bigint-low-long",EntityType::Function(abi::TYPE_COUNT+1));
    types.ty().function([ValType::I32,ValType::Ref(RefType { nullable: true, heap_type: HeapType::Concrete(abi::PACKED_STORAGE+2) }),ValType::I32],[VALUE]);
    imports.import("runtime","bigint-new",EntityType::Function(abi::TYPE_COUNT+2));
    types.ty().function([VALUE,VALUE,ValType::I32],[ValType::I64]);
    let mut funcs=FunctionSection::new();let mut code=CodeSection::new();let mut exports=ExportSection::new();
    let mut globals=GlobalSection::new();
    globals.global(GlobalType{val_type:VALUE,mutable:true,shared:false},&ConstExpr::ref_null(HeapType::Abstract{shared:false,ty:AbstractHeapType::Eq}));
    exports.export("sink",ExportKind::Global,0);
    for (choice,name) in ["gc","helper","dispatch","control","allocate"].iter().enumerate() {
        funcs.function(abi::TYPE_COUNT+3);exports.export(name,ExportKind::Func,5+choice as u32);
        let mut body=Function::new([(1,ValType::I32),(1,ValType::I64),(1,VALUE),(1,ValType::I32)]);
        let mut ops=vec![Block(BlockType::Empty),Loop(BlockType::Empty),LocalGet(3),LocalGet(2),I32GeU,BrIf(1)];
        if choice<3 {ops.extend([LocalGet(0),LocalGet(1),Call(choice as u32)]);}
        else if choice==3 {ops.push(LocalGet(0));}
        else {
            for local in [0,1] {ops.extend([LocalGet(local),RefCastNonNull(HeapType::Concrete(abi::BIGINT)),StructGet{struct_type_index:abi::BIGINT,field_index:2}]);}
            ops.extend([I32Add,LocalSet(6),I32Const(1),I32Const(1),LocalGet(6),ArrayNew(abi::PACKED_STORAGE+2),LocalGet(6),Call(4)]);
        }
        ops.extend([LocalSet(5),LocalGet(4),LocalGet(5),Call(3),I64Add,LocalSet(4),LocalGet(3),I32Const(1),I32Add,LocalSet(3),Br(0),End,End,LocalGet(5),GlobalSet(0),LocalGet(4),End]);
        for op in ops {body.instruction(&op);}code.function(&body);
    }
    let mut module=wasm_encoder::Module::new();module.section(&types).section(&imports).section(&funcs).section(&globals).section(&exports).section(&code);module.finish()
}
fn call(store:&mut Store<()>,runtime:&Instance,name:&str,args:&[Val])->Val {
    let mut out=[Val::null_any_ref()];runtime.get_func(&mut *store,name).unwrap().call(&mut *store,args,&mut out).unwrap();out[0].clone()
}
fn integer(store:&mut Store<()>,runtime:&Instance,words:&[u32])->Val {
    let value=call(store,runtime,"bigint-limbs-new",&[Val::I32(words.len() as i32)]);
    let array=value.unwrap_anyref().unwrap().as_array(&*store).unwrap().unwrap();
    for (i,&word) in words.iter().enumerate(){array.set(&mut *store,i as u32,Val::I32(word as i32)).unwrap();}
    call(store,runtime,"bigint-new",&[Val::I32(1),value,Val::I32(words.len() as i32)])
}
fn words(v:&serde_json::Value)->Vec<u32>{v.as_array().unwrap().iter().map(|v|u32::try_from(v.as_u64().unwrap()).unwrap()).collect()}
fn main()->Result<(),Box<dyn std::error::Error>> {
    let args=env::args().collect::<Vec<_>>();assert!(args.len()>=4,"prepare|run DIRECTORY auto|null [CASES INDEX CANDIDATE COUNT]");
    let folder=Path::new(&args[2]);let collector=&args[3];
    let mut config=Config::new();config.wasm_gc(true).wasm_function_references(true).wasm_tail_call(true).wasm_exceptions(true).collector(match collector.as_str(){"auto"=>Collector::Auto,"null"=>Collector::Null,_=>panic!("collector")});
    let engine=Engine::new(&config)?;
    let load=|name:&str|->Result<Module,Box<dyn std::error::Error>> {
        let path=folder.join(format!("{name}-{collector}.cwasm"));
        if args[1]=="prepare" {
            let bytes=if name=="runtime" {abi::module()}else{driver()};
            fs::write(folder.join(format!("{name}.wasm")),&bytes)?;
            let module=Module::new(&engine,&bytes)?;fs::write(path,module.serialize()?)?;Ok(module)
        } else {
            // Trusted only when produced locally by this executable's prepare
            // command and checked against the measurement driver's hashes.
            Ok(unsafe{Module::deserialize_file(&engine,path)?})
        }
    };
    let runtime_module=load("runtime")?;let loop_module=load("loop")?;
    if args[1]=="prepare" {println!("prepared {collector}");return Ok(());}
    let cases:serde_json::Value=serde_json::from_slice(&fs::read(&args[4])?)?;
    let case=&cases[args[5].parse::<usize>()?];let candidate=&args[6];let count=args[7].parse::<i32>()?;assert!(count>0);
    let (a,b)=(words(&case["a"]),words(&case["b"]));
    let mut expected=if candidate=="control" {a.clone()} else if candidate=="allocate" {vec![1;a.len()+b.len()]}else{words(&case["product"])};
    let used=expected.len();let cap=if candidate=="control"{a.len()}else{a.len()+b.len()};
    let low=expected.first().copied().unwrap_or(0) as u64 | ((expected.get(1).copied().unwrap_or(0) as u64)<<32);
    expected.resize(cap,0);
    let mut store=Store::new(&engine,());let runtime=Instance::new(&mut store,&runtime_module,&[])?;
    let mut linker=Linker::new(&engine);linker.instance(&mut store,"runtime",runtime)?;let loops=linker.instantiate(&mut store,&loop_module)?;
    let a=integer(&mut store,&runtime,&a);let b=integer(&mut store,&runtime,&b);let mut out=[Val::I64(0)];
    loops.get_func(&mut store,candidate).unwrap().call(&mut store,&[a,b,Val::I32(count)],&mut out)?;
    assert_eq!(out[0].unwrap_i64(),(low as i64).wrapping_mul(i64::from(count)));
    let sink=loops.get_global(&mut store,"sink").unwrap().get(&mut store);let object=sink.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
    assert_eq!(object.field(&mut store,0)?.unwrap_i32(),i32::from(used>0));assert_eq!(object.field(&mut store,2)?.unwrap_i32() as usize,used);assert_eq!(object.field(&mut store,3)?.unwrap_i32(),0);
    let storage=object.field(&mut store,1)?;let storage=storage.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
    assert_eq!(storage.elems(&mut store)?.map(|v|v.unwrap_i32() as u32).collect::<Vec<_>>(),expected);
    println!("{}",out[0].unwrap_i64());
    let status=fs::read_to_string("/proc/self/status")?;let kib=status.lines().find(|line|line.starts_with("VmHWM:")).unwrap().split_whitespace().nth(1).unwrap().parse::<u64>()?;eprintln!("peak-bytes={}",kib*1024);
    Ok(())
}
