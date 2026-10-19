//! Compile and execute large/deep macro inputs and generated results in full.
#[path = "support/compilation_fuel.rs"]
mod compilation_fuel;
use suss_cli::{
    portable_macro_data::FormBridge, portable_macros::CompiledMacros, portable_session::Session,
};
use suss_reader::forms::{Form, Kind};

fn metadata_number(form: &Form, name: &str) -> f64 {
    for metadata in &form.metadata {
        if let Kind::Map(entries) = &metadata.kind {
            for pair in entries.chunks_exact(2) {
                if matches!(&pair[0].kind, Kind::Keyword(key)
                    if key.namespace.is_none() && key.name == name)
                {
                    let Kind::Number(value) = &pair[1].kind else {
                        panic!("metadata {name} must be numeric");
                    };
                    return *value;
                }
            }
        }
    }
    panic!("required metadata {name} missing");
}

#[test]
fn compiled_large_and_deep_inputs_and_generated_results_retain_all_data_after_gc() {
    const WIDTH: usize = 4097;
    const DEPTH: usize = 80;
    // This new positive fixture constructs and reads a large Runtime value.
    // Its first10M evaluation attempt exhausted after successful compilation;
    // use an explicit execution allowance, separate from compiler fuel.
    const RESULT_EVALUATION_FUEL: u64 = 100_000_000;
    let wide = (0..WIDTH)
        .map(|index| {
            if index == WIDTH / 2 {
                "^{:leaf 91} marker".to_owned()
            } else {
                index.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let wide_input = format!("(finite-pass ^{{:payload 17}} [{wide}])");
    let deep_input = format!(
        "(finite-pass ^{{:payload 17}} {}^{{:leaf 91}} marker{})",
        "[".repeat(DEPTH),
        "]".repeat(DEPTH)
    );
    for (mut caller, phase) in [
        (Session::new_repl().unwrap(), "Runtime"),
        (Session::new_macro().unwrap(), "Macro"),
    ] {
        assert_eq!(caller.options().fuel_per_operation, 10_000_000);
        caller.set_operation_fuel(RESULT_EVALUATION_FUEL);
        let caller_fuel = caller.options().fuel_per_operation;
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
        macros
            .define("(defmacro finite-pass [x] (list 'quote x))")
            .unwrap();
        // Small source input, genuinely large result built inside the transformer.
        macros.define("(defmacro finite-generated-wide [] (loop [i 0 v []] (if (< i 4097) (recur (inc i) (conj v (if (= i 2048) (with-meta 'marker {:leaf 91}) i))) (list 'quote (with-meta v {:payload 17})))))").unwrap();
        macros.define("(defmacro finite-generated-deep [] (loop [i 0 v (with-meta 'marker {:leaf 91})] (if (< i 80) (recur (inc i) [v]) (list 'quote (with-meta v {:payload 17})))))").unwrap();
        let bridge = FormBridge::new(&mut caller).unwrap();
        for (label, source, is_wide) in [
            ("wide argument", wide_input.as_str(), true),
            ("deep argument", deep_input.as_str(), false),
            ("generated wide", "(finite-generated-wide)", true),
            ("generated deep", "(finite-generated-deep)", false),
        ] {
            // Actual compiled macro output -> caller compilation -> execution.
            let value = caller
                .eval_with_macros(source, &mut macros)
                .unwrap_or_else(|error| panic!("{phase} {label}: {error:?}"));
            caller.collect().unwrap();
            let decoded = bridge.read(&mut caller, &value, 0..source.len()).unwrap();
            assert_eq!(metadata_number(&decoded, "payload"), 17.0);
            if is_wide {
                let Kind::Vector(items) = &decoded.kind else {
                    panic!("complete wide result")
                };
                assert_eq!(items.len(), WIDTH);
                for (index, item) in items.iter().enumerate() {
                    if index == WIDTH / 2 {
                        assert!(matches!(&item.kind, Kind::Symbol(name)
                            if name.namespace.is_none() && name.name == "marker"));
                        assert_eq!(metadata_number(item, "leaf"), 91.0);
                    } else {
                        assert_eq!(item.kind, Kind::Number(index as f64), "index {index}");
                    }
                }
            } else {
                let mut cursor = &decoded;
                for level in 0..DEPTH {
                    let Kind::Vector(items) = &cursor.kind else {
                        panic!("level {level}")
                    };
                    assert_eq!(items.len(), 1, "level {level}");
                    cursor = &items[0];
                }
                assert!(matches!(&cursor.kind, Kind::Symbol(name)
                    if name.namespace.is_none() && name.name == "marker"));
                assert_eq!(metadata_number(cursor, "leaf"), 91.0);
            }
            assert_eq!(caller.options().fuel_per_operation, caller_fuel);
            assert_eq!(
                suss_compile::portable::ExpansionHost::expansion_work_fuel(&macros),
                compilation_fuel::COMPILATION_FUEL
            );
        }
    }
}
