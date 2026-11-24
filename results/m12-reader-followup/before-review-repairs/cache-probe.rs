use suss_compile::{portable::ExpansionHost, portable_macros::CompiledMacros,
    portable_session::{Session,SessionOptions}};
fn main() {
 let root=tempfile::tempdir().unwrap();
 std::fs::write(root.path().join("ledger.sus"),"(ns ledger) (def effects (atom []))").unwrap();
 let a=root.path().join("a.sus");let b=root.path().join("b.sus");
 std::fs::write(&a,"(ns a (:require [ledger])) (swap! ledger/effects conj :a) (def value (fn [] (count @ledger/effects)))").unwrap();
 std::fs::write(&b,"(ns b (:require [a])) (throw (suss.bootstrap/error \"later initializer failed\")) (def value (fn [] (a/value)))").unwrap();
 std::fs::write(root.path().join("app.sus"),"(ns app (:require [b])) (defmacro result [] (b/value)) (def value (result))").unwrap();
 let mut session=Session::new_repl_with_options(SessionOptions{source_paths:vec![root.path().to_owned()],..SessionOptions::default()}).unwrap();
 let mut macros=CompiledMacros::new().unwrap();
 session.load_namespace_with_macros("a",&mut macros).unwrap();
 assert!(session.load_namespace_with_macros("app",&mut macros).is_err());
 assert!(macros.artifact_dependencies().is_none());
 std::fs::write(&a,"(ns a) (throw (suss.bootstrap/error \"successful dependency was replayed\"))").unwrap();
 std::fs::write(&b,"(ns b (:require [a])) (def value (fn [] (a/value)))").unwrap();
 session.load_namespace_with_macros("app",&mut macros).unwrap();
 assert!(macros.artifact_dependencies().is_some(),"source cache identities must recover after successful retry");
}
