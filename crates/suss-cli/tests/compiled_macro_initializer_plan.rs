//! Authored execution of immutable definition/initializer transport sharing.
#[path = "support/compilation_fuel.rs"]
mod compilation_fuel;
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};

#[test]
fn definition_initializer_plan_preserves_metadata_quote_barriers_and_source_values() {
    const OBSERVE: &str = r#"(defmacro inspect-initializer-plan [name]
      (let [info (get (:defs (:ns &env)) name)
            definition (:suss/definition-form info)
            initializer (:suss/initializer-form info)
            child (last definition)]
        (list 'quote [(identical? child initializer)
                      (identical? (meta child) (meta initializer))
                      (= (:doc (:meta info)) "plan fixture")
                      (= (:inner (meta initializer)) 7)
                      (= (nth initializer 0) 17)
                      (= (:a (nth initializer 1)) 1)])))"#;
    const QUOTED: &str = r#"(defmacro inspect-quoted-initializer-plan []
        (let [info (get (:defs (:ns &env)) 'plan-quoted)
              definition (:suss/definition-form info)
              initializer (:suss/initializer-form info)]
          (list 'quote [(identical? (last definition) initializer)
                        (= (first initializer) 'quote)
                        (= (first (nth initializer 1)) 'unresolved-plan-name)])))"#;
    for mut caller in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        // Expansion is compilation of the caller; its Runtime keeps defaults.
        macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
        macros.define(OBSERVE).unwrap();
        macros.define(QUOTED).unwrap();
        caller.eval("(def plan-effects 0) (def plan-effect-target (do (set! plan-effects (inc plan-effects)) 23)) (def ^{:doc \"plan fixture\"} plan-target ^{:inner 7} [17 {:a 1}]) (def plan-quoted '(unresolved-plan-name 19))").unwrap();
        caller.collect().unwrap();
        let result = caller.eval_with_macros("[(inspect-initializer-plan plan-target) (inspect-quoted-initializer-plan)]", &mut macros).unwrap();
        caller.collect().unwrap();
        // Exact base 8fa6c3f observation: metadata-bearing reader children
        // retain occurrence identities despite sharing the native recipe.
        assert_eq!(suss_cli::portable_repl::display(&mut caller, &result).unwrap(),
            "[[false false true true true true] [true true true]]");
        let value = caller.eval("[(nth plan-target 0) (:a (nth plan-target 1)) (first plan-quoted) plan-effects plan-effect-target]").unwrap();
        assert_eq!(suss_cli::portable_repl::display(&mut caller, &value).unwrap(),
            "[17 1 unresolved-plan-name 1 23]");
    }
}
