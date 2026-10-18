//! Matched pinned reader/analyzer identifier initialization and effects.
//! Native observations cover &env metadata/leaf identity, not CLJS resolve/AST views.
use super::*;
use crate::portable::resolve::Phase;
use crate::portable_session::memo_leaf_test_support::Method;
use serde_json::{Value, json};

const OBSERVE: &str = r#"
(defmacro observe-leaf [stage warm]
  (let [m (:meta (get (:defs (:ns &env)) 'watched))
        leaf (:audit-leaf m)]
    (if (= stage 0)
      (do
        (def leaf-saved-meta m)
        (def leaf-saved-value leaf)
        (def leaf-initial-memo (.-_hash leaf))
        (def leaf-canonical (if warm (hash leaf) nil))
        (def leaf-before-custom-memo (.-_hash leaf))) nil)
    (let [before leaf-dispatches
          result (if (= stage 1) (hash leaf-saved-value) nil)
          after leaf-dispatches]
      (list 'quote
        [leaf-initial-memo leaf-canonical leaf-before-custom-memo
         (identical? leaf-saved-meta m) (identical? leaf-saved-value leaf)
         (.-_hash leaf-saved-value) (.-_hash leaf)
         leaf-dispatches leaf-writes (- after before) result
         (str leaf-saved-value) (str leaf)]))))
"#;
fn run_group(kind: &str, nil_aware: bool) {
    let reference: Value = serde_json::from_str(include_str!(
        "../../../../tests/oracle/metadata-memo-leaf-observations.json"
    ))
    .unwrap();
    let mut differences = Vec::new();
    let mut rows = Vec::new();
    for warm in [false, true] {
        let pinned = reference["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| {
                case["kind"] == kind
                    && case["nil-aware"] == nil_aware
                    && case["explicit-warm"] == warm
            })
            .unwrap();
        let owner = if kind == "symbol" {
            "Symbol"
        } else {
            "Keyword"
        };
        for phase in [Phase::Runtime, Phase::Macro] {
            let label = format!("{kind}/nil-aware={nil_aware}/warm={warm}/{phase:?}");
            let mut stage = "setup";
            let run = (|| -> Result<(), String> {
                let mut caller = match phase {
                    Phase::Runtime => Session::new_repl(),
                    Phase::Macro => Session::new_macro(),
                }
                .map_err(|e| e.to_string())?;
                caller
                    .eval(pinned["source"].as_str().unwrap())
                    .map_err(|e| e.to_string())?;
                let mut macros = CompiledMacros::new().map_err(|e| e.to_string())?;
                macros.session.eval("(def leaf-dispatches 0) (def leaf-writes 0) (def leaf-original-method nil)").map_err(|e|e.to_string())?;
                let original =
                    Method::capture(&mut macros.session, owner).map_err(|e| e.to_string())?;
                original
                    .bind_delegate(&mut macros.session)
                    .map_err(|e| e.to_string())?;
                macros.define(OBSERVE).map_err(|e| e.to_string())?;
                let result = (|| -> Result<(), String> {
                    for index in 0..3 {
                        stage = ["baseline", "custom-write", "restored-method"][index];
                        if index == 1 {
                            let custom = format!(
                                r#"(extend-type {owner} IHash
                              (-hash [self]
                                (if (identical? self leaf-saved-value)
                                  (do (set! leaf-dispatches (inc leaf-dispatches))
                                    (let [memo (.-_hash self)]
                                      (if (and {nil_aware} (some? memo)) memo
                                        (do (set! leaf-writes (inc leaf-writes))
                                            (set! (.-_hash self) 91) 91))))
                                  (leaf-original-method self))))"#
                            );
                            macros.session.eval(&custom).map_err(|e| e.to_string())?;
                        }
                        if index == 2 {
                            original
                                .restore(&mut macros.session)
                                .map_err(|e| e.to_string())?;
                        }
                        caller.collect().map_err(|e| e.to_string())?;
                        let source = format!("(observe-leaf {index} {warm})");
                        let value = caller
                            .eval_with_macros(&source, &mut macros)
                            .map_err(|e| e.to_string())?;
                        let form = crate::portable_repl::display(&mut caller, &value)
                            .map_err(|e| e.to_string())?;
                        let parsed = read_forms(&form).map_err(|e| e.to_string())?;
                        let Kind::Vector(items) = &parsed[0].kind else {
                            return Err(format!("expected observation vector: {form}"));
                        };
                        let actual = items
                            .iter()
                            .map(|item| match &item.kind {
                                Kind::Nil => Ok(Value::Null),
                                Kind::Bool(x) => Ok(json!(x)),
                                Kind::Number(x) => Ok(json!(*x as i64)),
                                Kind::String(x) => String::from_utf16(x)
                                    .map(Value::String)
                                    .map_err(|e| e.to_string()),
                                _ => Err(format!("unexpected observation scalar: {item:?}")),
                            })
                            .collect::<Result<Vec<_>, String>>()?;
                        let canonical = pinned["canonical-hash"].clone();
                        let before = pinned["before-custom-memo"].clone();
                        let expected_memo = if index == 0 {
                            before.clone()
                        } else if nil_aware && warm {
                            canonical.clone()
                        } else {
                            json!(91)
                        };
                        let expected_writes = if index == 0 || (nil_aware && warm) {
                            0
                        } else {
                            1
                        };
                        let expected = vec![
                            Value::Null,
                            canonical,
                            before,
                            json!(true),
                            json!(true),
                            expected_memo.clone(),
                            expected_memo.clone(),
                            json!(if index == 0 { 0 } else { 1 }),
                            json!(expected_writes),
                            json!(if index == 1 { 1 } else { 0 }),
                            if index == 1 {
                                expected_memo
                            } else {
                                Value::Null
                            },
                            pinned["leaf-text"].clone(),
                            pinned["leaf-text"].clone(),
                        ];
                        if actual != expected {
                            differences.push(format!(
                                "{label}/{stage}: actual={actual:?}; expected={expected:?}"
                            ));
                        }
                        let row = json!({"case":label,"stage":stage,"fields":["initial-memo","canonical-hash","before-custom-memo","env-meta-identity","env-leaf-identity","saved-memo","current-memo","subject-dispatches","subject-writes","window-dispatches","hash-result","saved-leaf-text","current-leaf-text"],"actual":actual,"expected":expected});
                        println!("NATIVE_MEMO_LEAF {}", row);
                        rows.push(row);
                    }
                    Ok(())
                })();
                let restored = original
                    .restore(&mut macros.session)
                    .map_err(|e| e.to_string());
                result?;
                restored?;
                let control = caller.eval("watched").map_err(|e| e.to_string())?;
                let actual = crate::portable_repl::display(&mut caller, &control)
                    .map_err(|e| e.to_string())?;
                if actual != "17" {
                    differences.push(format!("{label}: declaration control {actual}"));
                }
                Ok(())
            })();
            if let Err(error) = run {
                differences.push(format!("{label}/{stage}: execution error {error}"));
            }
        }
    }
    println!(
        "NATIVE_MEMO_LEAF_SUMMARY {}",
        json!({"kind":kind,"nil-aware":nil_aware,"rows":rows.len(),"differences":differences})
    );
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}
#[test]
fn native_memo_leaf_symbol_forcewrite() {
    run_group("symbol", false);
}
#[test]
fn native_memo_leaf_symbol_nil_aware() {
    run_group("symbol", true);
}
#[test]
fn native_memo_leaf_keyword_forcewrite() {
    run_group("keyword", false);
}
#[test]
fn native_memo_leaf_keyword_nil_aware() {
    run_group("keyword", true);
}

#[test]
fn compiled_identifier_constants_keep_pinned_warmed_memos() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let value = session
            .eval("[(.-_hash 'memo-symbol-leaf-warm) (.-_hash :memo-keyword-leaf-warm)]")
            .unwrap();
        assert_eq!(
            crate::portable_repl::display(&mut session, &value).unwrap(),
            "[1454146648 -24028250]"
        );
    }
}
