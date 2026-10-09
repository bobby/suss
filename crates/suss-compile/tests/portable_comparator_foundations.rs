//! Compiler guards only; native behavior belongs to core_comparator_foundations.
use suss_compile::portable::{
    self,
    resolve::{Environment, Phase},
};

#[test]
fn private_comparator_adapters_require_one_operand_and_are_not_first_class() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let environment = Environment::default();
        for name in ["number?", "string?", "value-constructor"] {
            for source in [
                format!("(suss.bootstrap/{name})"),
                format!("(suss.bootstrap/{name} nil nil)"),
                format!("suss.bootstrap/{name}"),
                format!("(let [f suss.bootstrap/{name}] (f nil))"),
            ] {
                assert!(
                    portable::compile_in(&source, &environment, phase).is_err(),
                    "{phase:?}: {source}"
                );
            }
        }
    }
}

#[test]
fn constructor_cache_preserves_shared_recursive_abi_type_count_and_descriptor_layout() {
    use wasmparser::{CompositeInnerType, Parser, Payload};
    assert_eq!(suss_compile::runtime_abi::VERSION, 2);
    let runtime = suss_compile::runtime_abi::module();
    for payload in Parser::new(0).parse_all(&runtime) {
        if let Payload::TypeSection(types) = payload.unwrap() {
            let group = types.into_iter().next().unwrap().unwrap();
            assert!(group.is_explicit_rec_group());
            let types = group.types().collect::<Vec<_>>();
            assert_eq!(
                types.len(),
                10,
                "cache must not enlarge the shared recursive ABI group"
            );
            let CompositeInnerType::Struct(descriptor) = &types[6].composite_type.inner else {
                panic!("descriptor shape")
            };
            assert_eq!(descriptor.fields.len(), 5);
            assert!(!descriptor.fields[0].mutable);
            assert!(!descriptor.fields[1].mutable);
            assert!(descriptor.fields[2].mutable);
            assert!(descriptor.fields[3].mutable);
            assert!(descriptor.fields[4].mutable);
            return;
        }
    }
    panic!("runtime type section missing");
}
