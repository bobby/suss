#[test]
fn exact_numeric_ast_values_survive_native_edn_and_source_round_trips() {
    use suss_reader::edn::{self, Options};
    let source = "1N\n18446744073709551616N\n1/3";
    let path = root().join("target/exact-data.sus");
    let fragments = analyze_source(source, &path, &[], None, None).unwrap();
    let document = hir_document(&fragments, &path).unwrap();
    let nodes = asts(&document);
    assert_eq!(nodes.len(), 3);
    for (node, source) in nodes.iter().zip(source.lines()) {
        assert_eq!(get(node, "val"), &edn::read_one(source, &Options::default()).unwrap());
    }
    let wire = print(&document).unwrap();
    assert!(!wire.contains("#suss/exact-number"));
    let restored = edn::read_one(&wire, &Options::default()).unwrap();
    assert_eq!(restored, document);
    let emitted = emitted_source(&restored).unwrap();
    assert_round_trip(&emitted, &path, &[]);
}
