//! Source graph preparation and actual cross-module initialization, never replay.
mod support;
use suss_compile::portable::{
    self,
    resolve::{Environment, Phase},
};

#[test]
fn required_source_is_compiled_before_the_importing_namespace() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("library.sus"),
        "(ns library) (def value 7)",
    )
    .unwrap();
    let source = "(ns app (:require [library :as lib])) (def value lib/value) value";
    std::fs::write(root.path().join("app.sus"), source).unwrap();
    let plan = portable::modules::prepare_modules(
        "app",
        &[root.path()],
        &Environment::default(),
        Phase::Runtime,
        &Default::default(),
    )
    .unwrap();
    assert_eq!(
        plan.modules
            .iter()
            .map(|module| module.identity.namespace())
            .collect::<Vec<_>>(),
        ["library", "app"]
    );
    let mut host = Host::new();
    let value = host.run(plan).unwrap().pop().unwrap();
    assert_eq!(host.bits(&value), 7.0f64.to_bits());
}

use portable::modules::{ModuleIdentity, ModulePlan, prepare_modules};
use std::collections::{BTreeMap, BTreeSet};
use suss_compile::runtime_abi;
use wasmtime::{
    Global, GlobalType, Instance, Linker, Module, Mutability, RefType, Store, Val, ValType,
};

struct Host {
    store: Store<Vec<u64>>,
    runtime: Instance,
    linker: Linker<Vec<u64>>,
    environment: Environment,
    cells: BTreeMap<portable::resolve::Global, Global>,
    provided: BTreeSet<ModuleIdentity>,
}
impl Host {
    fn new() -> Self {
        let engine = support::engine();
        let mut store = Store::new(&engine, Vec::new());
        let runtime = Instance::new(
            &mut store,
            &Module::new(&engine, runtime_abi::module()).unwrap(),
            &[],
        )
        .unwrap();
        let mut linker = Linker::new(&engine);
        linker.allow_shadowing(true);
        linker
            .instance(&mut store, "suss.runtime", runtime)
            .unwrap();
        let setter = runtime.get_func(&mut store, "binding-set").unwrap();
        let setter_type = setter.ty(&store);
        let trace =
            wasmtime::Func::new(&mut store, setter_type, move |mut caller, args, results| {
                let object = args[1]
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&caller)
                    .unwrap()
                    .unwrap();
                let fields = object.fields(&mut caller).unwrap().collect::<Vec<_>>();
                let [Val::F64(bits)] = fields.as_slice() else {
                    panic!("this trace fixture only defines Numbers")
                };
                caller.data_mut().push(*bits);
                setter.call(&mut caller, args, results)
            });
        linker
            .define(&store, "suss.runtime", "binding-set", trace)
            .unwrap();
        Self {
            store,
            runtime,
            linker,
            environment: Environment::default(),
            cells: BTreeMap::new(),
            provided: BTreeSet::new(),
        }
    }
    fn plan(
        &self,
        entry: &str,
        roots: &[&std::path::Path],
        phase: Phase,
    ) -> Result<ModulePlan, portable::modules::ModuleDiagnostic> {
        prepare_modules(entry, roots, &self.environment, phase, &self.provided)
    }
    fn run(&mut self, plan: ModulePlan) -> Result<Vec<Val>, wasmtime::Error> {
        // Validate every artifact before allocating bindings or running any initializer.
        let mut compiled = Vec::new();
        for module in &plan.modules {
            runtime_abi::verify_artifact(&module.wasm, &runtime_abi::Manifest::default())
                .map_err(wasmtime::Error::msg)?;
            compiled.push(Module::new(self.store.engine(), &module.wasm)?);
        }
        for identity in &plan.cells {
            if !self.cells.contains_key(identity) {
                let mut result = [Val::null_any_ref()];
                self.runtime
                    .get_func(&mut self.store, "binding-unbound")
                    .unwrap()
                    .call(&mut self.store, &[], &mut result)?;
                let ty = result[0]
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&self.store)?
                    .unwrap()
                    .ty(&self.store)?;
                let global = Global::new(
                    &mut self.store,
                    GlobalType::new(
                        ValType::Ref(RefType::new(false, ty.into())),
                        Mutability::Const,
                    ),
                    result[0].clone(),
                )?;
                self.linker.define(
                    &self.store,
                    identity.import_module(),
                    &identity.import_name(),
                    global,
                )?;
                self.cells.insert(identity.clone(), global);
            }
        }
        let mut instances = Vec::new();
        for module in compiled {
            instances.push(self.linker.instantiate(&mut self.store, &module)?);
        }
        self.environment = plan.environment;
        let mut values = Vec::new();
        for (module, instance) in plan.modules.iter().zip(instances) {
            let mut result = [Val::null_any_ref()];
            instance.get_func(&mut self.store, "eval").unwrap().call(
                &mut self.store,
                &[],
                &mut result,
            )?;
            self.provided.insert(module.identity.clone());
            self.store.gc(None)?;
            values.push(result[0].clone());
        }
        Ok(values)
    }
    fn bits(&mut self, value: &Val) -> u64 {
        let object = value
            .unwrap_anyref()
            .unwrap()
            .as_struct(&self.store)
            .unwrap()
            .unwrap();
        object
            .field(&mut self.store, 0)
            .unwrap()
            .unwrap_f64()
            .to_bits()
    }
    fn read(&mut self, name: &str) -> Val {
        let bytes = portable::compile_in(name, &self.environment, Phase::Runtime).unwrap();
        let module = Module::new(self.store.engine(), bytes).unwrap();
        let instance = self.linker.instantiate(&mut self.store, &module).unwrap();
        let mut result = [Val::null_any_ref()];
        instance
            .get_func(&mut self.store, "eval")
            .unwrap()
            .call(&mut self.store, &[], &mut result)
            .unwrap();
        result[0].clone()
    }
}
fn file(root: &std::path::Path, relative: &str, source: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
}

