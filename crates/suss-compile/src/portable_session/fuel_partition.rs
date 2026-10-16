//! Diagnostic native accounting only. No additional guest calls or charge changes.
use std::{cell::RefCell,collections::BTreeMap};
use wasmtime::{AsContext, StoreContextMut};
use serde_json::{json,Value};
#[derive(Default)]
struct State {last:Option<u64>, mode:&'static str, stack:Vec<&'static str>, counts:BTreeMap<String,u128>, previous:BTreeMap<String,u128>}
#[derive(Default)]
struct Ledger {keys:BTreeMap<usize,u64>,states:BTreeMap<u64,State>, sequence:u64}
thread_local! {static LEDGER:RefCell<Ledger>=RefCell::new(Ledger::default());}
fn enabled()->bool {std::env::var_os("SUSS_FUEL_PARTITION").is_some()}
fn key(data:&())->usize {data as *const () as usize}
fn add(s:&mut State,name:impl Into<String>,n:u128){*s.counts.entry(name.into()).or_default()+=n;}
fn observe(s:&mut State,fuel:Option<u64>){
 if let (Some(before),Some(after))=(s.last,fuel){
  if let Some(debit)=before.checked_sub(after){add(s,format!("vm_debit:{}",s.mode),debit as u128);}
  else{add(s,"unknown_unwrapped_credit",(after-before) as u128);}
 } else if fuel.is_none(){add(s,"unknown_fuel_read",1);}
 s.last=fuel;
}
pub(super) fn register(store:&wasmtime::Store<()>,id:u64){if !enabled(){return;}LEDGER.with(|l|{let mut l=l.borrow_mut();l.keys.insert(key(store.data()),id);l.states.insert(id,State{last:store.get_fuel().ok(),mode:"initial",..State::default()});});eprintln!("fuel-ledger-register {}",json!({"session":id,"store_data_address":key(store.data())}));}
pub(super) fn set(store:&mut StoreContextMut<'_,()>,fuel:u64,label:&'static str)->wasmtime::Result<()> {
 if !enabled(){return store.set_fuel(fuel);}
 let pointer=key(store.data());let before=store.get_fuel().ok();let result=store.set_fuel(fuel);let after=store.get_fuel().ok();
 LEDGER.with(|l|{let mut l=l.borrow_mut();let Some(id)=l.keys.get(&pointer).copied()else{eprintln!("fuel-ledger-unknown-store {pointer}");return;};let s=l.states.get_mut(&id).unwrap();observe(s,before);
  if result.is_ok(){
   add(s,format!("set_count:{label}"),1);
   if let (Some(a),Some(b))=(before,after){
    if label=="host-success"{add(s,format!("successful_host_units:{}",s.mode),(a-b)as u128);}
    else if label=="host-exhaust"||label=="host-interrupt"{add(s,format!("failed_host_actual_debit:{}",s.mode),a.saturating_sub(b)as u128);}
    else {if b>=a{add(s,format!("administrative_credit:{label}"),(b-a)as u128);}else{add(s,format!("administrative_debit:{label}"),(a-b)as u128);}}
   }
   match label {"setup-max"=>s.mode="setup-unlimited", "budget-reset"=>s.mode="finite", "restore-open"|"async-open"=>{s.stack.push(s.mode);s.mode=if label=="restore-open"{"restoration-unlimited"}else{"async-recovery-unlimited"};},"restore-close"|"async-close"=>{s.mode=s.stack.pop().unwrap_or("unknown");},_=>{}}
  }else{add(s,"set_errors",1);}
  s.last=after;
 });result
}
fn strings(m:&BTreeMap<String,u128>)->Value {Value::Object(m.iter().map(|(k,v)|(k.clone(),Value::String(v.to_string()))).collect())}
pub(crate) fn stage(session:&super::Session,target:&(String,String),stage:&str){if !enabled(){return;}
 let fuel=session.store.get_fuel().ok();let pointer=key(session.store.data());LEDGER.with(|l|{let mut l=l.borrow_mut();let id=l.keys[&pointer];let sequence=l.sequence;l.sequence+=1;let s=l.states.get_mut(&id).unwrap();observe(s,fuel);let delta=s.counts.iter().map(|(k,v)|(k.clone(),v-s.previous.get(k).copied().unwrap_or(0))).collect::<BTreeMap<_,_>>();s.previous=s.counts.clone();eprintln!("fuel-ledger-stage {}",json!({"sequence":sequence,"session":id,"target":target,"stage":stage,"fuel":fuel,"mode":s.mode,"unlimited_depth":s.stack.len(),"cumulative":strings(&s.counts),"since_previous_stage":strings(&delta)}));});
}
