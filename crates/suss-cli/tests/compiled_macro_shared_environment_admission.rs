//! Source-only new companion; original public namespace/callable assertions stay
//! byte-for-byte unchanged. Explicit Macro-only compilation allowance; caller Runtime
//! and shipped defaults are unchanged.
#[path = "support/compilation_fuel.rs"]
mod compilation_fuel;
use suss_cli::{
    portable_macro_data::FormBridge, portable_macros::CompiledMacros, portable_session::Session,
};
use suss_reader::forms::Kind;
const OBSERVER: &str = r#"(defmacro inspect-shared-multi-ast-env [label]
  (let [raw (get (:defs (:ns &env)) 'multiast)
        gather (fn gather [node]
          (if (and (= (:op node) :local) (= (:form node) 'x))
            [node]
            (reduce (fn [out child-key]
              (let [child (get node child-key)]
                (if (vector? child)
                  (reduce (fn [out item] (into out (gather item))) out child)
                  (into out (gather child))))) [] (:children node))))
        refs (gather (:init raw))
        a (:locals (:env (nth refs 0)))
        b (:locals (:env (nth refs 1)))]
    (defonce saved-multi-ast-locals a)
    (list 'quote [label (= 2 (count refs)) (identical? a b)
      (identical? saved-multi-ast-locals a)
      (identical? saved-multi-ast-locals b)])))"#;
#[test]
fn actual_multi_ast_locals_share_one_projection_cold_and_across_same_revision_requests() {
    let mut failures = Vec::new();
    for (phase, caller) in [
        ("runtime", Session::new_repl()),
        ("macro", Session::new_macro()),
    ] {
        let result: Result<(), String> = (|| {
            let mut caller = caller.map_err(|e| e.to_string())?;
            let mut macros = CompiledMacros::new().map_err(|e| e.to_string())?;
            macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
            caller
                .enter_namespace("shared-env-regression")
                .map_err(|e| e.to_string())?;
            macros
                .enter_namespace("shared-env-regression")
                .map_err(|e| e.to_string())?;
            macros.define(OBSERVER).map_err(|e| e.to_string())?;
            caller
                .eval("(def multiast (fn* [x] (do x x)))")
                .map_err(|e| e.to_string())?;
            for label in ["first", "same-revision-2", "same-revision-3"] {
                caller.collect().map_err(|e| e.to_string())?;
                let input = format!("(inspect-shared-multi-ast-env \"{label}\")");
                let value = caller
                    .eval_with_macros(&input, &mut macros)
                    .map_err(|e| format!("{label}: {e}"))?;
                caller.collect().map_err(|e| e.to_string())?;
                let bridge = FormBridge::new(&mut caller).map_err(|e| e.to_string())?;
                let form = bridge
                    .read(&mut caller, &value, 0..1)
                    .map_err(|e| e.to_string())?;
                eprintln!("SHARED_MULTI_AST_ENV {phase}/{label}: {:?}", form.kind);
                let Kind::Vector(row) = &form.kind else {
                    return Err("missing observation row".into());
                };
                if row.len() != 5
                    || row[0].kind != Kind::String(label.encode_utf16().collect())
                    || !row[1..].iter().all(|field| field.kind == Kind::Bool(true))
                {
                    return Err(format!(
                        "{label}: actual local environments failed identity checks: {row:?}"
                    ));
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            failures.push(format!("{phase}: {error}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
