use sha2::{Digest, Sha256};
fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).unwrap();
        wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all()).validate_all(&bytes).unwrap();
        let mut found = 0;
        for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
            if let wasmparser::Payload::CodeSectionEntry(body) = payload.unwrap() {
                let marked = body.get_operators_reader().unwrap().into_iter().any(|op|
                    matches!(op.unwrap(), wasmparser::Operator::I64Const { value: 1234567 }));
                if marked {
                    println!("{{\"file\":{:?},\"body_bytes\":{},\"sha256\":\"{:x}\"}}", path, body.as_bytes().len(), Sha256::digest(body.as_bytes()));
                    found += 1;
                }
            }
        }
        assert!(found > 0, "missing sum-loop marker");
    }
}
