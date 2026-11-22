//! Genuine recursive binding snapshots stay separate from their runtime array.
use suss_cli::{portable_session::Session, portable_macros::CompiledMacros};

fn rendered(session: &mut Session, macros: &mut CompiledMacros, source: &str) -> String {
    let value = session.eval_with_macros(&format!("(pr-str {source})"), macros).unwrap();
    session.collect().unwrap();
    let units = session.inspect(&value, |mut store, value| {
        let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
        Ok(array.elems(&mut store)?.map(|unit| unit.unwrap_i32() as u16).collect::<Vec<_>>())
    }).unwrap();
    String::from_utf16(&units).unwrap()
}

#[test]
fn staged_recursive_bindings_and_complete_initializers_have_distinct_snapshots() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros.define("(defmacro inspect-letfn [name] (let [b (get (get &env :locals) name)] (list 'quote [(get b :local) (contains? b :init) (get (get b :init) :op)])))").unwrap();
        assert_eq!(rendered(&mut session, &mut macros,
            "(letfn [(f [] (inspect-letfn g)) (g [] 9)] [(f) (inspect-letfn f) (inspect-letfn g)])"),
            "[[:letfn false nil] [:letfn true :fn] [:letfn true :fn]]");
        macros.define("(defmacro inspect-group [name] (let [ast (get (get (get &env :locals) name) :init) bs (get ast :bindings)] (list 'quote [(get ast :op) (mapv (fn [b] [(get b :local) (get (get b :init) :op)]) bs) (get ast :children)])))").unwrap();
        assert_eq!(rendered(&mut session, &mut macros,
            "(let [owner (letfn [(f [] g) (g [] f)] f)] (inspect-group owner))"),
            "[:letfn [[:letfn :fn] [:letfn :fn]] [:bindings :body]]");
    }
}
