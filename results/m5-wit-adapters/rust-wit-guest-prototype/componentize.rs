fn main() {
 let mut args=std::env::args().skip(1);
 let core=std::fs::read(args.next().unwrap()).unwrap();
 let component=wit_component::ComponentEncoder::default().module(&core).unwrap().validate(true).encode().unwrap();
 wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all()).validate_all(&component).unwrap();
 std::fs::write(args.next().unwrap(),&component).unwrap();
 println!("validated independent Rust component: {} bytes",component.len());
}
