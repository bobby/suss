use super::*;
use crate::portable_macro_data::FormBridge;
use crate::portable_session::memo_leaf_test_support::Method;
use suss_reader::forms::Kind;

#[test]
fn reader_shorthand_declaration_keys_preserve_first_origin_and_meta_alias_after_gc() {
    let cases = [
        ("(def ^number watched 17)", true, "number", "tag"),
        ("(def ^\"number\" watched 17)", true, "number", "tag"),
        ("(def ^:private watched 17)", false, "true", "private"),
        ("(def ^{:tag source-tag} watched 17)", false, "source-tag", "tag"),
        ("(def ^number ^{:tag source-tag} watched 17)", false, "number", "tag"),
        ("(def ^{:tag source-tag} ^number watched 17)", true, "source-tag", "tag"),
        ("(def ^\"number\" ^{:tag source-tag} watched 17)", false, "number", "tag"),
        ("(def ^{:tag source-tag} ^\"number\" watched 17)", true, "source-tag", "tag"),
    ];
    let mut failures = Vec::new();
    for phase in ["runtime", "macro"] {
        for (source, numeric, expected_value, field) in cases {
            let mut caller = if phase == "runtime" { Session::new_repl().unwrap() } else { Session::new_macro().unwrap() };
            let mut macros = CompiledMacros::new().unwrap();
            let result = (|| -> Result<(), String> {
                caller.eval(source).map_err(|e| e.to_string())?;
                macros.session.eval("(def leaf-original-method nil) (def reader-origin-events [])").map_err(|e| e.to_string())?;
                let original = Method::capture(&mut macros.session, "Keyword").map_err(|e| e.to_string())?;
                original.bind_delegate(&mut macros.session).map_err(|e| e.to_string())?;
                macros.define(r#"(defmacro inspect-reader-origin [stage selected literal-origin cold]
                  (let [decl (get (:defs (:ns &env)) 'watched)
                        find-key (fn [m text] (reduce (fn [found k] (if (= (str k) text) k found)) nil (keys m)))
                        key (find-key decl selected)
                        inner (find-key (:meta decl) selected)
                        hits (filter (fn [event] (identical? key (first event))) reader-origin-events)]
                    (list 'quote [(some? key) (identical? key inner)
                                  (str (get decl key))
                                  (if (= stage 0)
                                    (if literal-origin
                                      (and (= -1290361223 (.-_hash key))
                                           (every? (fn [event] (= -1290361223 (second event))) hits))
                                      (if (seq hits) (nil? (second (first hits))) (nil? (.-_hash key))))
                                    (empty? hits))
                                  (nil? (.-_hash cold)) (not (identical? key cold))])))"#).map_err(|e| e.to_string())?;
                macros.session.eval(r#"(extend-type Keyword IHash (-hash [self]
                  (let [memo (.-_hash self)]
                    (do (if (or (= (.-name self) "tag") (= (.-name self) "private"))
                          (set! reader-origin-events (conj reader-origin-events [self memo])) nil)
                        (if (nil? memo) (leaf-original-method self) memo)))))"#).map_err(|e| e.to_string())?;
                let observed = (|| -> Result<(), String> {
                    for stage in 0..2 {
                        if stage == 1 {
                            original.restore(&mut macros.session).map_err(|e| e.to_string())?;
                            macros.session.eval("(set! reader-origin-events [])").map_err(|e| e.to_string())?;
                        }
                        caller.collect().map_err(|e| e.to_string())?;
                        macros.session.collect().map_err(|e| e.to_string())?;
                        let request = format!("(inspect-reader-origin {stage} \":{field}\" {numeric} :{field})");
                        let value = caller.eval_with_macros(&request, &mut macros).map_err(|e| format!("stage{stage}: {e}"))?;
                        caller.collect().map_err(|e| e.to_string())?;
                        let bridge = FormBridge::new(&mut caller).map_err(|e| e.to_string())?;
                        let decoded = bridge.read(&mut caller, &value, 0..0).map_err(|e| e.to_string())?;
                        let Kind::Vector(values) = &decoded.kind else { return Err(format!("not vector: {decoded:?}")); };
                        if values.len()!=6 || ![0,1,3,4,5].into_iter().all(|i| values[i].kind==Kind::Bool(true))
                            || !matches!(&values[2].kind,Kind::String(units) if units.iter().copied().eq(expected_value.encode_utf16())) {
                            return Err(format!("stage{stage}: {decoded:?}"));
                        }
                    }
                    Ok(())
                })();
                let restored = original.restore(&mut macros.session).map_err(|e| e.to_string());
                observed?; restored?; Ok(())
            })();
            if let Err(error) = result { failures.push(format!("{phase}/{source}: {error}")); }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
