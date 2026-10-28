//! Compare complete private factory forms and metadata with the predecessor.
//! Upstream semantics remain covered by public record and pinned-oracle cases.

use suss_cli::{portable_macro_data::FormBridge, portable_session::Session};
use suss_reader::forms::{Form, Kind};

const NEW_SOURCE_FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../suss-compile/src/portable_record_factories.rs"
);
const OLD_SOURCE_FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/oracle/fixtures/record-factories-before-direct-construction.rs.txt"
);
const NEW_SOURCE_SHA256: &str = "3b9ea000fe9c73f7945d5fb95d25c2c2ab073f44234713bfcbf6813d2670159d";
const OLD_SOURCE_SHA256: &str = "b8917bda74562ad8738594c7108bf848e5903de4ceba277c8188bffb5565f66b";

const SOURCE_PREFIX: &str = "pub(crate) const SOURCE: &str = r#\"\n";
const SOURCE_SUFFIX: &str = "\n\"#;";

// Each source request is byte-pinned below. The map-factory calls are performed
// in the same order in fresh, independently initialized Macro sessions, so the
// factory's gensym counter starts at the same value in both implementations.
const CASES: &[(&str, &str, &str)] = &[
    (
        "positional-zero-fields",
        "(build-positional-factory (with-meta 'ZeroRecord {:doc \"positional zero\" :factory-marker :positional-zero}) (with-meta 'demo.records/ZeroRecord {:rname-marker :zero}) (with-meta '[] {:fields-marker :empty}))",
        "dbab2906325f3eb3fa2945ba2654e7c50cf4051862a5c40025e354ffafca0259",
    ),
    (
        "positional-internal-constructor-branch",
        "(build-positional-factory (with-meta 'InternalRecord {:doc \"internal constructor\" :internal-ctor true}) (with-meta 'demo.records/InternalRecord {:rname-marker :internal}) (with-meta '[left right] {:fields-marker :internal-fields}))",
        "fea97daf97d13277060641ecc3aedf39c180fb3431e179664c5264c8dd5ed955",
    ),
    (
        "positional-hints-mutable-order-and-vector-meta",
        "(build-positional-factory (with-meta 'HintedRecord {:doc \"hints and order\" :factory-marker :hinted}) (with-meta 'demo.records/HintedRecord {:rname-marker :hinted}) (with-meta '[^{:tag :number :mutable true} first ^{:tag :string} second third] {:fields-marker :hinted-vector}))",
        "9cfba39c901ffff92bcebbde399cdeef6970ddf0605104e4b438af961e3f3b68",
    ),
    (
        "positional-33-field-boundary",
        "(build-positional-factory (with-meta 'BoundaryRecord {:doc \"33 fields\"}) 'demo.records/BoundaryRecord (with-meta '[f00 f01 f02 f03 f04 f05 f06 f07 f08 f09 f10 f11 f12 f13 f14 f15 f16 f17 f18 f19 f20 f21 f22 f23 f24 f25 f26 f27 f28 f29 f30 f31 f32] {:fields-marker :boundary}))",
        "36453383dc3fbd910edb57fa856cb26e7a659611f8b591accd1c1f8839c40b99",
    ),
    (
        "map-zero-fields-and-nominal-metadata",
        "(build-map-factory (with-meta 'ZeroMapRecord {:doc \"map zero\" :factory-marker :map-zero}) (with-meta 'demo.records/ZeroMapRecord {:rname-marker :map-zero}) (with-meta '[] {:fields-marker :empty-map-fields}))",
        "bcf74238a4fee9c698f87feccea1ae71d0c44d4cd8d30e4748dd0f163240f6ce",
    ),
    (
        "map-hints-mutable-order-and-vector-meta",
        "(build-map-factory (with-meta 'HintedMapRecord {:doc \"map hints\" :factory-marker :map-hinted}) (with-meta 'demo.records/HintedMapRecord {:rname-marker :map-hinted}) (with-meta '[^{:tag :number :mutable true} first ^{:tag :string} second third] {:fields-marker :map-hinted-vector}))",
        "f9390794201eb32cce3a8908949a3cc9e9aa260edc48f09446ab144f4a8d6b91",
    ),
    (
        "map-33-field-boundary",
        "(build-map-factory (with-meta 'BoundaryMapRecord {:doc \"map 33 fields\"}) 'demo.records/BoundaryMapRecord (with-meta '[f00 f01 f02 f03 f04 f05 f06 f07 f08 f09 f10 f11 f12 f13 f14 f15 f16 f17 f18 f19 f20 f21 f22 f23 f24 f25 f26 f27 f28 f29 f30 f31 f32] {:fields-marker :boundary-map}))",
        "45e78bb299e28696bede2e9f3f85bd1035873aecb7c44c7a96bd770ccbfb1603",
    ),
];

