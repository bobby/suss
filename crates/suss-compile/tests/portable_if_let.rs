use suss_compile::portable::{self, hir::Expression};

#[test]
fn if_let_keeps_reader_metadata_and_distinct_scoped_binding_identities() {
    let source = "(let [x 17] (if-let [^:bound x x] ^:then x ^:else x))";
    let hir = portable::analyze(source).unwrap();
    let Expression::Do(top) = hir.kind else {
        panic!()
    };
    let Expression::Let {
        bindings: outer,
        body,
    } = &top[0].kind
    else {
        panic!()
    };
    let Expression::Do(body) = &body.kind else {
        panic!()
    };
    let Expression::Let {
        bindings: temporary,
        body,
    } = &body[0].kind
    else {
        panic!()
    };
    assert!(matches!(temporary[0].value.kind, Expression::Local(id) if id == outer[0].id));
    let Expression::If {
        condition,
        consequent,
        alternative,
    } = &body.kind
    else {
        panic!()
    };
    assert!(matches!(condition.kind, Expression::Local(id) if id == temporary[0].id));
    let Expression::Let {
        bindings: inner,
        body: then,
    } = &consequent.kind
    else {
        panic!()
    };
    assert_ne!(outer[0].id, temporary[0].id);
    assert_ne!(outer[0].id, inner[0].id);
    assert_ne!(temporary[0].id, inner[0].id);
    assert_eq!(inner[0].name, "x");
    assert_eq!(inner[0].metadata.len(), 1);
    assert_eq!(&source[inner[0].span.clone()], "^:bound x");
    assert!(matches!(inner[0].value.kind, Expression::Local(id) if id == temporary[0].id));
    assert!(matches!(then.kind, Expression::Local(id) if id == inner[0].id));
    assert_eq!(then.metadata.len(), 1);
    assert_eq!(&source[then.span.clone()], "^:then x");
    assert!(matches!(alternative.kind, Expression::Local(id) if id == outer[0].id));
    assert_eq!(alternative.metadata.len(), 1);
    assert_eq!(&source[alternative.span.clone()], "^:else x");
}