#[test]
fn diamond_graph_initializes_once_in_require_order_and_reuses_provided_modules() {
    let root = tempfile::tempdir().unwrap();
    file(root.path(), "shared.sus", "(ns shared) (def value 1)");
    file(
        root.path(),
        "left.cljs",
        "(ns left (:require shared)) (def value 2)",
    );
    file(
        root.path(),
        "right.cljc",
        "(ns right (:require shared)) (def value 3)",
    );
    file(
        root.path(),
        "app.sus",
        "(ns app (:require [left :as left] [right :as right] [shared :as shared])) (def value 4) shared/value",
    );
    let mut host = Host::new();
    let plan = host.plan("app", &[root.path()], Phase::Runtime).unwrap();
    assert_eq!(
        plan.modules
            .iter()
            .map(|module| module.identity.namespace())
            .collect::<Vec<_>>(),
        ["shared", "left", "right", "app"]
    );
    let values = host.run(plan).unwrap();
    assert_eq!(host.bits(values.last().unwrap()), 1.0f64.to_bits());
    assert_eq!(
        host.store.data(),
        &[
            1.0f64.to_bits(),
            2.0f64.to_bits(),
            3.0f64.to_bits(),
            4.0f64.to_bits()
        ]
    );
    // Already initialized is explicit host authority, not inferred from a namespace.
    file(root.path(), "shared.sus", "(ns wrong) unsupported");
    let plan = host.plan("app", &[root.path()], Phase::Runtime).unwrap();
    assert!(plan.modules.is_empty());
    host.run(plan).unwrap();
    assert_eq!(host.store.data().len(), 4);
    let value = host.read("shared/value");
    assert_eq!(host.bits(&value), 1.0f64.to_bits());
}

#[test]
fn source_failures_locate_declared_namespace_missing_edges_and_cycles() {
    let root = tempfile::tempdir().unwrap();
    let host = Host::new();
    file(root.path(), "app.sus", "(ns other)");
    let error = host
        .plan("app", &[root.path()], Phase::Runtime)
        .unwrap_err();
    assert!(error.message.contains("does not match"));
    assert_eq!(error.span, 4..9);
    assert_eq!(
        error.source_path.unwrap(),
        root.path().join("app.sus").canonicalize().unwrap()
    );
    file(root.path(), "app.sus", "(def value 7)");
    assert!(
        host.plan("app", &[root.path()], Phase::Runtime)
            .unwrap_err()
            .message
            .contains("leading ns")
    );
    let source = "(ns app (:require missing))";
    file(root.path(), "app.sus", source);
    let error = host
        .plan("app", &[root.path()], Phase::Runtime)
        .unwrap_err();
    assert_eq!(&source[error.span], "missing");
    assert_eq!(error.namespace, "missing");
    assert!(error.source_path.unwrap().ends_with("app.sus"));
    file(root.path(), "app.sus", "(ns app (:require dependency))");
    file(
        root.path(),
        "dependency.sus",
        "(ns dependency (:require app))",
    );
    let a = host
        .plan("app", &[root.path()], Phase::Runtime)
        .unwrap_err();
    let b = host
        .plan("app", &[root.path()], Phase::Runtime)
        .unwrap_err();
    assert_eq!(a.to_string(), b.to_string());
    assert!(a.message.contains("app -> dependency -> app"));
    assert!(a.source_path.unwrap().ends_with("dependency.sus"));
}

