//! Ordered AOT loading under one dynamic namespace binding (#312).
//! Original Suss code. Incremental sessions keep their independent frames.
use super::{Diagnostic, PreparedFragment, resolve::Phase};
use crate::runtime_abi;
use std::borrow::Cow;
use suss_reader::Symbol;
use wasm_encoder::*;

pub(super) const MODULE: &str = "suss.load";

pub(super) struct Plan {
    pub names: Vec<String>,
    pub module: Option<Vec<u8>>,
    source_count: usize,
}

impl Plan {
    /// Schema factories run immediately after core, before user initializers.
    /// They retain their own frames and are excluded from the shared load.
    pub fn new(fragments: &[PreparedFragment], schema_count: usize) -> Result<Self, Diagnostic> {
        let source_count = fragments.len() - schema_count;
        let ordered = fragments
            .iter()
            .any(|fragment| fragment.environment.ordered_namespace_initialization);
        if !ordered {
            return Ok(Self {
                names: (0..fragments.len())
                    .map(|index| format!("suss.fragment.{index}"))
                    .collect(),
                module: None,
                source_count,
            });
        }
        let namespace = fragments
            .last()
            .expect("AOT requires fragments")
            .environment
            .resolve(
                Phase::Runtime,
                &Symbol::namespaced("suss.core", "*ns*"),
                0..0,
            )?
            .global()
            .clone();
        let mut names = vec!["suss.fragment.0".to_owned(), MODULE.to_owned()];
        names.extend((source_count..fragments.len()).map(|index| format!("suss.fragment.{index}")));
        let value = ValType::Ref(RefType::EQREF);
        let mut types = runtime_abi::prelude();
        let mut imports = ImportSection::new();
        for (index, (name, params)) in [
            ("binding-get", vec![value]),
            (
                "closure-new",
                vec![
                    value,
                    runtime_abi::invoke_type(),
                    ValType::I32,
                    ValType::I32,
                ],
            ),
            ("dynamic-invoke", vec![value, value]),
        ]
        .into_iter()
        .enumerate()
        {
            types.ty().function(params, [value]);
            imports.import(
                "suss.runtime",
                name,
                EntityType::Function(runtime_abi::TYPE_COUNT + index as u32),
            );
        }
        types.ty().function([], [value]);
        for index in 1..source_count {
            imports.import(
                &format!("suss.fragment.{index}"),
                "eval",
                EntityType::Function(runtime_abi::TYPE_COUNT + 3),
            );
        }
        imports.import(
            namespace.import_module(),
            &namespace.import_name(),
            EntityType::Global(GlobalType {
                val_type: runtime_abi::binding_cell_type(),
                mutable: false,
                shared: false,
            }),
        );
        let callback = 3 + source_count as u32 - 1;
        let mut functions = FunctionSection::new();
        functions.function(runtime_abi::INVOKE);
        functions.function(runtime_abi::TYPE_COUNT + 3);
        let mut code = CodeSection::new();
        let mut body = Function::new([]);
        for index in 1..source_count {
            body.instruction(&Instruction::Call(3 + index as u32 - 1))
                .instruction(&Instruction::Drop);
        }
        body.instruction(&Instruction::RefNull(HeapType::Abstract {
            shared: false,
            ty: AbstractHeapType::Eq,
        }))
        .instruction(&Instruction::End);
        code.function(&body);
        let mut body = Function::new([(1, value)]);
        for instruction in [
            Instruction::GlobalGet(0),
            Instruction::Call(0),
            Instruction::LocalSet(0),
            Instruction::GlobalGet(0),
            Instruction::LocalGet(0),
            Instruction::LocalGet(0),
            Instruction::ArrayNewFixed {
                array_type_index: runtime_abi::ARGS,
                array_size: 3,
            },
            Instruction::RefNull(HeapType::Abstract {
                shared: false,
                ty: AbstractHeapType::Eq,
            }),
            Instruction::RefFunc(callback),
            Instruction::I32Const(0),
            Instruction::I32Const(0),
            Instruction::Call(1),
            Instruction::Call(2),
            Instruction::End,
        ] {
            body.instruction(&instruction);
        }
        code.function(&body);
        let mut exports = ExportSection::new();
        exports.export("eval", ExportKind::Func, callback + 1);
        let mut elements = ElementSection::new();
        elements.declared(Elements::Functions(Cow::Borrowed(&[callback])));
        let mut module = Module::new();
        module
            .section(&runtime_abi::Manifest::default().section())
            .section(&types)
            .section(&imports)
            .section(&functions)
            .section(&exports)
            .section(&elements)
            .section(&code);
        Ok(Self {
            names,
            module: Some(module.finish()),
            source_count,
        })
    }

