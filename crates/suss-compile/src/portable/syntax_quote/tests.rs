use super::*;
use serde_json::{Value, json};
use suss_reader::forms::{read_forms, read_forms_with_source_metadata, resolve_conditionals};

fn read(source: &str) -> Form {
    resolve_conditionals(read_forms(source).unwrap())
        .unwrap()
        .remove(0)
}
fn project(form: &Form) -> Value {
    match &form.kind {
        Kind::Nil => Value::Null,
        Kind::Bool(v) => json!(v),
        Kind::Number(v) => json!(v),
        Kind::String(v) => json!(String::from_utf16(v).unwrap()),
        Kind::Symbol(v) => json!(["symbol", v.to_string()]),
        Kind::Keyword(v) => json!(["keyword", v.to_string()]),
        Kind::List(v) => json!(["seq", v.iter().map(project).collect::<Vec<_>>()]),
        Kind::Vector(v) => json!(["vector", v.iter().map(project).collect::<Vec<_>>()]),
        Kind::Map(_) | Kind::Set(_) => panic!("Reader collection should have expanded"),
        _ => panic!("Unresolved reader data"),
    }
}
// Normalize only numeric generated IDs, consistently by their first occurrence.
// Keep spelling, qualification, distinctness and repeated use fully observable.
fn normalize(value: &mut Value, ids: &mut BTreeMap<String, usize>) {
    if let Value::Number(number) = value {
        // The development reader prints JVM integral literals as JSON integers;
        // portable source numbers use binary64. Compare at that contract.
        *value = json!(number.as_f64().unwrap());
        return;
    }
    if let Value::Array(items) = value {
        if items.first() == Some(&json!("symbol")) {
            let spelling = items[1].as_str().unwrap();
            if let Some(stem) = spelling.strip_suffix("__auto__") {
                if let Some((prefix, id)) = stem.rsplit_once("__") {
                    assert!(id.parse::<u64>().is_ok(), "Malformed generated ID");
                    let next = ids.len();
                    let number = *ids.entry(id.to_owned()).or_insert(next);
                    items[1] = json!(format!("{prefix}__{number}__auto__"));
                }
            }
        } else {
            for item in items {
                normalize(item, ids);
            }
        }
    }
}
fn equivalent(actual: &Form, mut expected: Value) {
    let mut actual = project(actual);
    normalize(&mut actual, &mut BTreeMap::new());
    normalize(&mut expected, &mut BTreeMap::new());
    pretty_assertions::assert_eq!(actual, expected);
}

#[test]
fn pinned_reader_trees_match_with_generated_identity_preserved() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../../../tests/oracle/syntax-quote-reader-observations.json"
    ))
    .unwrap();
    assert_eq!(corpus["schema"], 1);
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    let sources = [
        (
            "add-one",
            include_str!("../../../../../tests/oracle/syntax-quote-add-one.sus"),
        ),
        (
            "sum-inputs",
            include_str!("../../../../../tests/oracle/syntax-quote-sum-inputs.sus"),
        ),
        (
            "twice",
            include_str!("../../../../../tests/oracle/syntax-quote-twice.sus"),
        ),
        (
            "quoted-data",
            include_str!("../../../../../tests/oracle/syntax-quote-data.sus"),
        ),
        (
            "quoted-vector",
            include_str!("../../../../../tests/oracle/syntax-quote-vector-splice.sus"),
        ),
        ("scalars", "`[nil false true 42 \"text\" :key]"),
        ("empty-list", "`()"),
        ("vector-splice", "`[1 ~x ~@xs]"),
        ("map", "`{:a ~x :b 2}"),
        ("set", "`#{~x :a}"),
        ("metadata", "`^{:marker true} [~x]"),
        ("nested", "`(outer `(inner value#) value#)"),
    ];
    assert_eq!(corpus["cases"].as_array().unwrap().len(), sources.len());
    for ((label, source), row) in sources.into_iter().zip(corpus["cases"].as_array().unwrap()) {
        assert_eq!(row[0], label);
        let form = read_forms_with_source_metadata(source, None)
            .unwrap()
            .remove(0);
        let result = ReaderState::default()
            .expand(&form, &Environment::default(), Phase::Macro)
            .unwrap();
        let mut expected = row[1].clone();
        if label == "metadata" {
            // Explicit accepted textual metadata order: marker appears before
            // indexing entries. The pinned reader happens to put it last.
            let chunks = expected
                .pointer_mut("/1/2/1/2/1/1/1/1/1")
                .unwrap()
                .as_array_mut()
                .unwrap();
            assert_eq!(chunks.len(), 11);
            assert_eq!(chunks[9][1][1], json!(["keyword", ":marker"]));
            let marker = chunks.split_off(9);
            chunks.splice(1..1, marker);
        }
        equivalent(&result, expected);
    }
}