#[test]
fn ambiguity_is_rejected_and_duplicate_canonical_roots_do_not_duplicate_modules() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    file(
        first.path(),
        "my_app/core.sus",
        "(ns my-app.core) (def value -0.0) value",
    );
    let mut host = Host::new();
    let plan = host
        .plan("my-app.core", &[first.path(), first.path()], Phase::Runtime)
        .unwrap();
    assert_eq!(plan.modules.len(), 1);
    let value = host.run(plan).unwrap().pop().unwrap();
    assert_eq!(host.bits(&value), (-0.0f64).to_bits());
    let host = Host::new();
    file(first.path(), "my_app/core.cljs", "(ns my-app.core)");
    assert!(
        host.plan("my-app.core", &[first.path()], Phase::Runtime)
            .unwrap_err()
            .message
            .contains("Ambiguous")
    );
    file(second.path(), "my_app/core.sus", "(ns my-app.core)");
    let error = host
        .plan(
            "my-app.core",
            &[second.path(), first.path()],
            Phase::Runtime,
        )
        .unwrap_err();
    assert!(error.message.contains("Ambiguous"));
    assert!(error.source_path.is_none());
}

#[test]
fn graph_compilation_failure_does_not_publish_bindings_or_execute_dependencies() {
    let root = tempfile::tempdir().unwrap();
    file(
        root.path(),
        "dependency.sus",
        "(ns dependency) (def value 1)",
    );
    file(
        root.path(),
        "app.sus",
        "(ns app (:require dependency)) (def value unresolved)",
    );
    let host = Host::new();
    let error = host
        .plan("app", &[root.path()], Phase::Runtime)
        .unwrap_err();
    assert!(error.message.contains("Unresolved"));
    assert!(error.source_path.unwrap().ends_with("app.sus"));
    assert!(host.environment.cells().is_empty());
    assert!(!host.environment.has_namespace(Phase::Runtime, "dependency"));
    assert!(host.cells.is_empty());
    assert!(host.store.data().is_empty());
}

#[test]
fn failed_initializer_keeps_prior_effects_and_only_successful_modules_are_provided() {
    let root = tempfile::tempdir().unwrap();
    file(
        root.path(),
        "dependency.sus",
        "(ns dependency) (def value 1)",
    );
    file(
        root.path(),
        "app.sus",
        "(ns app (:require dependency)) (def value 2) (def value (1))",
    );
    let mut host = Host::new();
    let plan = host.plan("app", &[root.path()], Phase::Runtime).unwrap();
    let error = host.run(plan).unwrap_err();
    assert!(error.is::<wasmtime::ThrownException>(), "{error:?}");
    let exception = host.store.take_pending_exception().unwrap();
    let tag = exception.tag(&mut host.store).unwrap();
    let language_tag = host
        .runtime
        .get_tag(&mut host.store, "language-exception")
        .unwrap();
    assert!(wasmtime::Tag::eq(&tag, &language_tag, &host.store));
    let payload = exception.field(&mut host.store, 0).unwrap();
    host.store.gc(None).unwrap();
    let fields = payload
        .unwrap_anyref()
        .unwrap()
        .as_struct(&host.store)
        .unwrap()
        .unwrap()
        .fields(&mut host.store)
        .unwrap()
        .collect::<Vec<_>>();
    let descriptor = fields[0]
        .unwrap_anyref()
        .unwrap()
        .as_struct(&host.store)
        .unwrap()
        .unwrap();
    assert_eq!(descriptor.field(&mut host.store, 0).unwrap().i64(), Some(3));
    assert!(
        host.provided
            .contains(&ModuleIdentity::new(Phase::Runtime, "dependency").unwrap())
    );
    assert!(
        !host
            .provided
            .contains(&ModuleIdentity::new(Phase::Runtime, "app").unwrap())
    );
    assert_eq!(host.store.data(), &[1.0f64.to_bits(), 2.0f64.to_bits()]);
    let previous = host.read("app/value");
    assert_eq!(host.bits(&previous), 2.0f64.to_bits());
    // Retrying the failed module must not replay its successful dependency.
    file(root.path(), "dependency.sus", "not valid source anymore");
    file(
        root.path(),
        "app.sus",
        "(ns app (:require dependency)) (def value 3) value",
    );
    let plan = host.plan("app", &[root.path()], Phase::Runtime).unwrap();
    assert_eq!(plan.modules.len(), 1);
    let value = host.run(plan).unwrap().pop().unwrap();
    assert_eq!(host.bits(&value), 3.0f64.to_bits());
    assert_eq!(
        host.store.data(),
        &[1.0f64.to_bits(), 2.0f64.to_bits(), 3.0f64.to_bits()]
    );
}

