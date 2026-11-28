fn main() {
    let source = "(defn ^:export bump [x] (assoc x 0 (inc (nth x 0))))";
    let wit = "package test:composites; world api { type row = tuple<s64, string, option<bool>>; export bump: func(x: row) -> row; }";
    let _bytes = suss_compile::Compiler::new().compile(source, wit).unwrap();
}
