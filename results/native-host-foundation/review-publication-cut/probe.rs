use suss_compile::{portable_macros::CompiledMacros,portable_session::Session,portable_repl};
use std::sync::{Arc,atomic::{AtomicU64,Ordering}};
fn main(){
 let root=std::env::args().nth(1).unwrap(); std::env::set_current_dir(root).unwrap();
 let mut macros=CompiledMacros::new().unwrap();
 let remaining=Arc::new(AtomicU64::new(u64::MAX)); let callback=remaining.clone();
 macros.install_observation_hook(move |mut store,_| {let n=callback.load(Ordering::SeqCst);if n!=u64::MAX {store.set_fuel(n)?;}Ok(0)}).unwrap();
 let rejected="(defmacro ^{:probe (do (suss.harness.observation/capture 99) nil)} staged-reload \"after\" [] 99)";
 let mut trials=0;
 let mut run=|n:u64| -> bool {
   remaining.store(u64::MAX,Ordering::SeqCst);
   macros.set_operation_fuel(400_000_000);
   macros.define("(defmacro staged-reload \"before\" [] 23)").unwrap();
   let mut session=Session::new_repl().unwrap();
   let before=portable_repl::evaluate_compiled(&mut session,&mut macros,"(let [v (get (ns-interns 'user) 'staged-reload)] [(:doc (meta v)) ((deref v) nil nil)])").unwrap();
   assert_eq!(before,"[\"before\" 23]");
   remaining.store(n,Ordering::SeqCst);
   let result=portable_repl::evaluate_script_compiled(&mut session,&mut macros,rejected,None);
   remaining.store(u64::MAX,Ordering::SeqCst);
   macros.set_operation_fuel(400_000_000);
   let success=result.is_ok();
   let result=match result {Ok(v)=>format!("OK {v}"),Err(e)=>format!("ERR {}",portable_repl::repl_error_display(&mut session,&macros,&e))};
   let mut fresh=Session::new_repl().unwrap();
   let after=portable_repl::evaluate_compiled(&mut fresh,&mut macros,"(let [v (get (ns-interns 'user) 'staged-reload)] [(:doc (meta v)) ((deref v) nil nil)])");
   println!("fuel={n} before={before:?} result={result:?} after={after:?}");
   trials+=1;
   if !success && after.as_ref().is_ok_and(|v|v.contains("after")) {println!("CONFIRMED_METADATA_LEAK fuel={n} trials={trials}"); std::process::exit(42);}
   success
 };
 let mut high=128;
 while !run(high) {high*=2; assert!(high<=1_048_576);}
 let mut low=high/2;
 while high-low>1 {let middle=low+(high-low)/2;if run(middle){high=middle;}else{low=middle;}}
 println!("first_success={high} scanning_last_256");
 for n in high.saturating_sub(256)..high {run(n);}
 println!("NO_METADATA_LEAK_FOUND");
}
