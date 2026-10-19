//! Actual public record source -> component -> execution under normal CLI limits.
//! Fixture includes separate MPL-2.0 Jank excerpts; see its retained provenance.
#[path = "support/compilation_fuel.rs"]
mod compilation_fuel;
use std::process::Command;
#[test]
fn public_records_execute_jank_and_authored_contracts_with_two_nominal_classes() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    std::fs::write(
        root.path().join("src/app_records.sus"),
        include_str!("../../../tests/oracle/fixtures/public-record-contracts.sus"),
    )
    .unwrap();
    std::fs::write(
        root.path().join("records.wit"),
        include_str!("../../../tests/oracle/fixtures/public-record-contracts.wit"),
    )
    .unwrap();
    let exports = [
        "value",
        "jank-dissoc",
        "jank-empty",
        "nominal-hash",
        "metadata-ops",
        "factory-values",
        "reduced-effects",
        "hash-invalidation",
        "constructor-effects",
    ];
    let mut command = Command::new(env!("CARGO_BIN_EXE_suss"));
    command.current_dir(root.path()).args([
        "compile",
        "-n",
        "app-records",
        "--src",
        "src",
        "-w",
        "records.wit",
        "--wit-world",
        "contracts",
        "-o",
        "records.wasm",
    ]);
    for export in exports {
        command
            .arg("--export")
            .arg(format!("{export}=app-records/{export}"));
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for export in exports {
        let output = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args(["run", "records.wasm", "--invoke", export])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{export}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = if export == "value" { "17" } else { "1" };
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            expected,
            "{export}"
        );
    }
}

#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;
#[test]
fn public_record_field_and_metadata_values_decode_independently_after_gc() {
    use portable_decode::{Decoder, Observation};
    use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
        session
            .eval_with_macros(
                include_str!("../../../tests/oracle/fixtures/public-record-contracts.sus"),
                &mut macros,
            )
            .unwrap();
        let value = session.eval("(def observed-record (with-meta (map->TestDissocRecord {:a 17 :b false :extra nil}) {:tag :record})) [(.-a observed-record) (.-b observed-record) (.-c observed-record) (.-__meta observed-record) (.-__extmap observed-record) (count observed-record)]").unwrap();
        session.collect().unwrap();
        decoder.refill(4096);
        let actual = decoder.decode_session(&mut session, &value).unwrap();
        let keyword = |name: &str| Observation::Keyword(None, name.encode_utf16().collect());
        assert_eq!(
            actual,
            Observation::Vector(vec![
                Observation::Number(17.0_f64.to_bits()),
                Observation::Bool(false),
                Observation::Nil,
                Observation::Map(vec![(keyword("tag"), keyword("record"))]),
                Observation::Map(vec![(keyword("extra"), Observation::Nil)]),
                Observation::Number(4.0_f64.to_bits()),
            ])
        );
        let value = session.eval("(let [changed (assoc observed-record :a 31) removed (dissoc observed-record :a)] [(.-a changed) (.-a observed-record) (meta changed) (meta removed) removed])").unwrap();
        session.collect().unwrap();
        decoder.refill(4096);
        let mut actual = decoder.decode_session(&mut session, &value).unwrap();
        // Map iteration order is undefined; compare decoded entries by key.
        if let Observation::Vector(values) = &mut actual {
            if let Observation::Map(pairs) = values.last_mut().unwrap() {
                pairs.sort_by(|(left, _), (right, _)| match (left, right) {
                    (
                        Observation::Keyword(left_ns, left_name),
                        Observation::Keyword(right_ns, right_name),
                    ) => (left_ns, left_name).cmp(&(right_ns, right_name)),
                    _ => panic!("expected decoded keyword keys"),
                });
            }
        }
        assert_eq!(
            actual,
            Observation::Vector(vec![
                Observation::Number(31.0_f64.to_bits()),
                Observation::Number(17.0_f64.to_bits()),
                Observation::Map(vec![(keyword("tag"), keyword("record"))]),
                Observation::Map(vec![(keyword("tag"), keyword("record"))]),
                Observation::Map(vec![
                    (keyword("b"), Observation::Bool(false)),
                    (keyword("c"), Observation::Nil),
                    (keyword("extra"), Observation::Nil)
                ]),
            ])
        );
    }
}

