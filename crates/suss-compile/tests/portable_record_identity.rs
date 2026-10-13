//! Intrinsic prerequisites for record equality; public defrecord acceptance is separate.
#[path = "support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};
use suss_compile::portable_session::Session;

#[test]
fn record_descriptor_identity_ignores_constructor_override_and_preserves_order_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let source = include_str!("record_nominal_identity.sus");
        let boundary = source.find("(deftype Before [x])").unwrap();
        session.eval(&source[..boundary]).unwrap();
        session.collect().unwrap();
        session.eval(&source[boundary..]).unwrap();
        session.collect().unwrap();
        let result = session.eval("(= [before-redefinition after-redefinition order-result throw-result @effects constructor-override-result] [[true false false false false false false false] [true false true] true true [:left :right :throw] :overridden])").unwrap();
        session.collect().unwrap();
        session
            .inspect(&result, |mut store, value| {
                assert_eq!(
                    Decoder::new(200).decode(&mut store, &value)?,
                    Observation::Bool(true)
                );
                Ok(())
            })
            .unwrap();
    }
}
