//! Same production macro expansion in both caller phases, with a genuine
//! delegated IHash replacement installed before caller environment transport.
use super::*;
use crate::portable_session::memo_leaf_test_support::Method;
#[path = "../../tests/support/compilation_fuel.rs"]
mod compilation_fuel;

#[test]
fn analyzer_literal_actual_env_pre_dispatch_memo_effects_survive_gc_and_restoration() {
    let mut failures = Vec::new();
    for (phase, mut caller) in [
        ("runtime", Session::new_repl().unwrap()),
        ("macro", Session::new_macro().unwrap()),
    ] {
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
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


// Pinned analyzer.cljc4369–4400 and compiler.cljc368–382 establish these
// construction-site hashes; this test exercises Suss transport, not an oracle
// regeneration. Record entry memos BEFORE the nil-aware delegate can warm them.
#[test]
fn analyzer_literal_collection_keys_observe_initial_memos_under_nil_aware_hash() {
    let mut failures = Vec::new();
    for (phase, mut caller) in [
        ("runtime", Session::new_repl().unwrap()),
        ("macro", Session::new_macro().unwrap()),
    ] {
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
        let run = (|| -> Result<(), String> {
            macros.session.eval("(def leaf-original-method nil) (def collection-events [])")
                .map_err(|e| e.to_string())?;
            let original = Method::capture(&mut macros.session, "Keyword")
                .map_err(|e| e.to_string())?;
            original.bind_delegate(&mut macros.session).map_err(|e| e.to_string())?;
            macros.define(r#"(defmacro inspect-collection-literals
                [stage map-name set-name vector-name cold-keys cold-vals cold-items cold-children cold-form]
              (let [find-key (fn [m text]
                               (reduce (fn [found key] (if (= (str key) text) key found)) nil (keys m)))
                    map-ast (:init (get (:locals &env) map-name))
                    set-ast (:init (get (:locals &env) set-name))
                    vector-ast (:init (get (:locals &env) vector-name))
                    check-key (fn [ast text expected]
                                (let [key (find-key ast text)
                                      initial-memo (.-_hash key)
                                      before (count (filter (fn [row] (identical? key (first row))) collection-events))
                                      explicit-hash (hash key)
                                      hits (reduce (fn [out row]
                                                     (if (identical? key (first row))
                                                       (conj out (second row)) out))
                                                   [] collection-events)]
                                  (and (some? key) (= expected initial-memo) (= expected explicit-hash)
                                       (if (= stage 0)
                                         (and (= (count hits) (+ before 1))
                                              (every? (fn [memo] (= expected memo)) hits))
                                         (empty? hits)))))
                    check-children (fn [ast specs]
                                     (let [names (:children ast)]
                                       (and (= (count names) (count specs))
                                            (every? true? (map (fn [name spec]
                                                                (and (= (str name) (first spec))
                                                                     (= (.-_hash name) (second spec))
                                                                     (not (identical? name (nth spec 2)))))
                                                              names specs)))))]
                (list 'quote
                  [(every? true? [(check-key map-ast ":keys" 1068423698)
                                  (check-key map-ast ":vals" 768058733)
                                  (check-key map-ast ":children" -940561982)
                                  (check-key map-ast ":form" -1624062471)])
                   (every? true? [(check-key set-ast ":items" 1031954938)
                                  (check-key set-ast ":children" -940561982)
                                  (check-key set-ast ":form" -1624062471)])
                   (every? true? [(check-key vector-ast ":items" 1031954938)
                                  (check-key vector-ast ":children" -940561982)
                                  (check-key vector-ast ":form" -1624062471)])
                   (check-children map-ast [[":keys" 1068423698 cold-keys] [":vals" 768058733 cold-vals]])
                   (check-children set-ast [[":items" 1031954938 cold-items]])
                   (check-children vector-ast [[":items" 1031954938 cold-items]])
                   (every? nil? [(.-_hash cold-keys) (.-_hash cold-vals) (.-_hash cold-items)
                                 (.-_hash cold-children) (.-_hash cold-form)])
                   (every? false? [(identical? cold-keys (find-key map-ast ":keys"))
                                   (identical? cold-vals (find-key map-ast ":vals"))
                                   (identical? cold-items (find-key set-ast ":items"))
                                   (identical? cold-children (find-key map-ast ":children"))
                                   (identical? cold-form (find-key vector-ast ":form"))])
                   (= 17 (:val (first (:items vector-ast))))])))"#)
                .map_err(|e| e.to_string())?;
            macros.session.eval(r#"(extend-type Keyword IHash
              (-hash [self]
                (let [memo (.-_hash self)]
                  (do (if (or (= (.-name self) "keys") (= (.-name self) "vals")
                              (= (.-name self) "items") (= (.-name self) "children")
                              (= (.-name self) "form"))
                        (set! collection-events (conj collection-events [self memo])) nil)
                      (if (nil? memo) (leaf-original-method self) memo)))))"#)
                .map_err(|e| e.to_string())?;
            let observed = (|| -> Result<(), String> {
                for stage in 0..2 {
                    if stage == 1 {
                        original.restore(&mut macros.session).map_err(|e| e.to_string())?;
                        macros.session.eval("(set! collection-events [])").map_err(|e| e.to_string())?;
                    }
                    macros.session.collect().map_err(|e| e.to_string())?;
                    caller.collect().map_err(|e| e.to_string())?;
                    let source = format!("(let [collection-m {{:left 17}} collection-s #{{17}} collection-v [17]] (inspect-collection-literals {stage} collection-m collection-s collection-v :keys :vals :items :children :form))");
                    let value = caller.eval_with_macros(&source, &mut macros)
                        .map_err(|e| format!("stage{stage}: {e}"))?;
                    // Retain the result through collection before decoding facts.
                    caller.collect().map_err(|e| e.to_string())?;
                    macros.session.collect().map_err(|e| e.to_string())?;
                    let bridge = FormBridge::new(&mut caller).map_err(|e| e.to_string())?;
                    let form = bridge.read(&mut caller, &value, 0..1).map_err(|e| e.to_string())?;
                    eprintln!("analyzer-collection-literals {phase}/stage{stage}: {:?}", form.kind);
                    if !matches!(&form.kind, Kind::Vector(values) if values.len() == 9 && values.iter().all(|v| v.kind == Kind::Bool(true))) {
                        failures.push(format!("{phase}/stage{stage}: {:?}", form.kind));
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


// Supplemental real transport observations; expected numeric memos are copied
// from the existing pinned analyzer.js constructors, not calculated by Suss.
fn run_proven_ast_literal_family(label: &str, initializer: &str, required_ops: &[&str], named: bool) {
    let mut failures = Vec::new();
    for (phase, mut caller) in [
        ("runtime", Session::new_repl().unwrap()),
        ("macro", Session::new_macro().unwrap()),
    ] {
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
        let run = (|| -> Result<(), String> {
            caller.eval("(def ast-literal-target 0)").map_err(|e| e.to_string())?;
            macros.session.eval(r#"(def leaf-original-method nil)
                (def ast-literal-events []) (def ast-observed-names #{"args" "bindings" "body" "catch" "children" "class" "else" "exception" "expr" "exprs" "finally" "fn" "fn-var" "form" "inferred-ret-tag" "literal?" "local" "max-fixed-arity" "meta" "method-params" "methods" "ret" "statements" "target" "test" "then" "val" "variadic?" "lowering" "info"})"#)
                .map_err(|e| e.to_string())?;
            let original = Method::capture(&mut macros.session, "Keyword").map_err(|e| e.to_string())?;
            original.bind_delegate(&mut macros.session).map_err(|e| e.to_string())?;
            macros.define(r#"(defmacro inspect-proven-ast-literals [stage binding cold-form cold-expr]
              (let [root (:init (get (:locals &env) binding))
                    specs [[":if" [[":test" 577538877] [":then" 460598070] [":else" -1508377146] [":children" -940561982] [":form" -1624062471]]] [":do" [[":statements" 600349855] [":ret" -468222814] [":children" -940561982] [":form" -1624062471]]] [":throw" [[":exception" -335277064] [":children" -940561982] [":form" -1624062471]]] [":recur" [[":exprs" 1795829094] [":children" -940561982] [":form" -1624062471]]] [":loop" [[":bindings" 1271397192] [":body" -2049205669] [":children" -940561982] [":form" -1624062471]]] [":let" [[":bindings" 1271397192] [":body" -2049205669] [":children" -940561982] [":form" -1624062471]]] [":try" [[":body" -2049205669] [":catch" 1038065524] [":finally" 1589088705] [":children" -940561982] [":form" -1624062471]]] [":set!" [[":target" 253001721] [":val" 128701612] [":children" -940561982] [":form" -1624062471]]] [":invoke" [[":fn" -1175266204] [":args" 1315556576] [":children" -940561982] [":form" -1624062471]]] [":new" [[":class" -2030961996] [":args" 1315556576] [":children" -940561982] [":form" -1624062471]]] [":with-meta" [[":meta" 1499536964] [":expr" 745722291] [":children" -940561982] [":form" -1624062471]]] [":quote" [[":literal?" 352485871] [":expr" 745722291] [":children" -940561982] [":form" -1624062471]]] [":const" [[":val" 128701612] [":form" -1624062471]]] [":fn" [[":methods" 453930866] [":children" -940561982] [":form" -1624062471]]] [":var" [[":info" -317069002] [":form" -1624062471]]] [":js-var" [[":info" -317069002] [":form" -1624062471]]]]
                    find-key (fn [node text]
                               (reduce (fn [found key] (if (= (str key) text) key found)) nil (keys node)))
                    check-key (fn [node pair]
                                (let [key (find-key node (first pair))
                                      hits (reduce (fn [out event]
                                                     (if (identical? key (first event))
                                                       (conj out (second event)) out)) [] ast-literal-events)]
                                  (if (some? key)
                                    (let [before (.-_hash key)
                                          before-count (count hits)
                                          actual (hash key)
                                          after-hits (reduce (fn [out event]
                                                               (if (identical? key (first event))
                                                                 (conj out (second event)) out))
                                                             [] ast-literal-events)]
                                      (and (= before (second pair)) (= actual (second pair))
                                           (= (.-_hash key) (second pair))
                                           (if (= stage 0)
                                             (and (every? (fn [memo] (= memo (second pair))) hits)
                                                  (= (count after-hits) (inc before-count))
                                                  (= (last after-hits) before))
                                             (and (empty? hits) (empty? after-hits)))))
                                    false)))
                    walk (fn walk [node]
                           (let [op (str (:op node))
                                 spec (some (fn [spec] (if (= op (first spec)) spec nil)) specs)
                                 names (:children node)
                                 expected-names (cond
                                   (= op ":do") [":statements" ":ret"]
                                   (= op ":if") [":test" ":then" ":else"]
                                   (= op ":recur") [":exprs"]
                                   (= op ":throw") [":exception"]
                                   (or (= op ":let") (= op ":loop")) [":bindings" ":body"]
                                   (= op ":try") (into [":body"] (concat (if (:catch node) [":catch"] [])
                                                                                       (if (:finally node) [":finally"] [])))
                                   (= op ":set!") [":target" ":val"]
                                   (= op ":invoke") [":fn" ":args"]
                                   (= op ":new") [":class" ":args"]
                                   (= op ":with-meta") [":meta" ":expr"]
                                   (= op ":quote") [":expr"]
                                   (= op ":fn") (if (some? (find-key node ":local")) [":local" ":methods"] [":methods"])
                                   :else [])
                                 names-ok (and (= (vec (map str names)) expected-names)
                                               (every? (fn [name]
                                                         (let [pair (some (fn [pair] (if (= (first pair) (str name)) pair nil))
                                                                          (conj (second spec) [":local" -1497766724]))]
                                                           (and pair (= (.-_hash name) (second pair))))) names))
                                 own (if spec [[op (and names-ok (every? (fn [pair] (check-key node pair)) (second spec)))]] [])
                                 children (reduce (fn [out name]
                                                    (let [value (get node name)]
                                                      (if (map? value) (conj out value)
                                                        (if (vector? value) (into out value) out))))
                                                  [] (:children node))]
                             (reduce (fn [out child] (into out (walk child))) own children)))
                    rows (walk root)
                    lowering (find-key root ":suss/lowering")
                    extension-events (filter (fn [event] (identical? lowering (first event))) ast-literal-events)
                    named-records (filter (fn [record] (some? (find-key record ":fn-var")))
                                          (mapcat (fn [method] (vals (:locals (:env method)))) (:methods root)))
                    named-ok (every? (fn [record]
                                      (every? (fn [pair] (check-key record pair))
                                        [[":fn-var" 1086204730] [":variadic?" 584179762]
                                         [":max-fixed-arity" -690205543] [":method-params" -980792179]]))
                                    named-records)
                    local-key (find-key root ":local")
                    local-ok (if local-key (check-key root [":local" -1497766724]) true)
                    inferred-key (find-key root ":inferred-ret-tag")
                    inferred-ok (if inferred-key (check-key root [":inferred-ret-tag" 798934347]) true)]
                (list 'quote [rows (nil? (.-_hash cold-form)) (nil? (.-_hash cold-expr))
                               (if (= stage 0)
                                 (and (seq extension-events) (nil? (second (first extension-events))))
                                 (empty? extension-events))
                               (and named-ok local-ok inferred-ok) (pos? (count named-records))])))"#)
                .map_err(|e| e.to_string())?;
            macros.session.eval(r#"(extend-type Keyword IHash
              (-hash [self]
                (let [memo (.-_hash self)]
                  (do (if (contains? ast-observed-names (.-name self))
                        (set! ast-literal-events (conj ast-literal-events [self memo])) nil)
                      (if (nil? memo) (leaf-original-method self) memo)))))"#)
                .map_err(|e| e.to_string())?;
            let observed = (|| -> Result<(), String> {
                for stage in 0..2 {
                    if stage == 1 {
                        original.restore(&mut macros.session).map_err(|e| e.to_string())?;
                        macros.session.eval("(set! ast-literal-events [])").map_err(|e| e.to_string())?;
                    }
                    caller.collect().map_err(|e| e.to_string())?;
                    macros.session.collect().map_err(|e| e.to_string())?;
                    let source = format!("(let [ast-subject {initializer}] (inspect-proven-ast-literals {stage} ast-subject :form :expr))");
                    let value = caller.eval_with_macros(&source, &mut macros)
                        .map_err(|e| format!("{phase}/{label}/stage{stage}: {e}"))?;
                    caller.collect().map_err(|e| e.to_string())?;
                    macros.session.collect().map_err(|e| e.to_string())?;
                    let bridge = FormBridge::new(&mut caller).map_err(|e| e.to_string())?;
                    let form = bridge.read(&mut caller, &value, 0..1).map_err(|e| e.to_string())?;
                    eprintln!("proven-ast-literals {phase}/{label}/stage{stage}: {:?}", form.kind);
                    let Kind::Vector(parts) = &form.kind else { return Err(format!("not a vector: {form:?}")); };
                    if parts.len() != 6 || !parts[1..5].iter().all(|f| f.kind == Kind::Bool(true)) {
                        return Err(format!("{phase}/{label}/stage{stage}: facts {form:?}"));
                    }
                    if named && parts[5].kind != Kind::Bool(true) {
                        return Err(format!("{phase}/{label}: no genuine FunctionName arity record observed"));
                    }
                    let Kind::Vector(rows) = &parts[0].kind else { return Err("missing rows".into()); };
                    let mut observed_ops = Vec::new();
                    for row in rows {
                        let Kind::Vector(row) = &row.kind else { return Err("invalid row".into()); };
                        if row.len() != 2 || row[1].kind != Kind::Bool(true) { return Err(format!("failed row: {row:?}")); }
                        let Kind::String(op) = &row[0].kind else { return Err("missing operation text".into()); };
                        observed_ops.push(String::from_utf16(op).map_err(|e| e.to_string())?);
                    }
                    for op in required_ops {
                        if !observed_ops.iter().any(|found| found == op) { return Err(format!("missing required {op}: {observed_ops:?}")); }
                    }
                }
                Ok(())
            })();
            let restored = original.restore(&mut macros.session).map_err(|e| e.to_string());
            observed?;
            restored?;
            Ok(())
        })();
        if let Err(error) = run { failures.push(error); }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn analyzer_literal_control_ast_keys_keep_initial_memos_and_cold_extensions() {
    run_proven_ast_literal_family("control",
        "(fn [n] (do (if n 17 (throw 19)) (loop [i 0] (if (< i 1) (recur (inc i)) i)) (try 17 (catch :default problem problem) (finally nil))))",
        &[":fn", ":do", ":if", ":throw", ":loop", ":recur", ":try"], false);
}

#[test]
fn analyzer_literal_quote_metadata_ast_keys_keep_source_payload_cold() {
    run_proven_ast_literal_family("quote-meta",
        "(fn [] (do '(source-word :source-key) ^{:reader-only :source-value} [17]))",
        &[":fn", ":do", ":quote", ":const", ":with-meta"], false);
}

#[test]
fn analyzer_literal_dispatch_and_named_fn_keys_preserve_live_hash_restoration() {
    run_proven_ast_literal_family("dispatch-named",
        "(fn ast-family-name [n] (do (set! ast-literal-target n) (identity n) (new js/Object)))",
        &[":fn", ":do", ":set!", ":invoke", ":new"], true);
}


#[test]
fn analyzer_literal_declaration_retained_keys_match_pinned_merge_boundaries() {
    let cases = [
        ("plain", "(def retained-watched 17)"),
        ("source-overrides", "(def ^{:name source-name :doc \"raw\" :meta {:leaf 17}} retained-watched 17)"),
        ("docstring-overrides-source", "(def ^{:doc \"raw\"} retained-watched \"docstring\" 17)"),
        ("function-raw-flags", "(def ^{:fn-var false :variadic? true :max-fixed-arity 9} retained-watched (fn [x] x))"),
        ("top-fn-overrides", "(def ^{:fn-var false :top-fn {:fn-var true :variadic? false :max-fixed-arity 3 :method-params [] :arglists [] :arglists-meta []}} retained-watched (fn [x] x))"),
    ];
    for (label, definition) in cases {
        for (phase, mut caller) in [("runtime", Session::new_repl().unwrap()), ("macro", Session::new_macro().unwrap())] {
            let mut macros = CompiledMacros::new().unwrap();
            macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
            macros.session.eval("(def leaf-original-method nil) (def retained-key-events [])").unwrap();
            let original = Method::capture(&mut macros.session, "Keyword").unwrap();
            original.bind_delegate(&mut macros.session).unwrap();
            macros.define(r#"(defmacro inspect-retained-key [stage which cold-doc]
              (let [decl (get (:defs (:ns &env)) 'retained-watched)
                    find-key (fn [m text] (reduce (fn [found key] (if (= (str key) text) key found)) nil (keys m)))
                    name-key (find-key decl ":name")
                    meta-map (:meta decl)
                    source-overrides (= which "source-overrides")
                    docstring (= which "docstring-overrides-source")
                    raw-flags (= which "function-raw-flags")
                    top (= which "top-fn-overrides")
                    raw-fields (cond source-overrides [":doc" ":meta"] docstring [":doc"]
                                     raw-flags [":fn-var" ":variadic?" ":max-fixed-arity"] top [":fn-var"] :else [])
                    raw-ok (every? (fn [text]
                                     (let [key (find-key decl text)
                                           source-key (find-key meta-map text)
                                           hits (filter (fn [event] (identical? key (first event))) retained-key-events)]
                                       (and (some? key) (identical? key source-key)
                                            (if (= stage 0) (and (seq hits) (nil? (second (first hits)))) (empty? hits))))) raw-fields)
                    name-hits (filter (fn [event] (identical? name-key (first event))) retained-key-events)
                    top-map (:top-fn meta-map)
                    top-ok (if top
                             (every? (fn [text]
                                       (let [key (find-key decl text)
                                             hits (filter (fn [event] (identical? key (first event))) retained-key-events)]
                                         (and (some? key) (identical? key (find-key top-map text))
                                              (if (= stage 0) (and (seq hits) (nil? (second (first hits)))) (empty? hits)))))
                                     [":variadic?" ":max-fixed-arity" ":method-params" ":arglists" ":arglists-meta"])
                             true)]
                (list 'quote
                  [(= (.-_hash name-key) 1843675177)
                   (if (= stage 0) (every? (fn [event] (= (second event) 1843675177)) name-hits) (empty? name-hits))
                   raw-ok top-ok (nil? (.-_hash cold-doc))
                   (if source-overrides (= (str (:name decl)) "source-name") true)
                   (if docstring (= (:doc decl) "docstring") true)
                   (if raw-flags (and (= (:fn-var decl) true) (= (:variadic? decl) false) (= (:max-fixed-arity decl) 1)) true)
                   (if top (and (= (:fn-var decl) true) (= (:max-fixed-arity decl) 3) (= (:method-params decl) [])) true)])))"#).unwrap();
            macros.session.eval(r#"(extend-type Keyword IHash (-hash [self]
              (let [memo (.-_hash self)]
                (do (if (or (= (.-name self) "name") (= (.-name self) "doc") (= (.-name self) "meta")
                            (= (.-name self) "fn-var") (= (.-name self) "variadic?") (= (.-name self) "max-fixed-arity")
                            (= (.-name self) "method-params") (= (.-name self) "arglists") (= (.-name self) "arglists-meta"))
                      (set! retained-key-events (conj retained-key-events [self memo])) nil)
                    (if (nil? memo) (leaf-original-method self) memo)))))"#).unwrap();
            let observed = (|| -> Result<(), String> {
                caller.eval(definition).map_err(|e| e.to_string())?;
                for stage in 0..2 {
                    if stage == 1 {
                        original.restore(&mut macros.session).map_err(|e| e.to_string())?;
                        macros.session.eval("(set! retained-key-events [])").map_err(|e| e.to_string())?;
                    }
                    caller.collect().map_err(|e| e.to_string())?;
                    macros.session.collect().map_err(|e| e.to_string())?;
                    let source = format!("(inspect-retained-key {stage} \"{label}\" :doc)");
                    let value = caller.eval_with_macros(&source, &mut macros).map_err(|e| e.to_string())?;
                    caller.collect().map_err(|e| e.to_string())?;
                    let bridge = FormBridge::new(&mut caller).map_err(|e| e.to_string())?;
                    let form = bridge.read(&mut caller, &value, 0..1).map_err(|e| e.to_string())?;
                    eprintln!("retained-declaration-key {phase}/{label}/stage{stage}: {:?}", form.kind);
                    if !matches!(&form.kind, Kind::Vector(values) if values.len()==9 && values.iter().all(|v|v.kind==Kind::Bool(true))) {
                        return Err(format!("{phase}/{label}/stage{stage}: {form:?}"));
                    }
                }
                Ok(())
            })();
            let restored = original.restore(&mut macros.session);
            observed.unwrap(); restored.unwrap();
        }
    }
}

#[test]
fn analyzer_literal_local_reference_select_keys_are_independent_of_binding_role() {
    for mut caller in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
        macros.session.eval("(def leaf-original-method nil) (def reference-key-events [])").unwrap();
        let original = Method::capture(&mut macros.session, "Keyword").unwrap();
        original.bind_delegate(&mut macros.session).unwrap();
        macros.define(r#"(defmacro inspect-reference-keys [stage name cold-local]
          (let [root (:init (get (:locals &env) name))
                find-key (fn [node text] (reduce (fn [found key] (if (= (str key) text) key found)) nil (keys node)))
                walk (fn walk [node]
                       (let [children (reduce (fn [out key] (let [value (get node key)]
                                                             (if (map? value) (conj out value)
                                                               (if (vector? value) (into out value) out)))) [] (:children node))
                             own (if (= (:op node) :local) [node] [])]
                         (reduce (fn [out child] (into out (walk child))) own children)))
                refs (walk root)
                valid (every? (fn [node]
                                (every? (fn [pair]
                                          (let [key (find-key node (first pair))
                                                hits (filter (fn [event] (identical? key (first event))) reference-key-events)]
                                            (and (some? key) (= (.-_hash key) (second pair))
                                                 (if (= stage 0) (and (seq hits) (every? (fn [event] (= (second event) (second pair))) hits)) (empty? hits)))))
                                        [[":info" -317069002] [":form" -1624062471] [":local" -1497766724]])) refs)]
            (list 'quote [(pos? (count refs)) valid (nil? (.-_hash cold-local))])))"#).unwrap();
        macros.session.eval(r#"(extend-type Keyword IHash (-hash [self]
          (let [memo (.-_hash self)]
            (do (if (or (= (.-name self) "info") (= (.-name self) "form") (= (.-name self) "local"))
                  (set! reference-key-events (conj reference-key-events [self memo])) nil)
                (if (nil? memo) (leaf-original-method self) memo)))))"#).unwrap();
        let result = (|| -> Result<(), String> {
            for stage in 0..2 {
                if stage == 1 {
                    original.restore(&mut macros.session).map_err(|e|e.to_string())?;
                    macros.session.eval("(set! reference-key-events [])").map_err(|e|e.to_string())?;
                }
                caller.collect().map_err(|e|e.to_string())?;
                macros.session.collect().map_err(|e|e.to_string())?;
                let value = caller.eval_with_macros(&format!("(let [reference-f (fn [arg] (try (do arg 17) (catch :default caught caught)))] (inspect-reference-keys {stage} reference-f :local))"), &mut macros).map_err(|e|e.to_string())?;
                caller.collect().map_err(|e|e.to_string())?;
                let bridge = FormBridge::new(&mut caller).map_err(|e|e.to_string())?;
                let form = bridge.read(&mut caller, &value, 0..1).map_err(|e|e.to_string())?;
                if !matches!(&form.kind,Kind::Vector(v) if v.len()==3 && v.iter().all(|f| f.kind==Kind::Bool(true))) {
                    return Err(format!("stage{stage}: {form:?}"));
                }
            }
            Ok(())
        })();
        let restored = original.restore(&mut macros.session);
        result.unwrap(); restored.unwrap();
    }
}