#[test]
fn reader_state_is_transactional_and_distinct_between_templates_and_inputs() {
    let env = Environment::default();
    let mut state = ReaderState::default();
    let first = state
        .expand(&read("`(x# x#)"), &env, Phase::Runtime)
        .unwrap();
    assert_eq!(state.next_gensym, 1);
    assert!(
        state
            .expand(&read("`[y# `~@bad]"), &env, Phase::Runtime)
            .is_err()
    );
    assert_eq!(state.next_gensym, 1);
    let second = state
        .expand(&read("`(x# x#)"), &env, Phase::Runtime)
        .unwrap();
    assert_eq!(state.next_gensym, 2);
    assert_ne!(project(&first), project(&second));
    state.next_gensym = u64::MAX;
    assert!(
        state
            .expand(&read("`x#"), &env, Phase::Runtime)
            .unwrap_err()
            .message
            .contains("exhausted")
    );
}

#[test]
fn independent_runtime_and_macro_reader_states_do_not_collide() {
    let env = Environment::default();
    let form = read("`value#");
    let runtime = ReaderState::default()
        .expand(&form, &env, Phase::Runtime)
        .unwrap();
    let macros = ReaderState::default()
        .expand(&form, &env, Phase::Macro)
        .unwrap();
    assert_ne!(project(&runtime), project(&macros));
}

#[test]
fn reader_core_catalog_qualifies_names_without_claiming_runtime_bindings() {
    let env = Environment::default();
    for name in ["sequence", "-conj", "->PersistentVector", "map", "when"] {
        let result = ReaderState::default()
            .expand(&read(&format!("`{name}")), &env, Phase::Runtime)
            .unwrap();
        assert_eq!(
            project(&result)[1][1],
            json!(["symbol", format!("cljs.core/{name}")])
        );
        if name != "when" {
            assert!(
                env.resolve(Phase::Runtime, &Symbol::namespaced("cljs.core", name), 0..1)
                    .is_err(),
                "Catalog fabricated a runtime binding for {name}"
            );
        }
    }
    assert!(
        core_names::CORE_READER_NAMES
            .windows(2)
            .all(|pair| pair[0] < pair[1])
    );
}

#[test]
fn bootstrap_lowering_preserves_quoted_reader_data_and_ordinary_sequence_calls() {
    let env = Environment::default();
    let source = "(do `(1 2) '(clojure.core/sequence [1]) (clojure.core/sequence [1]))";
    let expanded = ReaderState::default()
        .expand(&read(source), &env, Phase::Runtime)
        .unwrap();
    let lowered = lower_generated(&expanded).unwrap();
    let Kind::List(items) = &lowered.kind else {
        panic!("do")
    };
    let Kind::List(generated) = &items[1].kind else {
        panic!("reader call")
    };
    assert_eq!(
        generated[0].kind,
        Kind::Symbol(Symbol::namespaced("cljs.core", "sequence")),
        "reader normalization defers coercion until live HIR resolution"
    );
    let Kind::List(quoted) = &items[2].kind else {
        panic!("quoted data")
    };
    let Kind::List(payload) = &quoted[1].kind else {
        panic!("quote payload")
    };
    assert_eq!(
        payload[0].kind,
        Kind::Symbol(Symbol::namespaced("clojure.core", "sequence"))
    );
    let Kind::List(ordinary) = &items[3].kind else {
        panic!("ordinary call")
    };
    assert_eq!(
        ordinary[0].kind,
        Kind::Symbol(Symbol::namespaced("cljs.core", "sequence"))
    );
}

