//! Source namespaces must prepare local transformers in the compiled Macro phase.
use suss_cli::{portable_macros::CompiledMacros, portable_session::{Session, SessionOptions}};
use suss_compile::portable::resolve::Phase;
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};

#[test]
fn source_namespace_local_macros_use_phase_imports_and_expand_following_forms() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("local")).unwrap();
    std::fs::write(root.path().join("local/reader.sus"), r##"(ns local.reader
  (:require [clojure.edn :as edn]))
(defmacro data [text] (list 'quote (edn/read-string text)))
(defmacro wrapped [text] `(vector (data ~text) :ok))
(def answer (wrapped "[1.25M #uuid \"1-2-3-4-5\"]"))
"##).unwrap();
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(SessionOptions {
            source_paths: vec![root.path().to_owned()],
            ..SessionOptions::default()
        }, phase).unwrap();
        session.eval(include_str!("../../../runtime/core-import/suss/core.sus")).unwrap();
        session.enter_namespace("user").unwrap();
        let mut decoder = Decoder::capture(&mut session, 20_000).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        session.load_namespace_with_macros("local.reader", &mut macros).unwrap();
        assert_eq!(session.current_namespace(), "user");
        let value = session.eval("local.reader/answer").unwrap();
        session.collect().unwrap();
        assert_eq!(decoder.decode_session(&mut session, &value).unwrap(), Observation::Vector(vec![
            Observation::Vector(vec![
                Observation::Decimal { unscaled: "125".into(), scale: 2 },
                Observation::Uuid { most: 0x100020003, least: 0x4000000000005 },
            ]),
            Observation::Keyword(None, "ok".encode_utf16().collect()),
        ]), "{phase:?}");
    }
}
