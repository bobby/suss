#[test]
fn native_numeric_class_references_round_trip_as_class_constants() {
    use suss_reader::edn::{self, Options, Value};
    let source = "Long\nclojure.lang.BigInt\njava.math.BigInteger\nclojure.lang.Ratio\nNumber\nClass";
    let path = root().join("target/exact-classes.sus");
    let fragments = analyze_source(source, &path, &[], None, None).unwrap();
    let document = hir_document(&fragments, &path).unwrap();
    for node in asts(&document) {
        assert_eq!(get(node, "op"), &Value::keyword("const"));
        assert_eq!(get(node, "type"), &Value::keyword("class"));
        let Value::Symbol(symbol) = get(node, "val") else { panic!("class symbol") };
        assert!(symbol.namespace.is_none());
        assert!(symbol.name.contains('.'));
    }
    let wire = print(&document).unwrap();
    assert_eq!(edn::read_one(&wire, &Options::default()).unwrap(), document);
    assert_round_trip(source, &path, &[]);
}
