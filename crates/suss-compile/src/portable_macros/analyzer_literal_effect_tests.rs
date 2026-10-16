//! Same production macro expansion in both caller phases, with a genuine
//! delegated IHash replacement installed before caller environment transport.
use super::*;
use crate::portable_session::memo_leaf_test_support::Method;
#[test]
fn analyzer_literal_actual_env_pre_dispatch_memo_effects_survive_gc_and_restoration() {
    let mut failures = Vec::new();
    for (phase, mut caller) in [
        ("runtime", Session::new_repl().unwrap()),
        ("macro", Session::new_macro().unwrap()),
    ] {
        let mut macros = CompiledMacros::new().unwrap();
        let run = (|| -> Result<(), String> {
            caller
                .eval("(def ^{:audit-leaf :source-only-leaf} watched 17)")
                .map_err(|e| e.to_string())?;
            macros
                .session
                .eval("(def leaf-original-method nil) (def literal-events [])")
                .map_err(|e| e.to_string())?;
            let original =
                Method::capture(&mut macros.session, "Keyword").map_err(|e| e.to_string())?;
            original
                .bind_delegate(&mut macros.session)
                .map_err(|e| e.to_string())?;
            macros.define(r#"(defmacro inspect-literal-effects [stage source-kw source-sym]
              (let [find-key (fn [m text] (reduce (fn [found key] (if (= (str key) text) key found)) nil (keys m)))
                    descriptor (get (:locals &env) 'local-x)
                    op-key (find-key descriptor ":op")
                    source-meta (:meta (get (:defs (:ns &env)) 'watched))
                    hits (reduce (fn [out row] (if (identical? op-key (first row)) (conj out (second row)) out)) [] literal-events)]
                (list 'quote
                  [(= (.-_hash op-key) -1882987955)
                   (if (= stage 0) (and (pos? (count hits)) (every? false? hits)) (empty? hits))
                   (nil? (.-_hash source-kw)) (nil? (.-_hash source-sym))
                   (nil? (.-_hash (:audit-leaf source-meta)))
                   (= 17 (:val (:init descriptor))) (= source-kw :op)
                   (= "op" (str source-sym))])))"#).map_err(|e|e.to_string())?;
            macros
                .session
                .eval(
                    r#"(extend-type Keyword IHash
              (-hash [self]
                (do (if (= (.-name self) "op")
                      (set! literal-events (conj literal-events [self (nil? (.-_hash self))])) nil)
                    (leaf-original-method self))))"#,
                )
                .map_err(|e| e.to_string())?;
            let observed = (|| -> Result<(), String> {
                for stage in 0..2 {
                    if stage == 1 {
                        original
                            .restore(&mut macros.session)
                            .map_err(|e| e.to_string())?;
                        // The restored stage must observe only its own transport:
                        // retained stage-zero event roots are not new callbacks.
                        macros
                            .session
                            .eval("(set! literal-events [])")
                            .map_err(|e| e.to_string())?;
                    }
                    macros.session.collect().map_err(|e| e.to_string())?;
                    caller.collect().map_err(|e| e.to_string())?;
                    let source = format!(
                        "(let [^number local-x 17] (inspect-literal-effects {stage} :op op))"
                    );
                    let value = caller
                        .eval_with_macros(&source, &mut macros)
                        .map_err(|e| format!("stage{stage}: {e}"))?;
                    let bridge = FormBridge::new(&mut caller).map_err(|e| e.to_string())?;
                    let form = bridge
                        .read(&mut caller, &value, 0..1)
                        .map_err(|e| e.to_string())?;
                    eprintln!(
                        "analyzer-literal-effects {phase}/stage{stage}: {:?}",
                        form.kind
                    );
                    if !matches!(&form.kind,Kind::Vector(values) if values.len()==8 && values.iter().all(|v|v.kind==Kind::Bool(true)))
                    {
                        failures.push(format!("{phase}/stage{stage}: {:?}", form.kind));
                    }
                }
                Ok(())
            })();
            let restored = original
                .restore(&mut macros.session)
                .map_err(|e| e.to_string());
            observed?;
            restored?;
            Ok(())
        })();
        if let Err(error) = run {
            failures.push(format!("{phase}: {error}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
