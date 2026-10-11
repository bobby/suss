//! Portable `seqable?` source contract; JavaScript-only iterable interop is
//! explicitly outside the accepted portable host profile.
use suss_cli::portable_session::{Session, SessionOptions};
use suss_compile::portable::resolve::Phase;
use wasmtime::Val;

#[test]
fn seqable_predicate_covers_portable_values_in_both_phases_after_gc() {
    let cases = [
        ("nil", "(seqable? nil)", true),
        ("vector", "(seqable? [1])", true),
        ("list", "(seqable? (list 1))", true),
        ("sorted-map", "(seqable? (sorted-map 1 2))", true),
        ("sorted-set", "(seqable? (sorted-set 1))", true),
        ("source-array", "(seqable? (array 1))", true),
        ("string", "(seqable? \"x\")", true),
        ("custom-ISeqable", "(seqable? (SeqableProbe.))", true),
        ("number", "(seqable? 1)", false),
        ("false", "(seqable? false)", false),
        ("keyword", "(seqable? :k)", false),
        // Foreign JavaScript iterator interop is excluded by the portable
        // profile; a foreign value must be wrapped in ISeqable/IIterable.
        ("plain-js-object", "(seqable? (js-obj))", false),
    ];
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
        session.set_operation_fuel(100_000_000);
        session
            .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
            .unwrap();
        session.enter_namespace("user").unwrap();
        session
            .eval("(deftype SeqableProbe [] ISeqable (-seq [_] (list 1)))")
            .unwrap();
        for (id, source, expected) in cases {
            let value = session
                .eval(source)
                .unwrap_or_else(|error| panic!("{phase:?} {id}: {error}"));
            session.collect().unwrap();
            session
                .inspect(&value, |store, value| {
                    assert_eq!(
                        value.unwrap_anyref().unwrap().as_i31(&store)?.unwrap().get_u32(),
                        if expected { 4 } else { 2 },
                        "{phase:?} {id}"
                    );
                    Ok(())
                })
                .unwrap();
        }
    }
}
