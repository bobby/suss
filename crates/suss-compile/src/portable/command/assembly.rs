//! Assemble reachable official command bindings around the shared source runtime.
use super::{adapter, diagnostic, exit, memory, official_profile, shape};
use crate::{
    portable::{Diagnostic, PreparedFragment, artifact_identity, core_bindings, resolve::Phase},
    runtime_abi,
};
use suss_reader::Symbol;
use wasm_encoder::*;
use wit_parser::WorldItem;

pub fn component(fragments: &[PreparedFragment], main: &Symbol) -> Result<Vec<u8>, Diagnostic> {
    assemble(fragments, main, None)
}

/// Bind the resolved exit capability to an explicitly selected Runtime var.
pub fn component_with_exit(
    fragments: &[PreparedFragment],
    main: &Symbol,
    exit: &Symbol,
) -> Result<Vec<u8>, Diagnostic> {
    assemble(fragments, main, Some(exit))
}

fn assemble(
    fragments: &[PreparedFragment],
    main: &Symbol,
    exit_symbol: Option<&Symbol>,
) -> Result<Vec<u8>, Diagnostic> {
    let (resolve, world) = official_profile()?;
    let selected = &resolve.worlds[world];
    let (environment_name, arguments) = selected
        .imports
        .iter()
        .find_map(|(key, item)| {
            let WorldItem::Interface { id, .. } = item else {
                return None;
            };
            let interface = &resolve.interfaces[*id];
            if interface.package != selected.package
                || interface.name.as_deref() != Some("environment")
            {
                return None;
            }
            Some((
                resolve.name_world_key(key),
                interface.functions.get("get-arguments")?,
            ))
        })
        .ok_or_else(|| diagnostic("Official command argument interface missing"))?;
    let (run_name, run) = selected
        .exports
        .iter()
        .find_map(|(key, item)| {
            let WorldItem::Interface { id, .. } = item else {
                return None;
            };
            let interface = &resolve.interfaces[*id];
            if interface.package != selected.package || interface.name.as_deref() != Some("run") {
                return None;
            }
            Some((resolve.name_world_key(key), interface.functions.get("run")?))
        })
        .ok_or_else(|| diagnostic("Official command run interface missing"))?;
    let last = fragments
        .last()
        .ok_or_else(|| diagnostic("Command needs prepared source"))?;
    let binding = last.environment.resolve(Phase::Runtime, main, 0..0)?;
    let exit = exit_symbol
        .map(|symbol| {
            let cell = last
                .environment
                .resolve(Phase::Runtime, symbol, 0..0)?
                .global()
                .clone();
            let (name, function) = selected
                .imports
                .iter()
                .find_map(|(key, item)| {
                    let WorldItem::Interface { id, .. } = item else {
                        return None;
                    };
                    let interface = &resolve.interfaces[*id];
                    if interface.package != selected.package
                        || interface.name.as_deref() != Some("exit")
                    {
                        return None;
                    }
                    Some((
                        resolve.name_world_key(key),
                        interface.functions.get("exit-with-code")?,
                    ))
                })
                .ok_or_else(|| diagnostic("Official command exit interface missing"))?;
            Ok::<_, Diagnostic>((name, function, cell))
        })
        .transpose()?;
    let exit_count = u32::from(exit.is_some());
    for fragment in fragments {
        runtime_abi::verify_artifact(&fragment.wasm, &runtime_abi::Manifest::default())
            .map_err(diagnostic)?;
        artifact_identity::verify(
            &fragment.wasm,
            artifact_identity::Expected {
                phase: Some(Phase::Runtime),
                ..Default::default()
            },
        )
        .map_err(diagnostic)?;
    }
    let mut cells = fragments
        .iter()
        .flat_map(|f| f.cells.iter().cloned())
        .collect::<Vec<_>>();
    if let Some((_, _, cell)) = &exit {
        cells.push(cell.clone());
    }
    let bindings = core_bindings::compile_with_cells(Phase::Runtime, &cells)?;
    let adapter = adapter::module(&resolve, arguments, run, binding.global(), fragments.len())?;
    let mut types = shape::Types::new(&resolve);
    let arguments_type = types.function(arguments)?;
    let run_type = types.function(run)?;
    let completion_type = types.value(
        run.result
            .ok_or_else(|| diagnostic("Official command result missing"))?,
    )?;
    let environment_type = types.section.len();
    let mut environment = InstanceType::new();
    environment.alias(Alias::Outer {
        kind: ComponentOuterAliasKind::Type,
        count: 1,
        index: arguments_type,
    });
    environment.export(arguments.name.as_str(), ComponentTypeRef::Func(0));
    types.section.instance(&environment);
    let mut imports = ComponentImportSection::new();
    imports.import(
        &environment_name,
        ComponentTypeRef::Instance(environment_type),
    );
    if let Some((name, function, _)) = &exit {
        let function_type = types.function(function)?;
        let instance_type = types.section.len();
        let mut instance = InstanceType::new();
        instance.alias(Alias::Outer {
            kind: ComponentOuterAliasKind::Type,
            count: 1,
            index: function_type,
        });
        instance.export(function.name.as_str(), ComponentTypeRef::Func(0));
        types.section.instance(&instance);
        imports.import(name, ComponentTypeRef::Instance(instance_type));
    }
    let mut component = Component::new();
    component.section(&types.section).section(&imports);
    let mut aliases = ComponentAliasSection::new();
    aliases.alias(Alias::InstanceExport {
        instance: 0,
        kind: ComponentExportKind::Func,
        name: &arguments.name,
    });
    if let Some((_, function, _)) = &exit {
        aliases.alias(Alias::InstanceExport {
            instance: 1,
            kind: ComponentExportKind::Func,
            name: &function.name,
        });
    }
    component.section(&aliases);
    let exit_module = exit
        .as_ref()
        .map(|(_, function, cell)| exit::module(&resolve, function, cell))
        .transpose()?;
    let modules = [runtime_abi::module(), memory::module(), bindings.wasm]
        .into_iter()
        .chain(fragments.iter().map(|f| f.wasm.clone()))
        .chain(std::iter::once(adapter))
        .chain(exit_module);
    for module in modules {
        component.section(&RawSection {
            id: ComponentSectionId::CoreModule.into(),
            data: &module,
        });
    }
    let mut instances = InstanceSection::new();
    instances.instantiate(0, std::iter::empty::<(&str, ModuleArg)>());
    instances.instantiate(1, std::iter::empty::<(&str, ModuleArg)>());
    instances.instantiate(2, [("suss.runtime", ModuleArg::Instance(0))]);
    for index in 0..fragments.len() {
        instances.instantiate(
            3 + index as u32,
            [
                ("suss.runtime", ModuleArg::Instance(0)),
                ("suss.bindings.runtime", ModuleArg::Instance(2)),
            ],
        );
    }
    component.section(&instances);
    let mut aliases = ComponentAliasSection::new();
    aliases.alias(Alias::CoreInstanceExport {
        instance: 1,
        kind: ExportKind::Memory,
        name: "memory",
    });
    aliases.alias(Alias::CoreInstanceExport {
        instance: 1,
        kind: ExportKind::Func,
        name: "cabi_realloc",
    });
    aliases.alias(Alias::CoreInstanceExport {
        instance: 1,
        kind: ExportKind::Func,
        name: "release",
    });
    component.section(&aliases);
    let mut canonical = CanonicalFunctionSection::new();
    canonical.lower(
        0,
        [
            CanonicalOption::Memory(0),
            CanonicalOption::Realloc(0),
            CanonicalOption::UTF16,
        ],
    );
    if exit.is_some() {
        canonical.lower(1, []);
    }
    canonical.task_return(Some(completion_type), []);
    component.section(&canonical);
    let shim = 3 + fragments.len() as u32;
    let adapter_instance = shim + 1 + exit_count;
    let mut instances = InstanceSection::new();
    let mut shim_exports = vec![
        ("arguments", ExportKind::Func, 2),
        ("realloc", ExportKind::Func, 0),
        ("release", ExportKind::Func, 1),
        ("complete", ExportKind::Func, 3 + exit_count),
    ];
    if exit.is_some() {
        shim_exports.push(("exit", ExportKind::Func, 3));
    }
    instances.export_items(shim_exports);
    if exit.is_some() {
        instances.instantiate(
            4 + fragments.len() as u32,
            [
                ("suss.runtime", ModuleArg::Instance(0)),
                ("suss.bindings.runtime", ModuleArg::Instance(2)),
                ("suss.canonical", ModuleArg::Instance(shim)),
            ],
        );
    }
    let mut args = vec![
        ("suss.runtime".to_owned(), ModuleArg::Instance(0)),
        ("suss.memory".to_owned(), ModuleArg::Instance(1)),
        ("suss.bindings.runtime".to_owned(), ModuleArg::Instance(2)),
        ("suss.canonical".to_owned(), ModuleArg::Instance(shim)),
    ];
    args.extend((0..fragments.len()).map(|index| {
        (
            format!("suss.fragment.{index}"),
            ModuleArg::Instance(3 + index as u32),
        )
    }));
    instances.instantiate(3 + fragments.len() as u32, args);
    component.section(&instances);
    let mut aliases = ComponentAliasSection::new();
    aliases.alias(Alias::CoreInstanceExport {
        instance: adapter_instance,
        kind: ExportKind::Func,
        name: "entry",
    });
    aliases.alias(Alias::CoreInstanceExport {
        instance: adapter_instance,
        kind: ExportKind::Func,
        name: "callback",
    });
    component.section(&aliases);
    let mut canonical = CanonicalFunctionSection::new();
    canonical.lift(
        4 + exit_count,
        run_type,
        [
            CanonicalOption::Async,
            CanonicalOption::Callback(5 + exit_count),
        ],
    );
    component.section(&canonical);
    let mut public_instances = ComponentInstanceSection::new();
    public_instances.export_items([(run.name.as_str(), ComponentExportKind::Func, 1 + exit_count)]);
    component.section(&public_instances);
    let mut exports = ComponentExportSection::new();
    exports.export(
        &run_name,
        ComponentExportKind::Instance,
        1 + exit_count,
        None,
    );
    component.section(&exports);
    let bytes = component.finish();
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&bytes)
        .map_err(|error| diagnostic(format!("Invalid official command component: {error}")))?;
    Ok(bytes)
}