    pub fn arguments(
        &self,
        runtime: u32,
        bindings: u32,
        first_fragment: u32,
    ) -> Vec<(String, ModuleArg)> {
        let mut args = vec![
            ("suss.runtime".to_owned(), ModuleArg::Instance(runtime)),
            (
                "suss.bindings.runtime".to_owned(),
                ModuleArg::Instance(bindings),
            ),
        ];
        args.extend((1..self.source_count).map(|index| {
            (
                format!("suss.fragment.{index}"),
                ModuleArg::Instance(first_fragment + index as u32),
            )
        }));
        args
    }
}

#[cfg(all(test, suss_host, not(suss_generator)))]
mod tests {
    use super::*;
    use crate::portable_session::{RuntimeNumber, runtime_number};
    use wasmtime::{AsContextMut, Config, Engine, Instance, Linker, Module, Rooted, Store, Val};

    #[test]
    fn failed_ordered_load_restores_namespace_and_retains_only_prior_effects() {
        static ENGINE: std::sync::OnceLock<Engine> = std::sync::OnceLock::new();
        let engine = ENGINE.get_or_init(|| {
            let mut config = Config::new();
            config
                .wasm_gc(true)
                .wasm_function_references(true)
                .wasm_tail_call(true)
                .wasm_exceptions(true)
                .consume_fuel(true)
                .cranelift_opt_level(wasmtime::OptLevel::None);
            Engine::new(&config).unwrap()
        });
        let fragments = crate::portable_aot::prepare_source(
            "(ns one) (def visits 0) (set! visits 1) (ns two)\n\
             (set! one/visits 2) (throw 17) (set! one/visits 3)",
            None,
            &[],
        )
        .unwrap();
        let plan = Plan::new(&fragments, 0).unwrap();
        let cells = fragments
            .iter()
            .flat_map(|fragment| fragment.cells.iter().cloned())
            .collect::<Vec<_>>();
        let bindings =
            super::super::core_bindings::compile_with_cells(Phase::Runtime, &cells).unwrap();
        let mut store = Store::new(engine, ());
        store.set_fuel(200_000_000).unwrap();
        let runtime = Instance::new(
            &mut store,
            &Module::new(engine, runtime_abi::module()).unwrap(),
            &[],
        )
        .unwrap();
        let mut linker = Linker::new(engine);
        linker
            .instance(&mut store, "suss.runtime", runtime)
            .unwrap();
        let bindings = linker
            .instantiate(&mut store, &Module::new(engine, bindings.wasm).unwrap())
            .unwrap();
        linker
            .instance(&mut store, "suss.bindings.runtime", bindings)
            .unwrap();
        let mut instances = Vec::new();
        for (index, fragment) in fragments.iter().enumerate() {
            let instance = linker
                .instantiate(&mut store, &Module::new(engine, &fragment.wasm).unwrap())
                .unwrap();
            linker
                .instance(&mut store, &format!("suss.fragment.{index}"), instance)
                .unwrap();
            instances.push(instance);
        }
        let mut result = [Val::null_any_ref()];
        instances[0]
            .get_func(&mut store, "eval")
            .unwrap()
            .call(&mut store, &[], &mut result)
            .unwrap();
        let get = runtime.get_func(&mut store, "binding-get").unwrap();
        let read = |store: &mut Store<()>, symbol: Symbol| {
            let global = fragments
                .last()
                .unwrap()
                .environment
                .resolve(Phase::Runtime, &symbol, 0..0)
                .unwrap()
                .global()
                .clone();
            let cell = bindings
                .get_global(&mut *store, &global.import_name())
                .unwrap()
                .get(&mut *store);
            let mut result = [Val::null_any_ref()];
            get.call(store, &[cell], &mut result).unwrap();
            result[0]
        };
        let before = read(&mut store, Symbol::namespaced("suss.core", "*ns*"));
        let load = linker
            .instantiate(
                &mut store,
                &Module::new(engine, plan.module.unwrap()).unwrap(),
            )
            .unwrap();
        assert!(
            load.get_func(&mut store, "eval")
                .unwrap()
                .call(&mut store, &[], &mut result)
                .is_err()
        );
        let exception = store
            .as_context_mut()
            .take_pending_exception()
            .expect("language throw");
        let payload = exception.fields(&mut store).unwrap().collect::<Vec<_>>();
        assert_eq!(
            runtime_number(&mut store, &payload[0]).unwrap(),
            Some(RuntimeNumber::Long(17))
        );
        store.gc(None).unwrap();
        let after = read(&mut store, Symbol::namespaced("suss.core", "*ns*"));
        assert!(
            Rooted::ref_eq(
                &store,
                before.unwrap_anyref().unwrap(),
                after.unwrap_anyref().unwrap()
            )
            .unwrap()
        );
        let visits = read(&mut store, Symbol::namespaced("one", "visits"));
        assert_eq!(
            runtime_number(&mut store, &visits).unwrap(),
            Some(RuntimeNumber::Long(2))
        );
        runtime
            .get_func(&mut store, "dynamic-save")
            .unwrap()
            .call(&mut store, &[], &mut result)
            .unwrap();
        assert!(
            result[0].unwrap_anyref().is_none(),
            "load binding leaked after throw"
        );
    }
}
