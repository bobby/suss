use super::*;
use crate::portable_macro_data::FormBridge;
use crate::portable_session::memo_leaf_test_support::Method;
use suss_reader::forms::Kind;

#[test]
fn analyzer_literal_actual_body_transport_distinguishes_future_origin_and_restores_hash() {
    let mut failures = Vec::new();
    for (phase, mut caller) in [("runtime", Session::new_repl().unwrap()), ("macro", Session::new_macro().unwrap())] {
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(crate::portable_session::memo_leaf_test_support::COMPILATION_FUEL);
        let run = (|| -> Result<(), String> {
            macros.session.eval("(def leaf-original-method nil) (def body-origin-events [])").map_err(|e| e.to_string())?;
            let original = Method::capture(&mut macros.session, "Keyword").map_err(|e| e.to_string())?;
            original.bind_delegate(&mut macros.session).map_err(|e| e.to_string())?;
            macros.define(r#"(defmacro inspect-body-origin [stage known extension cold]
              (let [find-key (fn [node] (reduce (fn [found key] (if (= (str key) ":body?") key found)) nil (keys node)))
                    known-body (:body (first (:methods (:init (get (:locals &env) known)))))
                    extension-method-body (:body (first (:methods (:init (get (:locals &env) extension)))))
                    future-node (:ret extension-method-body)
                    future-body (:body future-node)
                    check-entry (fn [node expected]
                                  (let [key (find-key node)
                                        hits (reduce (fn [out row] (if (identical? key (first row)) (conj out (second row)) out)) [] body-origin-events)]
                                    (and key (if (= stage 0) (and (pos? (count hits)) (= expected (first hits))) (empty? hits)))))]
                (list 'quote [(check-entry known-body -1333761917)
                              (check-entry future-body nil)
                              (= true (:body? known-body)) (= true (:body? future-body))
                              (= :do (:op known-body)) (= :do (:op future-body))
                              (= :suss/future (:op future-node)) (nil? (.-_hash cold))])))"#).map_err(|e| e.to_string())?;
            macros.session.eval(r#"(extend-type Keyword IHash
              (-hash [self]
                (let [memo (.-_hash self)]
                  (do (if (= (.-name self) "body?")
                        (set! body-origin-events (conj body-origin-events [self memo])) nil)
                      (if (nil? memo) (leaf-original-method self) memo)))))"#).map_err(|e| e.to_string())?;
            let observed = (|| -> Result<(), String> {
                for stage in 0..2 {
                    if stage == 1 {
                        original.restore(&mut macros.session).map_err(|e| e.to_string())?;
                        macros.session.eval("(set! body-origin-events [])").map_err(|e| e.to_string())?;
                    }
                    caller.collect().map_err(|e| e.to_string())?;
                    macros.session.collect().map_err(|e| e.to_string())?;
                    // Both initializers create uncalled functions; no future is started.
                    let source = format!("(let [known-body-fn (fn [] 17) future-body-fn (fn [] (suss.async/future* 17))] (inspect-body-origin {stage} known-body-fn future-body-fn :body?))");
                    let value = caller.eval_with_macros(&source, &mut macros).map_err(|e| format!("stage{stage}: {e}"))?;
                    caller.collect().map_err(|e| e.to_string())?;
                    macros.session.collect().map_err(|e| e.to_string())?;
                    let bridge = FormBridge::new(&mut caller).map_err(|e| e.to_string())?;
                    let form = bridge.read(&mut caller, &value, 0..1).map_err(|e| e.to_string())?;
                    eprintln!("body-origin {phase}/stage{stage}: {:?}", form.kind);
                    if !matches!(&form.kind, Kind::Vector(values) if values.len()==8 && values.iter().all(|v|v.kind==Kind::Bool(true))) {
                        return Err(format!("stage{stage}: {:?}", form.kind));
                    }
                }
                Ok(())
            })();
            let restored = original.restore(&mut macros.session).map_err(|e| e.to_string());
            observed?;
            restored?;
            Ok(())
        })();
        if let Err(error) = run { failures.push(format!("{phase}: {error}")); }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
