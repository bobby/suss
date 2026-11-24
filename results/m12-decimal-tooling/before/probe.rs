fn main() {
    let input = "1.0M";
    let value = suss_reader::edn::read_one(input, &suss_reader::edn::Options::default());
    println!("input={input} result={value:?}");
    assert!(value.is_ok(), "BigDecimal literal admission remains missing from the tooling EDN value model");
}
