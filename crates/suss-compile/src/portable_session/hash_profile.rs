//! Diagnostic only. Epoch/entry-biased samples are not exclusive fuel costs.
use std::{cell::{Cell, RefCell}, collections::BTreeMap, sync::atomic::{AtomicU64, Ordering}};
static NEXT: AtomicU64 = AtomicU64::new(0);
static RETAINED: AtomicU64 = AtomicU64::new(0);
thread_local! {
    static DEPTH: Cell<usize> = const { Cell::new(0) };
    static ACTIVE: RefCell<Option<Operation>> = const { RefCell::new(None) };
}
struct Operation { id:u64, target:(String,String), initial:Option<u64>, bins:BTreeMap<u64,(u64,Vec<serde_json::Value>)> }
pub(super) fn enabled()->bool { std::env::var_os("SUSS_HASH_STACK_SAMPLE").is_some() }
pub(super) struct HashScope(bool);
impl HashScope {pub(super) fn new(active:bool)->Self {let active=active&&enabled();if active {DEPTH.with(|x|x.set(x.get()+1));}Self(active)}}
impl Drop for HashScope {fn drop(&mut self){if self.0 {DEPTH.with(|x|x.set(x.get()-1));}}}
pub(crate) struct OperationScope(bool);
impl OperationScope {
    pub(crate) fn new(target:&(String,String))->Self {
        let active=enabled();if active {ACTIVE.with(|x| {assert!(x.borrow().is_none(),"nested diagnostic operation");*x.borrow_mut()=Some(Operation{id:NEXT.fetch_add(1,Ordering::Relaxed),target:target.clone(),initial:None,bins:BTreeMap::new()});});}Self(active)
    }
}
impl Drop for OperationScope {fn drop(&mut self){if self.0 {ACTIVE.with(|x|{if let Some(op)=x.borrow_mut().take(){
    eprintln!("hash-stack-operation {}",serde_json::json!({"operation":op.id,"target":op.target,"hash_depth_on_exit":DEPTH.with(Cell::get),"bins":op.bins.into_iter().map(|(bin,(seen,samples))|serde_json::json!({"bin":bin,"seen":seen,"samples":samples})).collect::<Vec<_>>()}));
}});}}}
pub(super) fn sample(store:&wasmtime::StoreContextMut<'_,()>) {
    if DEPTH.with(Cell::get)==0{return;}
    ACTIVE.with(|x| {let mut state=x.borrow_mut();let Some(op)=state.as_mut() else{return;};
        let Ok(fuel)=store.get_fuel() else{return;};
        // Fixed 156250-fuel strata span the unchanged 10M allowance. Reservoir
        // replacement continues throughout each stratum, not just first entries.
        let initial=*op.initial.get_or_insert(fuel);
        let bin=(initial.saturating_sub(fuel)/156250).min(63);
        let (seen,rows)=op.bins.entry(bin).or_default();*seen+=1;
        let slot=if rows.len()<8 {
            if RETAINED.fetch_update(Ordering::Relaxed,Ordering::Relaxed,|n|(n<4096).then_some(n+1)).is_err(){return;} rows.len()
        } else {
            // Deterministic pseudo-random reservoir selection; not statistical
            // independence from epoch scheduling or guest entry safepoints.
            let mut z=seen.wrapping_add(op.id.wrapping_mul(0x9e3779b97f4a7c15));
            z=(z^(z>>30)).wrapping_mul(0xbf58476d1ce4e5b9);z=(z^(z>>27)).wrapping_mul(0x94d049bb133111eb);z^=z>>31;
            let slot=(z%*seen) as usize;if slot>=8{return;}slot
        };
        let trace=wasmtime::WasmBacktrace::force_capture(store);
        let frames=trace.frames().iter().map(|f|serde_json::json!({"name":f.func_name(),"function":f.func_index(),"module_offset":f.module_offset(),"function_offset":f.func_offset()})).collect::<Vec<_>>();
        let row=serde_json::json!({"operation":op.id,"target":op.target,"fuel":fuel,"bin":bin,"eligible_tick":seen,"hash_depth":DEPTH.with(Cell::get),"frames":frames});
        if slot==rows.len(){rows.push(row);}else{rows[slot]=row;}
    });
}
