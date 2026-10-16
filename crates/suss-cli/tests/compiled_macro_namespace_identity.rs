//! Five actual compiled macro invocations: a bounded pinned persistent-object contract.
//! Fresh re-resolution inside an observer is unavailable here (no ana/analyze API).
//! Its fresh-info contract remains in compiled_macro_cross_invocation_identity_probe.
use serde_json::{json, Value};
use suss_cli::{portable_macro_data::FormBridge, portable_macros::CompiledMacros, portable_session::Session};
use suss_reader::forms::{Form, Kind};

const OBSERVER: &str = r#"(defmacro observe-namespace-sharing [label]
 (let [ns (:ns &env) defs (:defs ns) live (:suss/catalog &env)
       scalar (get defs 'watched) callable (get defs 'callable)
       locals (:locals &env) captured (get locals 'captured)
       localfn (get locals 'localfn) init (:init captured) f-init (:init localfn)
       method (first (:methods f-init))]
  (defonce saved-env &env)
  (defonce saved-ns ns)
  (defonce saved-defs defs)
  (defonce saved-live live)
  (defonce saved-scalar scalar)
  (defonce saved-callable callable)
  (defonce saved-meta (:meta scalar))
  (defonce saved-audit (:audit (:meta scalar)))
  (defonce saved-fn-meta (:meta callable))
  (defonce saved-method-params (:method-params callable))
  (defonce saved-arglists-meta (:arglists-meta callable))
  (defonce saved-locals locals)
  (defonce saved-captured captured)
  (defonce saved-init init)
  (defonce saved-info (:info init))
  (defonce saved-localfn localfn)
  (defonce saved-f-init f-init)
  (defonce saved-methods (:methods f-init))
  (defonce saved-method method)
  (defonce saved-params (:params method))
  (defonce saved-body (:body method))
  (let [row [label
   [(identical? saved-env &env) (identical? saved-ns ns)
    (identical? saved-defs defs) (identical? saved-live live)
    (identical? ns live)]
   [(identical? saved-scalar scalar) (identical? saved-callable callable)
    (identical? saved-meta (:meta scalar)) (identical? saved-audit (:audit (:meta scalar)))
    (identical? saved-fn-meta (:meta callable))
    (identical? saved-method-params (:method-params callable))
    (identical? saved-arglists-meta (:arglists-meta callable))]
   [(contains? locals 'captured) (contains? locals 'localfn)
    (identical? saved-locals locals)
    (if captured [(identical? saved-captured captured) (identical? saved-init init)
                  (identical? saved-info (:info init))
                  (identical? (:meta (:info init)) (:meta scalar))] nil)
    (if localfn [(identical? saved-localfn localfn) (identical? saved-f-init f-init)
                 (identical? saved-methods (:methods f-init))
                 (identical? saved-method method) (identical? saved-params (:params method))
                 (identical? saved-body (:body method))] nil)]
   [(contains? defs 'unrelated) (contains? (:defs live) 'unrelated)
    (:doc (:meta scalar)) (:doc (:meta callable)) (:fn-var callable)
    (count (:method-params callable)) (nil? (:arglists callable))]]]
   (defonce recorded-rows [])
   (set! recorded-rows (conj recorded-rows row))
   (list 'quote (if (= label "new-env-after-def") recorded-rows row)))))"#;

const LEXICAL: &str = r#"(let* [captured watched localfn (fn* [v] v)]
 (observe-namespace-sharing "same-1")
 (observe-namespace-sharing "same-2")
 (def unrelated 23)
 (observe-namespace-sharing "post-def-1")
 (observe-namespace-sharing "post-def-2") nil)"#;

fn data(form: &Form) -> Value {
    match &form.kind {
        Kind::Nil => Value::Null,
        Kind::Bool(x) => json!(x),
        Kind::Number(x) => json!(x),
        Kind::String(x) => json!(String::from_utf16(x).unwrap()),
        Kind::Vector(xs) | Kind::List(xs) => json!(xs.iter().map(data).collect::<Vec<_>>()),
        other => panic!("unexpected projection: {other:?}"),
    }
}

fn expected_rows() -> Value {
    let labels=["same-1","same-2","post-def-1","post-def-2","new-env-after-def"];
    json!(labels.iter().enumerate().map(|(i,label)| {
        let lexical=i<4;
        json!([label,
            [i==0,lexical,lexical,i<2,i<2 || i==4],
            [true,true,true,true,true,true,true],
            [lexical,lexical,lexical,
             if lexical {json!([true,true,true,true])}else{Value::Null},
             if lexical {json!([true,true,true,true,true,true])}else{Value::Null}],
            [i==4,i>=2,Value::Null,"callable-doc",true,2.0,true]])
    }).collect::<Vec<_>>())
}

#[test]
fn compiler_namespace_and_declaration_aliases_match_pinned_five_call_observations() {
    let mut failures = Vec::new();
    let mut receipts = Vec::new();
    for (phase, session) in [("runtime", Session::new_repl()), ("macro", Session::new_macro())] {
        let result: Result<Value, String> = (|| {
            let mut session = session.map_err(|e| format!("session: {e:?}"))?;
            let mut macros = CompiledMacros::new().map_err(|e| format!("macro store: {e:?}"))?;
            session.enter_namespace("suss-oracle.sharing-target").map_err(|e| format!("caller ns: {e:?}"))?;
            macros.enter_namespace("suss-oracle.sharing-target").map_err(|e| format!("macro ns: {e:?}"))?;
            // No fuel setter: both stores retain their existing default allowances.
            macros.define(OBSERVER).map_err(|e| format!("define observer: {e:?}"))?;
            session.eval("(def ^{:audit #{101 102 103 104 105 106 107 108 109}} watched 17) (def ^{:doc \"callable-doc\"} callable (fn* ([x] x) ([x y] y)))")
                .map_err(|e| format!("setup declarations: {e:?}"))?;
            session.eval_with_macros(LEXICAL, &mut macros).map_err(|e| format!("four lexical calls: {e:?}"))?;
            let last = session.eval_with_macros("(observe-namespace-sharing \"new-env-after-def\")", &mut macros)
                .map_err(|e| format!("fifth call: {e:?}"))?;
            // Fifth invocation returns its accumulated lossless rows; no sixth macro call.
            session.collect().map_err(|e| format!("GC: {e:?}"))?;
            let bridge = FormBridge::new(&mut session).map_err(|e| format!("bridge: {e:?}"))?;
            let form = bridge.read(&mut session, &last, 0..1).map_err(|e| format!("decode: {e:?}"))?;
            Ok(data(&form))
        })();
        match result {
            Ok(rows) => {
                let expected = expected_rows();
                if rows != expected {failures.push(format!("{phase}: identity/value projection differs from pinned subset"));}
                receipts.push(json!({"phase":phase,"rows":rows,"expected":expected,"matches":rows==expected}));
            },
            Err(error) => {failures.push(format!("{phase}: {error}"));receipts.push(json!({"phase":phase,"error":error,"rows":"unexecuted; inspect stage"}));}
        }
    }
    let receipt=json!({"phases":receipts,"limits":"Bounded aliases only; Suss :suss/catalog mapped to live analyzer namespace. No fresh ana/analyze inside native macro observer. Existing oracle arglists is nil; no full-schema or source/storage-proof claim."});
    if let Some(path)=std::env::var_os("SUSS_NAMESPACE_IDENTITY_OUTPUT") {std::fs::write(path,serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();}
    eprintln!("NAMESPACE_IDENTITY {receipt}");
    assert!(failures.is_empty(),"{}",failures.join("\n"));
}

/// Separate fresh caller bindings expose real resolved info without an analyzer API.
#[test]
fn resolved_callable_info_keeps_raw_declaration_children_across_macro_invocations() {
    const CALLABLE_OBSERVER: &str = r#"(defmacro observe-callable-info [label binding]
      (let [raw (get (:defs (:ns &env)) 'callable)
            info (:info (:init (get (:locals &env) binding)))
            raw-params (:method-params raw) info-params (:method-params info)]
        (defonce callable-saved-info info)
        (defonce callable-saved-raw raw)
        (defonce callable-saved-params raw-params)
        (defonce callable-saved-meta (:meta raw))
        (list 'quote [label
          (contains? raw :method-params) (contains? info :method-params)
          (identical? callable-saved-info info)
          (identical? raw info)
          (identical? callable-saved-raw raw)
          (identical? raw-params info-params)
          (identical? callable-saved-params raw-params)
          (identical? callable-saved-params info-params)
          (identical? (:meta raw) (:meta info))
          (identical? callable-saved-meta (:meta raw))
          (identical? callable-saved-meta (:meta info))
          (count raw-params) (count info-params)
          (:doc (:meta info))])))"#;
    let mut failures = Vec::new();
    let mut observations = Vec::new();
    for (phase, session) in [("runtime", Session::new_repl()), ("macro", Session::new_macro())] {
        let result: Result<Value, String> = (|| {
            let mut session = session.map_err(|e| format!("session: {e:?}"))?;
            let mut macros = CompiledMacros::new().map_err(|e| format!("macro store: {e:?}"))?;
            session.enter_namespace("suss-oracle.sharing-target").map_err(|e| format!("caller ns: {e:?}"))?;
            macros.enter_namespace("suss-oracle.sharing-target").map_err(|e| format!("macro ns: {e:?}"))?;
            macros.define(CALLABLE_OBSERVER).map_err(|e| format!("define observer: {e:?}"))?;
            session.eval("(def ^{:doc \"callable-doc\"} callable (fn* ([x] x) ([x y] y)))")
                .map_err(|e| format!("setup callable: {e:?}"))?;
            let mut rows = Vec::new();
            let mut errors = Vec::new();
            for (i, label) in ["first", "same-revision-2", "same-revision-3"].into_iter().enumerate() {
                let source=format!("(let [resolved callable] (observe-callable-info \"{label}\" resolved))");
                let observed: Result<Value, String> = (|| {
                    let value=session.eval_with_macros(&source, &mut macros).map_err(|e| format!("{label}: {e:?}"))?;
                    session.collect().map_err(|e| format!("{label} GC: {e:?}"))?;
                    let bridge=FormBridge::new(&mut session).map_err(|e| format!("{label} bridge: {e:?}"))?;
                    let form=bridge.read(&mut session, &value, 0..1).map_err(|e| format!("{label} decode: {e:?}"))?;
                    Ok(data(&form))
                })();
                match observed {
                    Ok(row) => {
                        let expected=json!([label,true,true,i==0,false,true,true,true,true,true,true,true,2.0,2.0,"callable-doc"]);
                        if row!=expected {failures.push(format!("{phase}/{label}: callable alias projection differs"));}
                        rows.push(json!({"label":label,"actual":row,"expected":expected,"matches":row==expected}));
                    },
                    Err(error) => {failures.push(format!("{phase}/{label}: {error}"));errors.push(json!({"label":label,"error":error}));}
                }
            }
            Ok(json!({"rows":rows,"errors":errors}))
        })();
        match result {
            Ok(rows) => observations.push(json!({"phase":phase,"observations":rows})),
            Err(error) => {failures.push(format!("{phase}: {error}"));observations.push(json!({"phase":phase,"error":error}));}
        }
    }
    let receipt=json!({"phases":observations,"limits":"Three independent default-budget requests per caller phase, fresh caller binding each time. Raw/resolved method-params and metadata references, not cloned entry values. Separate from the five-call namespace test. No full-schema/source-storage-authority claim."});
    if let Some(path)=std::env::var_os("SUSS_NAMESPACE_IDENTITY_OUTPUT") {std::fs::write(path,serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();}
    eprintln!("CALLABLE_NAMESPACE_IDENTITY {receipt}");
    assert!(failures.is_empty(),"{}",failures.join("\n"));
}
