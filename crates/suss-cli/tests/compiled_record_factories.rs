//! Complete compiled factory helper regressions, not public defrecord acceptance.
//! Public defrecord acceptance is checked separately from these helper cases.
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;

use portable_decode::{Decoder, Observation};
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};

fn number(value: f64) -> Observation {
    Observation::Number(value.to_bits())
}

fn observe(
    session: &mut Session,
    macros: &mut CompiledMacros,
    decoder: &mut Decoder,
    source: &str,
) -> Observation {
    decoder.refill(4096);
    let value = session
        .eval_with_macros(source, macros)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    session.collect().unwrap();
    decoder
        .decode_session(session, &value)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"))
}

fn factory_tools(macros: &mut CompiledMacros) {
    macros.define("(defmacro record-positional [name fields] (suss.internal.record-factories/build-positional-factory (with-meta name {:internal-ctor true}) (symbol (str (get (get &env :ns) :name)) (str name)) fields))").unwrap();
    macros.define("(defmacro record-map [name fields] (suss.internal.record-factories/build-map-factory name (symbol (str (get (get &env :ns) :name)) (str name)) fields))").unwrap();
    macros.define("(defmacro plain-positional [name fields] (suss.internal.record-factories/build-positional-factory name (symbol (str (get (get &env :ns) :name)) (str name)) fields))").unwrap();
    macros.define("(defmacro factory-metadata [name] (let [info (get (get (get &env :suss/catalog) :defs) name)] (list 'quote [(get (get info :meta) :factory) (get info :doc)])))").unwrap();
}

#[test]
fn compiled_positional_factories_use_declared_arity_and_internal_nil_fields() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        factory_tools(&mut macros);
        session.eval_with_macros("(defrecord* FactoryRecord [left right] nil nil) (record-positional FactoryRecord [left right]) (deftype* FactoryType [value] nil nil) (plain-positional FactoryType [value])", &mut macros).unwrap();
        assert_eq!(
            observe(
                &mut session,
                &mut macros,
                &mut decoder,
                "(let [r (->FactoryRecord 7 false)] [(.-left r) (.-right r) (.-__meta r) (.-__extmap r) (.-__hash r)])"
            ),
            Observation::Vector(vec![
                number(7.0),
                Observation::Bool(false),
                Observation::Nil,
                Observation::Nil,
                Observation::Nil
            ])
        );
        assert_eq!(
            observe(
                &mut session,
                &mut macros,
                &mut decoder,
                "(.-value (->FactoryType 17))"
            ),
            number(17.0)
        );
        assert!(session.eval("(->FactoryRecord 7)").is_err());
        assert!(session.eval("(->FactoryRecord 7 false 9)").is_err());
        let metadata = observe(
            &mut session,
            &mut macros,
            &mut decoder,
            "(factory-metadata ->FactoryRecord)",
        );
        assert_eq!(
            metadata,
            Observation::Vector(vec![
                Observation::Keyword(None, "positional".encode_utf16().collect()),
                Observation::String(
                    "Positional factory function for user/FactoryRecord."
                        .encode_utf16()
                        .collect()
                )
            ])
        );
    }
}

#[test]
fn compiled_map_factory_preserves_missing_nil_false_values_and_extension_entries() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        factory_tools(&mut macros);
        session.eval_with_macros("(defrecord* MapFactoryRecord [left right] nil nil) (record-map MapFactoryRecord [left right]) (defrecord* EmptyFactoryRecord [] nil nil) (record-map EmptyFactoryRecord [])", &mut macros).unwrap();
        assert_eq!(
            observe(
                &mut session,
                &mut macros,
                &mut decoder,
                "(let [r (map->MapFactoryRecord {:left 7 :right false :extra nil})] [(.-left r) (.-right r) (.-__meta r) (.-__extmap r) (.-__hash r)])"
            ),
            Observation::Vector(vec![
                number(7.0),
                Observation::Bool(false),
                Observation::Nil,
                Observation::Map(vec![(
                    Observation::Keyword(None, "extra".encode_utf16().collect()),
                    Observation::Nil
                )]),
                Observation::Nil
            ])
        );
        assert_eq!(
            observe(
                &mut session,
                &mut macros,
                &mut decoder,
                "(let [r (map->MapFactoryRecord {:left nil})] [(.-left r) (.-right r) (.-__extmap r)])"
            ),
            Observation::Vector(vec![Observation::Nil, Observation::Nil, Observation::Nil])
        );
        assert_eq!(
            observe(
                &mut session,
                &mut macros,
                &mut decoder,
                "(let [r (map->EmptyFactoryRecord {})] [(.-__meta r) (.-__extmap r) (.-__hash r)])"
            ),
            Observation::Vector(vec![Observation::Nil, Observation::Nil, Observation::Nil])
        );
        assert_eq!(
            observe(
                &mut session,
                &mut macros,
                &mut decoder,
                "(.-__extmap (map->EmptyFactoryRecord {nil false}))"
            ),
            Observation::Map(vec![(Observation::Nil, Observation::Bool(false))])
        );
        let metadata = observe(
            &mut session,
            &mut macros,
            &mut decoder,
            "(factory-metadata map->MapFactoryRecord)",
        );
        assert_eq!(metadata, Observation::Vector(vec![Observation::Keyword(None, "map".encode_utf16().collect()), Observation::String("Factory function for user/MapFactoryRecord, taking a map of keywords to field values.".encode_utf16().collect())]));
    }
}
