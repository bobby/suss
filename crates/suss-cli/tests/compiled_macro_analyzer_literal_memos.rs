use suss_cli::{portable_macro_data::FormBridge, portable_macros::CompiledMacros, portable_session::Session};
use suss_reader::forms::Kind;
#[test]
fn fresh_env_literal_keys_keep_reader_arguments_and_source_metadata_cold_after_gc() {
    let mut failures = Vec::new();
    for (phase, mut caller) in [("runtime",Session::new_repl().unwrap()),("macro",Session::new_macro().unwrap())] {
        let result = (|| -> Result<(), String> {
            caller.eval("(def ^{:audit-leaf :reader-only-leaf} watched 17)").map_err(|e| e.to_string())?;
            let mut macros = CompiledMacros::new().map_err(|e|e.to_string())?;
            macros.define(r#"(defmacro inspect-literal-memos [source-kw source-sym]
              (let [find-key (fn [m text]
                     (reduce (fn [found key] (if (= (str key) text) key found)) nil (keys m)))
                    descriptor (get (:locals &env) 'local-x)
                    metadata (:meta (get (:defs (:ns &env)) 'watched))
                    root-key (find-key &env ":locals")
                    op-key (find-key descriptor ":op")
                    env-key (find-key (:init descriptor) ":env")
                    tag-key (find-key descriptor ":tag")]
                (if (and root-key op-key env-key tag-key)
                 (list 'quote
                  [(= (.-_hash root-key) 535295783)
                   (= (.-_hash op-key) -1882987955)
                   (= (.-_hash env-key) -1815813235)
                   (= (.-_hash tag-key) -1290361223)
                   (nil? (.-_hash source-kw)) (nil? (.-_hash source-sym))
                   (nil? (.-_hash (:audit-leaf metadata)))
                   (= source-kw :op) (= (str source-sym) "op")
                   (= 17 (:val (:init descriptor)))])
                 (list 'quote [:missing-key
                   (vec (map str (keys &env)))
                   (vec (map str (keys descriptor)))
                   (vec (map str (keys (:init descriptor))))]))))"#).map_err(|e|e.to_string())?;
            for stage in 0..2 {
                caller.collect().map_err(|e|e.to_string())?;
                let value=caller.eval_with_macros("(let [^number local-x 17] (inspect-literal-memos :op op))",&mut macros).map_err(|e|format!("stage{stage}: {e}"))?;
                let bridge=FormBridge::new(&mut caller).map_err(|e|e.to_string())?;
                let form=bridge.read(&mut caller,&value,0..1).map_err(|e|e.to_string())?;
                eprintln!("analyzer-literal-env {phase}/stage{stage}: {:?}",form.kind);
                match &form.kind {Kind::Vector(values) if values.len()==10 && values.iter().all(|v|v.kind==Kind::Bool(true))=>{},other=>return Err(format!("stage{stage}: {other:?}"))}
            }
            Ok(())
        })();
        if let Err(error)=result {failures.push(format!("{phase}: {error}"));}
    }
    assert!(failures.is_empty(),"{}",failures.join("\n"));
}

#[test]
fn vector_source_literal_items_children_keys_and_child_keyword_have_initialized_memos() {
    for mut caller in [Session::new_repl().unwrap(),Session::new_macro().unwrap()] {
        let mut macros=CompiledMacros::new().unwrap();
        macros.define(r#"(defmacro inspect-vector-literals [name]
          (let [find-key (fn [m text] (reduce (fn [found key] (if (= (str key) text) key found)) nil (keys m)))
                ast (:init (get (:locals &env) name))
                items-key (find-key ast ":items")
                children-key (find-key ast ":children")
                child-name (first (:children ast))]
            (list 'quote [(some? (.-_hash items-key)) (some? (.-_hash children-key))
                         (some? (.-_hash child-name)) (= :items child-name)
                         (= 17 (:val (first (:items ast))))])))"#).unwrap();
        caller.collect().unwrap();
        let value=caller.eval_with_macros("(let [v [17]] (inspect-vector-literals v))",&mut macros).unwrap();
        let bridge=FormBridge::new(&mut caller).unwrap();
        let form=bridge.read(&mut caller,&value,0..1).unwrap();
        eprintln!("analyzer-literal-vector: {:?}",form.kind);
        assert!(matches!(&form.kind,Kind::Vector(values) if values.len()==5 && values.iter().all(|v|v.kind==Kind::Bool(true))));
    }
}
