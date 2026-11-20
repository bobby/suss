//! Retaining an immutable receiver layout must keep field values live.
use suss_cli::portable_session::{Session, SessionValue};

fn text(session: &mut Session, value: &SessionValue) -> String {
    let units = session.inspect(value, |mut store, value| {
        let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
        Ok(array.elems(&mut store)?.map(|unit| unit.unwrap_i32() as u16).collect::<Vec<_>>())
    }).unwrap();
    String::from_utf16(&units).unwrap()
}

fn printed(session: &mut Session, source: &str) -> String {
    let value = session.eval(&format!(
        "(pr-str (try {source} (catch :default error [:error (ex-message error)])))")).unwrap();
    text(session, &value)
}

#[test]
fn core_receiver_views_keep_live_fields_callbacks_and_distinct_layouts() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(deftype LayoutA [^:mutable x ^:mutable y])
            (deftype LayoutB [padding ^:mutable y ^:mutable x])
            (def events (atom []))
            (def a (LayoutA. 1 2)) (def b (LayoutB. :padding 10 20))").unwrap();
        session.eval_privileged("(def layout-read (fn [owner change skip]
            (if skip :skipped
                (let [a (.-x owner) _ (change owner) b (.-x owner) c (.-y owner)]
                    [a b c]))))").unwrap();
        // Core callbacks may access these internal mutable fields. Ordinary
        // source continues to use the enclosing-type privacy checks.
        session.eval_privileged("(def layout-change (fn [owner] (swap! events conj (.-x owner))
            (set! (.-x owner) (inc (.-x owner)))
            (set! (.-y owner) (+ 2 (.-y owner)))))").unwrap();
        let change = "layout-change";
        assert_eq!(printed(&mut session, &format!(
            "[(layout-read a {change} false) (layout-read b {change} false) @events]")),
            "[[1 2 4] [20 21 12] [1 20]]");
        session.collect().unwrap();
        // Old objects retain their original layout after a new class definition.
        session.eval("(deftype LayoutA [padding ^:mutable y ^:mutable x])
            (def c (LayoutA. :new 100 200))").unwrap();
        assert_eq!(printed(&mut session, &format!(
            "[(layout-read a {change} false) (layout-read c {change} false) @events]")),
            "[[2 3 6] [200 201 102] [1 20 2 200]]");
        assert_eq!(printed(&mut session,
            "[(layout-read nil nil true) (layout-read false nil true) (layout-read 1 nil true)]"),
            "[:skipped :skipped :skipped]");
    }
}

#[test]
fn core_receiver_views_keep_method_extension_and_non_object_fallbacks() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(deftype LayoutMethod [] Object (value [this] :old))
            (def owner (LayoutMethod.))").unwrap();
        session.eval_privileged("(def layout-methods (fn [owner change]
            (let [a (.-value owner) _ (change) b (.-value owner) c (.-value owner)]
                [(identical? a b) (identical? b c) (.value owner)])))
            (def layout-length (fn [owner]
                [(.-length owner) (.-length owner) (.-length owner)]))").unwrap();
        // The cached layout cannot turn a method lookup into a retained value.
        assert_eq!(printed(&mut session,
            "(layout-methods owner (fn []
                (extend-type LayoutMethod Object (value [this] :new))))"), "[false true :new]");
        session.collect().unwrap();
        assert_eq!(printed(&mut session, "(layout-length \"abc\")"), "[3 3 3]");
    }
}
