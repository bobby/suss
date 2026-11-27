use suss_compile::{portable_macros::CompiledMacros,portable_session::Session,portable_repl};
use std::sync::{Arc,atomic::{AtomicU64,Ordering}};
fn session(macro_phase:bool)->Session{if macro_phase{Session::new_macro().unwrap()}else{Session::new_repl().unwrap()}}
fn main(){
 let root=std::env::args().nth(1).unwrap();std::env::set_current_dir(root).unwrap();
 let mut failures=Vec::new();
 for macro_phase in [false,true]{
 let mut macros=CompiledMacros::new().unwrap();
 let remaining=Arc::new(AtomicU64::new(u64::MAX));let callback=remaining.clone();
 macros.install_observation_hook(move|mut store,_|{let n=callback.load(Ordering::SeqCst);if n!=u64::MAX{store.set_fuel(n)?;}Ok(0)}).unwrap();
 let mut main=session(macro_phase);
 let numeric=portable_repl::evaluate_compiled(&mut main,&mut macros,r#"[(try (+ (suss.bootstrap/evaluate-form "x")) (catch :default e (ex-message e))) (try (* (suss.bootstrap/evaluate-form false)) (catch :default e (ex-message e))) (try (+ (suss.bootstrap/evaluate-form nil)) (catch :default e (ex-message e))) (try (* (suss.bootstrap/evaluate-form [1])) (catch :default e (ex-message e)))]"#).unwrap();
 println!("phase={macro_phase} unary={numeric}");
 if numeric!=r#"["+ requires number operands" "* requires number operands" "+ requires number operands" "* requires number operands"]"#{failures.push(format!("unary phase={macro_phase}: {numeric}"));}
 // Fresh Var escapes in an already completed Atom mutation. Cut guest fuel
 // only after that mutation, then reject any otherwise completed source too.
 macros.define("(defmacro ^{:box (atom nil)} holder [] nil)").unwrap();
 macros.define("(defmacro inspect-escaped [] (list 'quote (let [v @(:box (meta (var holder)))] [(var? v) (:doc (meta v)) (try (@v nil nil) (catch :default e :unbound))])))").unwrap();
 for (i,fuel) in [0,1,100,1000,5000,8750,8760,8761,8768,8789,10000,12000,20000,100000].into_iter().enumerate(){
  remaining.store(u64::MAX,Ordering::SeqCst);macros.set_operation_fuel(200_000_000);
  let name=format!("fresh-cut-{i}");
  let source=format!("(defmacro ^{{:probe (do (reset! (:box (meta (var holder))) (var {name})) (suss.harness.observation/capture 99) nil)}} {name} \"rejected\" [] 99) (no-such-source-initializer)");
  let mut target=session(macro_phase);remaining.store(fuel,Ordering::SeqCst);
  let result=portable_repl::evaluate_script_compiled(&mut target,&mut macros,&source,None);
  remaining.store(u64::MAX,Ordering::SeqCst);macros.set_operation_fuel(200_000_000);
  let error=match result{Ok(v)=>format!("UNEXPECTED_OK {v}"),Err(e)=>portable_repl::repl_error_display(&mut target,&macros,&e)};
  let mut fresh=session(macro_phase);fresh.collect().unwrap();
  let actual=portable_repl::evaluate_compiled(&mut fresh,&mut macros,"(inspect-escaped)").unwrap();
  println!("phase={macro_phase} fresh-fuel={fuel} actual={actual} error={error:?}");
  if actual!="[true nil :unbound]"{failures.push(format!("fresh phase={macro_phase} fuel={fuel}: {actual}"));}
 }
 // A completed nested native root/metadata definition is a language effect;
 // rejection of the enclosing fresh source declaration must preserve it.
 for(i,write)in [
  "(suss.bootstrap/evaluate-form '(def fresh-effect-root \"completed\" (fn [form env] 37)))",
  "(suss.bootstrap/evaluate-form '(defmacro fresh-effect-macro \"completed\" [] 38))"
 ].into_iter().enumerate(){
  let name=if i==0{"fresh-effect-root"}else{"fresh-effect-macro"};
  let source=format!("(defmacro ^{{:probe (do (reset! (:box (meta (var holder))) (var {name})) {write} nil)}} {name} \"rejected\" [] 99) (no-such-source-initializer)");
  let mut target=session(macro_phase);let result=portable_repl::evaluate_script_compiled(&mut target,&mut macros,&source,None);
  let error=match result{Ok(v)=>format!("UNEXPECTED_OK {v}"),Err(e)=>portable_repl::repl_error_display(&mut target,&macros,&e)};
  let mut fresh=session(macro_phase);fresh.collect().unwrap();
  let actual=portable_repl::evaluate_compiled(&mut fresh,&mut macros,"(inspect-escaped)").unwrap();
  let expected=format!("[true \"completed\" {}]",37+i);
  println!("phase={macro_phase} completed-write={name} actual={actual} expected={expected} error={error:?}");
  if actual!=expected{failures.push(format!("completed phase={macro_phase} name={name}: {actual}"));}
 }
 }
 println!("failures={failures:?}");if!failures.is_empty(){std::process::exit(42);}
}
