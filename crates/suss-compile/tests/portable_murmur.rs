use suss_compile::portable::{self, hir::Expression};

#[test]
fn threading_keeps_list_metadata_without_promoting_symbol_metadata() {
    for (source, count) in [("(-> 3 ^:tag (bit-or 4))", 1), ("(-> 3 ^:tag bit-not)", 0)] {
        let hir = portable::analyze(source).unwrap();
        let Expression::Do(items) = &hir.kind else {
            panic!("fragment body");
        };
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].metadata.len(), count, "{source}");
        portable::compile_ir(&portable::ir::lower(&hir).unwrap()).unwrap();
    }
}