#[test]
fn public_record_custom_protocols_and_record_input_factory_decode_after_gc() {
    use portable_decode::{Decoder, Observation};
    use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
        session
            .eval_with_macros(
                "(ns record-custom) (defprotocol ValueProbe (-probe [this])) (defrecord Custom [value] ValueProbe (-probe [this] 73) ICounted (-count [this] 99))",
                &mut macros,
            )
            .unwrap();
        let value = session
            .eval(
                "(def original (with-meta (assoc (->Custom false) :extra nil) {:tag :old}))
             (def copied (map->Custom original))
             [(count copied) (:value copied) (contains? copied :extra)
              (:extra copied) (meta copied) (:tag (meta original))
              (.-__hash copied) (-probe copied)]",
            )
            .unwrap();
        session.collect().unwrap();
        decoder.refill(4096);
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Vector(vec![
                // Pinned CLJS warns about duplicate ICounted and generated count wins.
                Observation::Number(2.0_f64.to_bits()),
                Observation::Bool(false),
                Observation::Bool(true),
                Observation::Nil,
                Observation::Nil,
                Observation::Keyword(None, "old".encode_utf16().collect()),
                Observation::Nil,
                Observation::Number(73.0_f64.to_bits()),
            ])
        );
    }
}

#[test]
fn public_record_nominal_equality_and_predicate_decode_after_gc_in_both_phases() {
    use portable_decode::{Decoder, Observation};
    use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};

    const UPSTREAM: &str = "c4295f303100bbf5afac449242d30bca1126f1a1";
    const FIXTURE: &str = "tests/oracle/fixtures/public-record-contracts.sus";
    const FIXTURE_SHA256: &str = "471fd42576aaabbc7967f4ed25fb0e0159a9d71382b5255f2716aec3832df8a7";
    const CASE_ID: &str = "public-record-nominal-contract";
    const FIXTURE_SOURCE: &str =
        include_str!("../../../tests/oracle/fixtures/public-record-contracts.sus");
    assert_eq!(
        suss_compile::portable::bootstrap::sha256(FIXTURE_SOURCE.as_bytes()),
        FIXTURE_SHA256
    );
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/record-nominal-cases.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(corpus["upstream"], UPSTREAM);
    assert_eq!(corpus["fixture"], FIXTURE);
    assert_eq!(corpus["fixture_sha256"], FIXTURE_SHA256);
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1);
    let case = &cases[0];
    assert_eq!(case["id"], CASE_ID);
    let query = case["source"].as_str().unwrap();
    let expected = case["expected"].as_object().unwrap();
    assert_eq!(expected.len(), 2);
    assert_eq!(expected["tag"], "vector");
    let expected_items = expected["items"].as_array().unwrap();
    assert_eq!(expected_items.len(), 10);
    let expected = Observation::Vector(
        expected_items
            .iter()
            .map(|item| {
                assert_eq!(item.as_object().unwrap().len(), 2);
                assert_eq!(item["tag"], "bool");
                Observation::Bool(item["value"].as_bool().unwrap())
            })
            .collect(),
    );

    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // This decoder lives outside both Wasm modules and observes the result
        // only after collection; the guest never prints or re-evaluates it.
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
        session
            .eval_with_macros(FIXTURE_SOURCE, &mut macros)
            .unwrap();
        session.enter_namespace("app-records").unwrap();
        macros.enter_namespace("app-records").unwrap();
        let actual = session.eval(query).unwrap();
        session.collect().unwrap();
        decoder.refill(4096);
        assert_eq!(decoder.decode_session(&mut session, &actual).unwrap(), expected);
    }
}
