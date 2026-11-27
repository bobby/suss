use suss_compile::{portable_macros::CompiledMacros, portable_repl, portable_session::Session};

fn main() {
    let root = std::env::args().nth(1).expect("source root");
    std::env::set_current_dir(root).unwrap();
    let mut leaked = false;
    for phase in ["Runtime", "Macro"] {
        let mut macros = CompiledMacros::new().unwrap();
        macros.enter_namespace("user").unwrap();
        macros.define("(defmacro ^{:box (atom nil)} holder [] nil)").unwrap();
        macros.define("(defmacro inspect-escaped [] (list 'quote (let [v @(:box (meta (var holder)))] [(var? v) (:doc (meta v)) (try (@v nil nil) (catch :default e :unbound))])))").unwrap();
        let mut session = if phase == "Runtime" { Session::new_repl().unwrap() } else { Session::new_macro().unwrap() };
        let before = portable_repl::evaluate_compiled(&mut session, &mut macros, "(inspect-escaped)").unwrap();
        println!("{phase} before={before}");
        let rejected = "(defmacro ^{:probe (do (reset! (:box (meta (var holder))) (var fresh-stage)) nil)} fresh-stage \"rejected\" [] 99)\n(no-such-source-initializer)\n";
        let result = portable_repl::evaluate_script_compiled(&mut session, &mut macros, rejected, None);
        match &result {
            Ok(value) => println!("{phase} unexpected_success={value}"),
            Err(error) => println!("{phase} rejection={}", portable_repl::repl_error_display(&mut session, &macros, error)),
        }
        let mut fresh = if phase == "Runtime" { Session::new_repl().unwrap() } else { Session::new_macro().unwrap() };
        let after = portable_repl::evaluate_compiled(&mut fresh, &mut macros, "(inspect-escaped)");
        println!("{phase} after={after:?}");
        if result.is_err() && after.as_ref().is_ok_and(|value| value.contains("rejected")) {
            println!("CONFIRMED_FRESH_VAR_METADATA_LEAK {phase}");
            leaked = true;
        }
    }
    if leaked { std::process::exit(42); }
}
