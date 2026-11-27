use suss_compile::{portable_macros::CompiledMacros,portable_session::Session,portable_repl};
use std::sync::{Arc,atomic::{AtomicU64,Ordering}};
fn main(){
 let root=std::env::args().nth(1).unwrap(); std::env::set_current_dir(root).unwrap();
 let mut macros=CompiledMacros::new().unwrap();
 let remaining=Arc::new(AtomicU64::new(u64::MAX)); let callback=remaining.clone();
 macros.install_observation_hook(move |mut store,_| {let n=callback.load(Ordering::SeqCst);if n!=u64::MAX {store.set_fuel(n)?;}Ok(0)}).unwrap();
 let rejected="(defmacro ^{:probe (do (suss.harness.observation/capture 99) nil)} staged-reload \"after\" [] 99)";
 {
 let mut test=Session::new_repl().unwrap();
 let value=portable_repl::evaluate_compiled(&mut test,&mut macros,r#"[(try (+ (suss.bootstrap/evaluate-form "x")) (catch :default e (ex-message e))) (try (* (suss.bootstrap/evaluate-form false)) (catch :default e (ex-message e))) (try (+ (identity "x")) (catch :default e (ex-message e))) (try (* (identity false)) (catch :default e (ex-message e)))]"#);
 println!("EVALUATION_RESULT_TYPE_PROBE {value:?}");
 }
 let mut trials=0;
 for phase in ["Runtime","Macro"] {

 let mut run=|n:u64| -> bool {
   remaining.store(u64::MAX,Ordering::SeqCst);
   macros.set_operation_fuel(400_000_000);
   macros.define("(defmacro staged-reload \"before\" [] 23)").unwrap();
   let mut session=if phase=="Runtime" {Session::new_repl().unwrap()} else {Session::new_macro().unwrap()};
   let before=portable_repl::evaluate_compiled(&mut session,&mut macros,"(let [v (get (ns-interns 'user) 'staged-reload)] [(:doc (meta v)) ((deref v) nil nil)])").unwrap();
   assert_eq!(before,"[\"before\" 23]");
   remaining.store(n,Ordering::SeqCst);
   let result=portable_repl::evaluate_script_compiled(&mut session,&mut macros,rejected,None);
   remaining.store(u64::MAX,Ordering::SeqCst);
   macros.set_operation_fuel(400_000_000);
   let success=result.is_ok();
   let result=match result {Ok(v)=>format!("OK {v}"),Err(e)=>format!("ERR {}",portable_repl::repl_error_display(&mut session,&macros,&e))};
   let mut fresh=if phase=="Runtime" {Session::new_repl().unwrap()} else {Session::new_macro().unwrap()};
   let after=portable_repl::evaluate_compiled(&mut fresh,&mut macros,"(let [v (get (ns-interns 'user) 'staged-reload)] [(:doc (meta v)) ((deref v) nil nil)])");
   println!("phase={phase} fuel={n} before={before:?} result={result:?} after={after:?}");
   trials+=1;
   if !success && after.as_ref().is_ok_and(|v|v.contains("after")) {println!("CONFIRMED_METADATA_LEAK phase={phase} fuel={n} trials={trials}");}
   success
 };
 for n in 8750..8790 {run(n);}
 for _ in 0..3 {run(8768);}
 }
 println!("REFINEMENT_COMPLETE");
}
