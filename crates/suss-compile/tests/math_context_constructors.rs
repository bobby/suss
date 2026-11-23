//! Execute the UTF-16 digit table against the complete pinned JVM char domain.
mod support;
use wasmtime::{Instance, Module, Store};

#[test]
fn context_text_digit_mapping_matches_every_primary_utf16_code_unit() {
    let engine = support::engine();
    let mut store = Store::new(&engine, ());
    let module = Module::new(&engine, suss_compile::runtime_abi::module()).unwrap();
    let runtime = Instance::new(&mut store, &module, &[]).unwrap();
    let digit = runtime
        .get_typed_func::<i32, i32>(&mut store, "decimal-context-digit")
        .unwrap();
    let primary: Vec<(i32, i32)> = serde_json::from_str(include_str!(
        "../../../results/m12-context-constructors/bmp-digits.json"
    ))
    .unwrap();
    let primary = primary
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(primary.len(), 370);
    for unit in 0..65536 {
        assert_eq!(
            digit.call(&mut store, unit).unwrap(),
            *primary.get(&unit).unwrap_or(&-1),
            "UTF-16 code unit {unit:#06x}"
        );
    }
    for invalid in [-1, i32::MIN, 65536, i32::MAX] {
        assert_eq!(digit.call(&mut store, invalid).unwrap(), -1);
    }
}