#[test]
fn collection_constructor_threshold_is_sixteen_map_entries() {
    for count in [0, 8, 15, 16, 17] {
        let entries = (0..count)
            .map(|i| format!(":k{i} {i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let result = ReaderState::default()
            .expand(
                &read(&format!("`{{{entries}}}")),
                &Environment::default(),
                Phase::Runtime,
            )
            .unwrap();
        let actual = project(&result);
        assert_eq!(
            actual[1][1],
            json!([
                "symbol",
                if count >= 16 {
                    "clojure.core/hash-map"
                } else {
                    "clojure.core/array-map"
                }
            ])
        );
    }
}

#[test]
fn templates_inside_quote_expand_but_ordinary_named_calls_do_not() {
    let env = Environment::default();
    let mut state = ReaderState::default();
    let ordinary = read("(syntax-quote x)");
    assert_eq!(
        state.expand(&ordinary, &env, Phase::Runtime).unwrap(),
        ordinary
    );
    let quoted = state
        .expand(
            &read("'`(unknown .method foo.bar if)"),
            &env,
            Phase::Runtime,
        )
        .unwrap();
    let result = project(&quoted).to_string();
    for spelling in ["user/unknown", ".method", "foo.bar", "if"] {
        assert!(result.contains(spelling));
    }
    assert!(!result.contains("syntax-quote"));

    let annotated = read("^:outside `[]");
    let result = state.expand(&annotated, &env, Phase::Runtime).unwrap();
    assert_eq!(
        project(&result)[1][0],
        json!(["symbol", "clojure.core/vec"])
    );
    assert_eq!(
        super::super::hir::reader_metadata_pairs(&result).unwrap(),
        super::super::hir::reader_metadata_pairs(&annotated).unwrap()
    );
}

#[test]
fn namespace_aliases_referrals_exclusions_and_phase_are_observed() {
    let mut env = Environment::default();
    env.declare_namespace(Phase::Runtime, "runtime.lib")
        .unwrap();
    env.declare_namespace(Phase::Macro, "macro.lib").unwrap();
    env.alias(Phase::Runtime, "lib", "runtime.lib").unwrap();
    env.alias(Phase::Macro, "lib", "macro.lib").unwrap();
    for (phase, namespace) in [(Phase::Runtime, "runtime.lib"), (Phase::Macro, "macro.lib")] {
        let result = ReaderState::default()
            .expand(&read("`lib/unknown"), &env, phase)
            .unwrap();
        assert_eq!(
            project(&result)[1][1],
            json!(["symbol", format!("{namespace}/unknown")])
        );
    }
    env.declare_cell(Phase::Runtime, "user", "+").unwrap();
    let result = ReaderState::default()
        .expand(&read("`+"), &env, Phase::Runtime)
        .unwrap();
    assert_eq!(project(&result)[1][1], json!(["symbol", "user/+"]));
    let result = ReaderState::default()
        .expand(&read("`+"), &env, Phase::Macro)
        .unwrap();
    assert_eq!(project(&result)[1][1], json!(["symbol", "cljs.core/+"]));
    env.declare_cell(Phase::Runtime, "runtime.lib", "remote")
        .unwrap();
    env.refer(Phase::Runtime, "renamed", "runtime.lib", "remote")
        .unwrap();
    let result = ReaderState::default()
        .expand(&read("`renamed"), &env, Phase::Runtime)
        .unwrap();
    assert_eq!(
        project(&result)[1][1],
        json!(["symbol", "runtime.lib/remote"])
    );
    env.exclude_core(Phase::Macro, "+").unwrap();
    let result = ReaderState::default()
        .expand(&read("`+"), &env, Phase::Macro)
        .unwrap();
    assert_eq!(project(&result)[1][1], json!(["symbol", "user/+"]));
    env.declare_macro_exports(Phase::Macro, "transform.lib", &["remote".into()])
        .unwrap();
    env.macro_alias(Phase::Macro, "transform", "transform.lib")
        .unwrap();
    env.macro_refer(Phase::Macro, "renamed", "transform.lib", "remote")
        .unwrap();
    for spelling in ["renamed", "transform/remote"] {
        let result = ReaderState::default()
            .expand(&read(&format!("`{spelling}")), &env, Phase::Macro)
            .unwrap();
        assert_eq!(
            project(&result)[1][1],
            json!(["symbol", "transform.lib/remote"])
        );
    }
}

#[test]
fn bounded_expansion_rejects_large_synthetic_inputs_without_consuming_ids() {
    let mut state = ReaderState::default();
    let item = read("x");
    let form = data(&(0..1), Kind::Vector(vec![item; 65_536]));
    assert!(
        state
            .expand(&form, &Environment::default(), Phase::Runtime)
            .unwrap_err()
            .message
            .contains("bounds")
    );
    assert_eq!(state.next_gensym, 0);
}

#[test]
fn scalar_bits_utf16_and_textual_collection_effect_order_are_preserved() {
    let env = Environment::default();
    for kind in [Kind::Number(-0.0), Kind::String(vec![0xd800, 0x61, 0xdc00])] {
        let form = data(&(0..1), kind.clone());
        let result = Pass {
            state: &mut ReaderState::default(),
            environment: &env,
            phase: Phase::Runtime,
            work: 65_536,
            units: 1_048_576,
            origin: None,
        }
        .quote(&form, &mut BTreeMap::new(), 0)
        .unwrap();
        match (&kind, &result.kind) {
            (Kind::Number(a), Kind::Number(b)) => assert_eq!(a.to_bits(), b.to_bits()),
            (a, b) => assert_eq!(a, b),
        }
    }
    for source in [
        "`{:first ~(tick 1) :second ~(tick 2)}",
        "`#{~(tick 1) ~(tick 2)}",
    ] {
        let result = ReaderState::default()
            .expand(&read(source), &env, Phase::Runtime)
            .unwrap();
        let text = project(&result).to_string();
        assert!(text.find("1.0").unwrap() < text.find("2.0").unwrap());
        assert_eq!(text.matches("tick").count(), 2);
    }
    let mut state = ReaderState::default();
    let oversized = data(&(0..1), Kind::String(vec![0x61; 1_048_577]));
    assert!(
        state
            .expand(&oversized, &env, Phase::Runtime)
            .unwrap_err()
            .message
            .contains("text bounds")
    );
}

#[test]
fn private_reader_cell_is_absent_from_source_namespace_identities_until_promoted() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut env = Environment::default();
        env.enter_namespace(phase, "cljs.core").unwrap();
        let (global, fallback, fresh) = env.reader_sequence_cells(phase);
        assert!(fresh);
        // The linker still owns the shared live cell, but &env namespace :defs
        // must describe source bindings rather than hidden reader support.
        assert!(env.cells().contains(&global));
        assert!(!super::super::hir::SourceNamespace::capture(&env, phase).identities.contains(&global));
        env.enter_namespace(phase, fallback.namespace()).unwrap();
        assert!(!super::super::hir::SourceNamespace::capture(&env, phase).identities.contains(&fallback));
        env.enter_namespace(phase, "cljs.core").unwrap();
        env.declare_cell(phase, "cljs.core", "sequence").unwrap();
        assert!(super::super::hir::SourceNamespace::capture(&env, phase).identities.contains(&global));
    }
}


