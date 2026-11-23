use suss_cli::portable_aot::prepare_source;
use suss_compile::portable;
fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    std::fs::create_dir_all(&out).unwrap();
    let mut resolve = portable::aot::Resolve::new();
    let package = resolve.push_str("size.wit", "package test:size; world w {}").unwrap();
    let world = *resolve.packages[package].worlds.values().next().unwrap();
    for count in [4, 24] {
        let mut source = String::from("(ns size)\n");
        for n in 0..count { source.push_str(&format!("(def f{n} (fn [a b] a))\n")); }
        let fragments = prepare_source(&source, None, &[]).unwrap();
        let bytes = portable::aot::component(&fragments, &resolve, world, &[]).unwrap();
        std::fs::write(out.join(format!("size-{count}.wasm")), &bytes).unwrap();
        let mut sections = std::collections::BTreeMap::<String, usize>::new();
        let mut stack = Vec::new();
        let mut artifact_index = 0;
        for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
            let payload = payload.unwrap();
            if let wasmparser::Payload::Version { encoding, .. } = &payload { stack.push(*encoding); }
            if let wasmparser::Payload::CustomSection(custom) = &payload {
                if custom.name() == "suss.source-artifact" {
                    std::fs::write(out.join(format!("manifest-{count}-{artifact_index}.edn")), custom.data()).unwrap();
                    artifact_index += 1;
                }
            }
            if let Some((id, range)) = payload.as_section() {
                let key = if let wasmparser::Payload::CustomSection(custom) = &payload {
                    format!("{:?}:custom:{}", stack.last(), custom.name())
                } else { format!("{:?}:section-{id}", stack.last()) };
                *sections.entry(key).or_default() += (range.end-range.start) as usize;
            }
            if matches!(payload, wasmparser::Payload::End(_)) { stack.pop(); }
        }
        println!("count {count}: bytes {}, sections {sections:?}", bytes.len());
    }
}