fn extract_source(file: &str, expected_sha256: &str) -> String {
    let bytes = std::fs::read_to_string(file).unwrap();
    assert_eq!(
        suss_compile::portable::bootstrap::sha256(bytes.as_bytes()),
        expected_sha256,
        "factory source bytes changed: {file}"
    );
    let start = bytes.find(SOURCE_PREFIX).expect("SOURCE raw-string opener") + SOURCE_PREFIX.len();
    let end = bytes[start..]
        .find(SOURCE_SUFFIX)
        .expect("SOURCE raw-string closer")
        + start;
    assert!(!bytes[end + SOURCE_SUFFIX.len()..].contains(SOURCE_SUFFIX));
    bytes[start..end].to_owned()
}

fn erase_spans(form: &mut Form) {
    form.span = 0..0;
    for metadata in &mut form.metadata {
        erase_spans(metadata);
    }
    match &mut form.kind {
        Kind::List(items)
        | Kind::Vector(items)
        | Kind::Set(items)
        | Kind::Map(items)
        | Kind::Conditional(items)
        | Kind::SplicingConditional(items) => {
            for item in items {
                erase_spans(item);
            }
        }
        Kind::Discard(target) => erase_spans(target),
        Kind::Prefix { operator, target } => {
            erase_spans(operator);
            erase_spans(target);
        }
        Kind::Nil
        | Kind::Bool(_)
        | Kind::Number(_)
        | Kind::String(_)
        | Kind::Symbol(_)
        | Kind::Keyword(_) => {}
    }
}

fn list(form: &Form) -> &[Form] {
    let Kind::List(items) = &form.kind else {
        panic!("generated defn must be a list: {form:?}")
    };
    items
}

fn assert_positional_branch(form: &Form, internal: bool, field_count: usize) {
    let defn = list(form);
    assert!(defn.len() >= 5, "complete generated defn: {form:?}");
    let Kind::Vector(parameters) = &defn[3].kind else {
        panic!("factory parameters must preserve a vector: {:?}", defn[3])
    };
    assert_eq!(parameters.len(), field_count);
    let body = list(&defn[4]);
    assert!(matches!(&body[0].kind, Kind::Symbol(name) if name.name == "new"));
    let expected_arguments = 2 + field_count + if internal { 3 } else { 0 };
    assert_eq!(body.len(), expected_arguments, "internal nil-field branch");
}

#[test]
fn old_and_candidate_factory_forms_match_with_recursive_metadata() {
    let old_source = extract_source(OLD_SOURCE_FILE, OLD_SOURCE_SHA256);
    let new_source = extract_source(NEW_SOURCE_FILE, NEW_SOURCE_SHA256);

    let mut old_session = Session::new_macro().expect("old independent Macro session");
    let mut new_session = Session::new_macro().expect("candidate independent Macro session");
    let old_bridge = FormBridge::new(&mut old_session).expect("old bridge");
    let new_bridge = FormBridge::new(&mut new_session).expect("new bridge");
    old_session
        .eval_privileged(&old_source)
        .expect("load old factory source");
    new_session
        .eval_privileged(&new_source)
        .expect("load candidate factory source");
    old_session
        .enter_namespace("suss.internal.record-factories")
        .unwrap();
    new_session
        .enter_namespace("suss.internal.record-factories")
        .unwrap();

    for &(case_id, request, expected_sha256) in CASES {
        assert_eq!(
            suss_compile::portable::bootstrap::sha256(request.as_bytes()),
            expected_sha256,
            "pinned request bytes changed: {case_id}"
        );
        let old_value = old_session
            .eval(request)
            .unwrap_or_else(|error| panic!("old/{case_id}: {error:?}"));
        let new_value = new_session
            .eval(request)
            .unwrap_or_else(|error| panic!("candidate/{case_id}: {error:?}"));
        old_session
            .collect()
            .expect("old generated form remains rooted");
        new_session
            .collect()
            .expect("new generated form remains rooted");
        let mut old_form = old_bridge
            .read(&mut old_session, &old_value, 0..request.len())
            .unwrap_or_else(|error| panic!("old FormBridge/{case_id}: {error:?}"));
        let mut new_form = new_bridge
            .read(&mut new_session, &new_value, 0..request.len())
            .unwrap_or_else(|error| panic!("candidate FormBridge/{case_id}: {error:?}"));
        erase_spans(&mut old_form);
        erase_spans(&mut new_form);
        assert_eq!(
            new_form, old_form,
            "complete generated form/metadata: {case_id}"
        );

        if case_id == "positional-internal-constructor-branch" {
            assert_positional_branch(&old_form, true, 2);
        } else if case_id.starts_with("positional-") {
            let count = if case_id == "positional-zero-fields" {
                0
            } else if case_id == "positional-33-field-boundary" {
                33
            } else {
                3
            };
            assert_positional_branch(&old_form, false, count);
        }
    }
}