#[test]
fn publication_probe_cannot_promote_real_reader_or_internal_reservations() {
    use super::super::hir::{SourceNamespace, prepare};
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut env = Environment::default();
        env.enter_namespace(phase, "cljs.core").unwrap();
        let (reader, fallback, _) = env.reader_sequence_cells(phase);
        for spelling in ["suss.core/sequence", "cljs.core/sequence"] {
            let form = read(&format!("(suss.compiler/cell-defined? {spelling})"));
            let error = prepare(&[form], 0..0, &env, phase).unwrap_err();
            assert!(error.message.contains("Compiler-owned cells"));
            assert!(env.is_hidden_cell(phase, reader.namespace(), reader.name()));
            assert!(
                !SourceNamespace::capture(&env, phase)
                    .identities
                    .contains(&reader)
            );
        }
        // JVM Clojure namespace spelling is not a portable canonical alias.
        let form = read("(suss.compiler/cell-defined? clojure.core/sequence)");
        let error = prepare(&[form], 0..0, &env, phase).unwrap_err();
        assert!(error.message.contains("Unresolved"));
        assert!(error.message.contains("clojure.core/sequence"));
        assert!(env.is_hidden_cell(phase, reader.namespace(), reader.name()));
        assert!(
            !SourceNamespace::capture(&env, phase)
                .identities
                .contains(&reader)
        );
        // A real source declaration intentionally promotes the ReaderCell.
        let (_, promoted) = prepare(&[read("(def sequence)")], 0..0, &env, phase).unwrap();
        assert!(!promoted.is_hidden_cell(phase, reader.namespace(), reader.name()));
        assert!(
            SourceNamespace::capture(&promoted, phase)
                .identities
                .contains(&reader)
        );
        prepare(
            &[read("(suss.compiler/cell-defined? suss.core/sequence)")],
            0..0,
            &promoted,
            phase,
        )
        .unwrap();
        assert!(env.is_hidden_cell(phase, reader.namespace(), reader.name()));
        let protocol = env
            .declare_cell(phase, "user", "PublicationProtocol")
            .unwrap();
        let internal = env.protocol_key(&protocol, "-publication", 1);
        for reserved in [fallback, internal] {
            env.enter_namespace(phase, reserved.namespace()).unwrap();
            let probe = read(&format!(
                "(suss.compiler/cell-defined? {}/{})",
                reserved.namespace(),
                reserved.name()
            ));
            let error = prepare(&[probe], 0..0, &env, phase).unwrap_err();
            assert!(error.message.contains("Compiler-owned cells"));
            assert!(env.is_hidden_cell(phase, reserved.namespace(), reserved.name()));
            assert!(
                !SourceNamespace::capture(&env, phase)
                    .identities
                    .contains(&reserved)
            );
            let definition = read(&format!("(def {})", reserved.name()));
            let error = prepare(&[definition], 0..0, &env, phase).unwrap_err();
            assert!(
                error
                    .message
                    .contains("Cannot redefine a compiler-owned binding")
            );
            assert!(env.is_hidden_cell(phase, reserved.namespace(), reserved.name()));
        }
    }
}