#[test]
fn immutable_source_snapshots_and_artifact_gates_precede_initialization() {
    let root = tempfile::tempdir().unwrap();
    file(
        root.path(),
        "dependency.sus",
        "(ns dependency) (def value 1)",
    );
    file(
        root.path(),
        "app.sus",
        "(ns app (:require dependency)) (def value 2) dependency/value",
    );
    let mut host = Host::new();
    let mut plan = host.plan("app", &[root.path()], Phase::Runtime).unwrap();
    plan.modules[1].wasm.clear();
    assert!(host.run(plan).is_err());
    assert!(host.store.data().is_empty());
    assert!(host.cells.is_empty());
    let plan = host.plan("app", &[root.path()], Phase::Runtime).unwrap();
    file(root.path(), "dependency.sus", "not the compiled source");
    let values = host.run(plan).unwrap();
    assert_eq!(host.bits(values.last().unwrap()), 1.0f64.to_bits());
}

#[test]
fn conditional_dependencies_follow_first_portable_branch_and_phase_identity() {
    let root = tempfile::tempdir().unwrap();
    file(root.path(), "selected.cljs", "(ns selected) (def value 7)");
    file(
        root.path(),
        "app.cljc",
        "(ns app #?(:cljs (:require [selected :as chosen]) :suss (:require missing))) (def value chosen/value) value",
    );
    let mut host = Host::new();
    let runtime = host.plan("app", &[root.path()], Phase::Runtime).unwrap();
    assert_eq!(runtime.modules.len(), 2);
    let values = host.run(runtime).unwrap();
    assert_eq!(host.bits(values.last().unwrap()), 7.0f64.to_bits());
    // Runtime availability does not skip the same source in the Macro phase.
    let macro_phase = host.plan("app", &[root.path()], Phase::Macro).unwrap();
    assert_eq!(macro_phase.modules.len(), 2);
    assert!(
        macro_phase
            .modules
            .iter()
            .all(|module| module.identity.phase() == Phase::Macro)
    );
    host.run(macro_phase).unwrap();
    assert_eq!(host.cells.len(), 4);
}

#[test]
fn declared_catalog_is_not_loaded_and_bad_headers_fail_before_dependency_search() {
    let root = tempfile::tempdir().unwrap();
    let mut host = Host::new();
    host.environment
        .declare_namespace(Phase::Runtime, "missing")
        .unwrap();
    file(root.path(), "app.sus", "(ns app (:require missing))");
    assert!(
        host.plan("app", &[root.path()], Phase::Runtime)
            .unwrap_err()
            .message
            .contains("No source")
    );
    for source in [
        "(ns app (:require [missing :unsupported true]))",
        "(ns app (:require ^:reload [missing]))",
        "(ns app (:require-macros missing))",
    ] {
        file(root.path(), "app.sus", source);
        let error = host
            .plan("app", &[root.path()], Phase::Runtime)
            .unwrap_err();
        assert!(!error.message.contains("No source"));
        assert_eq!(error.namespace, "app");
        assert!(error.span.start < error.span.end);
    }
    host.provided
        .insert(ModuleIdentity::new(Phase::Runtime, "undeclared").unwrap());
    file(root.path(), "app.sus", "(ns app (:require undeclared))");
    assert!(
        host.plan("app", &[root.path()], Phase::Runtime)
            .unwrap_err()
            .message
            .contains("no matching phase declaration")
    );
}

#[test]
fn bootstrap_core_aliases_are_explicit_provided_identities_and_body_refs_execute() {
    let root = tempfile::tempdir().unwrap();
    let mut host = Host::new();
    host.provided
        .insert(ModuleIdentity::new(Phase::Runtime, "cljs.core").unwrap());
    file(
        root.path(),
        "app.sus",
        "(ns app (:require [cljs.core :as core] [suss.core :refer [+] :rename {+ sum}])) (def value (sum 2 3)) (core/+ 1 2)",
    );
    let plan = host.plan("app", &[root.path()], Phase::Runtime).unwrap();
    assert_eq!(plan.modules.len(), 1);
    assert_eq!(plan.modules[0].dependencies.len(), 1);
    assert_eq!(plan.modules[0].dependencies[0].namespace(), "suss.core");
    let value = host.run(plan).unwrap().pop().unwrap();
    assert_eq!(host.bits(&value), 3.0f64.to_bits());
}

#[test]
fn dependency_nesting_is_bounded_with_a_located_diagnostic() {
    let root = tempfile::tempdir().unwrap();
    for index in 0..65 {
        let source = if index == 64 {
            format!("(ns n{index})")
        } else {
            format!("(ns n{index} (:require n{}))", index + 1)
        };
        file(root.path(), &format!("n{index}.sus"), &source);
    }
    let host = Host::new();
    let error = host.plan("n0", &[root.path()], Phase::Runtime).unwrap_err();
    assert!(error.message.contains("nesting exceeds 64"));
    assert!(error.source_path.unwrap().ends_with("n63.sus"));
    assert!(error.span.start < error.span.end);
}
