use super::*;
use crate::portable_macro_data::FormBridge;
use crate::portable_session::memo_leaf_test_support::Method;
use suss_reader::forms::Kind;

#[test]
fn analyzer_literal_global_references_keep_tags_and_resolved_identity_after_gc_and_hash_restore() {
    let mut failures = Vec::new();
    for (phase, mut caller) in [("runtime", Session::new_repl().unwrap()), ("macro", Session::new_macro().unwrap())] {
        let mut macros = CompiledMacros::new().unwrap();
        let run = (|| -> Result<(), String> {
            caller.eval("(def ^{:name shadow-name :ns shadow-ns} reference-num 17)").map_err(|e|e.to_string())?;
            macros.session.eval("(def leaf-original-method nil) (def reference-tag-events [])").map_err(|e|e.to_string())?;
            let original = Method::capture(&mut macros.session, "Keyword").map_err(|e|e.to_string())?;
            original.bind_delegate(&mut macros.session).map_err(|e|e.to_string())?;
            macros.define(r#"(defmacro inspect-reference-fields [stage local cold-tag]
              (let [ast (:init (get (:locals &env) local))
                    info (:info ast)
                    raw (get (:defs (:ns &env)) 'reference-num)
                    tag-key (reduce (fn [found key] (if (= (str key) ":tag") key found)) nil (keys ast))
                    tag-events (filter (fn [event] (identical? tag-key (first event))) reference-tag-events)]
                (list 'quote [(str (:name ast)) (str (:ns ast)) (str (:tag ast))
                              (str (:name info)) (str (:ns info)) (str (:tag info))
                              (str (:name raw)) (str (:ns raw)) (nil? (.-_hash cold-tag))
                              (if (= stage 0) (every? (fn [event] (= -1290361223 (second event))) tag-events)
                                                (empty? tag-events))])))"#).map_err(|e|e.to_string())?;
            macros.session.eval(r#"(extend-type Keyword IHash (-hash [self]
              (let [memo (.-_hash self)]
                (do (if (= (.-name self) "tag")
                      (set! reference-tag-events (conj reference-tag-events [self memo])) nil)
                    (if (nil? memo) (leaf-original-method self) memo)))))"#).map_err(|e|e.to_string())?;
            let observed = (|| -> Result<(), String> {
                for stage in 0..2 {
                    if stage == 1 {
                        original.restore(&mut macros.session).map_err(|e|e.to_string())?;
                        macros.session.eval("(set! reference-tag-events [])").map_err(|e|e.to_string())?;
                    }
                    caller.collect().map_err(|e|e.to_string())?;
                    macros.session.collect().map_err(|e|e.to_string())?;
                    let source = format!("(let [reference-snapshot reference-num] (inspect-reference-fields {stage} reference-snapshot :tag))");
                    let value = caller.eval_with_macros(&source, &mut macros).map_err(|e|format!("stage{stage}: {e}"))?;
                    caller.collect().map_err(|e|e.to_string())?;
                    let bridge = FormBridge::new(&mut caller).map_err(|e|e.to_string())?;
                    let form = bridge.read(&mut caller, &value, 0..1).map_err(|e|e.to_string())?;
                    eprintln!("reference-fields {phase}/stage{stage}: {:?}", form.kind);
                    let Kind::Vector(values) = &form.kind else { return Err(format!("not vector: {form:?}")); };
                    let expected = ["user/reference-num", "user", "number", "user/reference-num", "user", "number", "shadow-name", "shadow-ns"];
                    if values.len()!=10 { return Err(format!("wrong length: {form:?}")); }
                    for (index,text) in expected.iter().enumerate() {
                        if !matches!(&values[index].kind,Kind::String(units) if units.iter().copied().eq(text.encode_utf16())) {
                            return Err(format!("stage{stage}/field{index}: {form:?}"));
                        }
                    }
                    if values[8].kind!=Kind::Bool(true) || values[9].kind!=Kind::Bool(true) { return Err(format!("stage{stage}: {form:?}")); }
                }
                Ok(())
            })();
            let restored = original.restore(&mut macros.session).map_err(|e|e.to_string());
            observed?;restored?;Ok(())
        })();
        if let Err(error)=run { failures.push(format!("{phase}: {error}")); }
    }
    assert!(failures.is_empty(),"{}",failures.join("\n"));
}
