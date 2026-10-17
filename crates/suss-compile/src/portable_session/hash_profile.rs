//! Temporary stack-sampling diagnostic. No production authorization or receipts.
use std::{cell::{Cell,RefCell},collections::BTreeMap};
thread_local! {
 static HASH_DEPTH:Cell<usize>=const{Cell::new(0)};
 static OPERATION:Cell<bool>=const{Cell::new(false)};
 static SAMPLES:RefCell<BTreeMap<Vec<String>,u64>>=RefCell::new(BTreeMap::new());
}
pub(super) fn enabled()->bool {std::env::var_os("SUSS_HASH_STACK_SAMPLE").is_some()}
pub(super) struct HashScope(bool);
impl HashScope {pub(super) fn new(active:bool)->Self {let active=active&&enabled();if active {HASH_DEPTH.with(|x|x.set(x.get()+1));}Self(active)}}
impl Drop for HashScope {fn drop(&mut self){if self.0 {HASH_DEPTH.with(|x|x.set(x.get()-1));}}}
pub(crate) struct OperationScope {enabled:bool,target:(String,String)}
impl OperationScope {
 pub(crate) fn new(target:&(String,String))->Self {let active=enabled();if active {SAMPLES.with(|x|x.borrow_mut().clear());OPERATION.with(|x|x.set(true));}Self{enabled:active,target:target.clone()}}
}
impl Drop for OperationScope {fn drop(&mut self){if self.enabled {
 OPERATION.with(|x|x.set(false));
 SAMPLES.with(|x|{let rows:Vec<_>=x.borrow().iter().map(|(stack,count)|serde_json::json!({"stack":stack,"count":count})).collect();eprintln!("hash-stack-samples {}",serde_json::json!({"target":self.target,"samples":rows,"hash_depth_on_exit":HASH_DEPTH.with(Cell::get)}));});
}}}
pub(super) fn sample(store:&wasmtime::StoreContextMut<'_,()>) {
 if !OPERATION.with(Cell::get)||HASH_DEPTH.with(Cell::get)==0{return;}
 let trace=wasmtime::WasmBacktrace::force_capture(store);
 let stack:Vec<_>=trace.frames().iter().map(|f|format!("{}#{}@{}",f.func_name().unwrap_or("unnamed"),f.func_index(),f.module_offset().unwrap_or(0))).collect();
 if !stack.is_empty(){SAMPLES.with(|x|*x.borrow_mut().entry(stack).or_default()+=1);}
}
