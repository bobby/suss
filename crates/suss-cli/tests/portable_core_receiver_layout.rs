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

#[test]
fn core_field_writes_keep_values_layout_changes_and_reentrant_rhs() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(deftype WriteA [^:mutable x ^:mutable y])
            (deftype WriteB [^:mutable y ^:mutable x])
            (def a (WriteA. 1 10)) (def b (WriteB. 20 2))
            (def write-events (atom []))").unwrap();
        session.eval_privileged("(def core-write (fn [owner rhs] (set! (.-x owner) (rhs))))
            (def core-x (fn [owner] (.-x owner)))
            (def core-write-read (fn [owner rhs]
                (let [a (.-x owner) b (.-y owner) c (.-x owner)
                      result (set! (.-x owner) (rhs))]
                    [a b c result (.-x owner) (.-y owner)])))").unwrap();
        assert_eq!(printed(&mut session,
            "[(core-write a (fn [] false)) (core-x a)
              (core-write b (fn [] nil)) (core-x b)
              (core-write a (fn [] :value)) (core-x a)]"),
            "[false false nil nil :value :value]");
        // The inner call fills this very write site's cache with B before
        // the outer A write selects its fast or fallback path.
        assert_eq!(printed(&mut session,
            "[(core-write a (fn [] (swap! write-events conj :outer)
                 (core-write b (fn [] (swap! write-events conj :inner) :b)) :a))
              (core-x a) (core-x b) @write-events]"),
            "[:a :a :b [:outer :inner]]");
        assert_eq!(printed(&mut session,
            "[(core-write-read a (fn [] :first)) (core-write-read a (fn [] :second))]"),
            "[[:a 10 :a :first :first 10] [:first 10 :first :second :second 10]]");
        session.collect().unwrap();
        session.eval("(deftype WriteA [padding ^:mutable y ^:mutable x])
            (def c (WriteA. :new 30 3))").unwrap();
        assert_eq!(printed(&mut session,
            "[(core-write c (fn [] :new)) (core-x c)
              (core-write a (fn [] :old)) (core-x a)]"),
            "[:new :new :old :old]");
    }
}

#[test]
fn core_field_write_misses_keep_rhs_order_errors_and_method_updates() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(deftype WriteValue [^:mutable x])
            (def owner (WriteValue. 1)) (def write-events (atom []))").unwrap();
        session.eval_privileged("(def core-write (fn [owner rhs] (set! (.-x owner) (rhs))))
            (def core-x (fn [owner] (.-x owner)))
            (def core-method-write (fn [owner value] (set! (.-method owner) value)))").unwrap();
        assert_eq!(printed(&mut session, "(core-write owner (fn [] 2))"), "2");
        for owner in ["nil", "false", "1", "\"x\""] {
            assert_eq!(printed(&mut session, &format!(
                "(let [threw (try (core-write (do (swap! write-events conj :owner) {owner})
                            (fn [] (swap! write-events conj :rhs) :value))
                           false (catch :default error true))]
                    [threw @write-events])")),
                "[true [:owner :rhs]]");
            session.eval("(reset! write-events [])").unwrap();
        }
        // An absent field remains an error, not an array store. A closure's
        // named properties still use the runtime's property/epoch update.
        assert_eq!(printed(&mut session,
            "(try (core-method-write owner :absent) false (catch :default error true))"), "true");
        session.eval("(def static-holder (fn [] nil))").unwrap();
        session.eval_privileged("(core-method-write static-holder (fn [] :first))").unwrap();
        assert_eq!(printed(&mut session, "((.-method static-holder))"), ":first");
        session.eval_privileged("(core-method-write static-holder (fn [] :second))").unwrap();
        session.collect().unwrap();
        assert_eq!(printed(&mut session, "((.-method static-holder))"), ":second");
        // A thrown RHS leaves both the warm field and its read unchanged.
        assert_eq!(printed(&mut session,
            "[(try (core-write owner (fn [] (throw (ex-info \"rhs\" {}))))
                   (catch :default error (ex-message error)))
              (core-x owner)]"), "[\"rhs\" 2]");
    }
}
