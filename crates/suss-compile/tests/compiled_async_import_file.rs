//! Public file compilation: compiled macros, dependency initialization and
//! genuinely pending canonical imports through production assembly.
use std::{
    future::Future,
    pin::pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};
use suss_compile::portable_aot::{AotOptions, AsyncImportMapping, compile_file_with_options};
use suss_reader::Symbol;
use wasmtime::{
    Config, Engine, Store,
    component::{Component, Linker, Val},
};
struct WakeThread(std::thread::Thread);
impl Wake for WakeThread {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}
fn drive<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let waker = Waker::from(Arc::new(WakeThread(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => {
                assert!(
                    std::time::Instant::now() < deadline,
                    "generated transport stalled"
                );
                std::thread::park_timeout(std::time::Duration::from_millis(5));
            }
        }
    }
}
#[test]
fn public_file_async_import_uses_macros_and_initializes_dependencies_once() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    // defonce keeps the counter alive if an initializer is accidentally replayed.
    std::fs::write(root.join("dependency.sus"),
        "(ns dependency) (defonce initialized 0) (def offset (do (set! initialized (+ initialized 1)) initialized))").unwrap();
    let source = root.join("app.sus");
    std::fs::write(
        &source,
        r#"
        (ns app (:require [dependency :as dep] [host] [suss.async])
                (:require-macros [suss.async :refer [future await]]))
        (def calls 0)
        (def run (fn [x]
          (future
            (set! calls (+ calls 1))
            (+ (await (host/increment x)) dep/offset calls))))
    "#,
    )
    .unwrap();
    let wit = root.join("api.wit");
    std::fs::write(&wit, "package test:file-async; world api { import increment: async func(value: u32) -> u32; export run: async func(value: u32) -> u32; }").unwrap();
    let options = AotOptions {
        exports: vec![("run".into(), Symbol::namespaced("app", "run"))],
        async_imports: vec![AsyncImportMapping {
            wit_name: "increment".into(),
            source: Symbol::namespaced("host", "increment"),
        }],
    };
    let bytes = compile_file_with_options(&source, &wit, Some("api"), &[root.to_owned()], &options)
        .expect("public AOT source/macro/dependency route");
    let mut config = Config::new();
    config
        .wasm_gc(true)
        .wasm_function_references(true)
        .wasm_tail_call(true)
        .wasm_exceptions(true)
        .wasm_component_model_async(true)
        .wasm_component_model_async_stackful(false)
        .wasm_component_model_more_async_builtins(true)
        .consume_fuel(true)
        .cranelift_opt_level(wasmtime::OptLevel::None);
    let engine = Engine::new(&config).unwrap();
    let component = Component::new(&engine, bytes).unwrap();
    let mut linker = Linker::new(&engine);
    let polls = Arc::new(AtomicUsize::new(0));
    let count = polls.clone();
    linker
        .root()
        .func_new_concurrent("increment", move |_, _, params, results| {
            let count = count.clone();
            Box::pin(async move {
                let mut first = true;
                std::future::poll_fn(|cx| {
                    count.fetch_add(1, Ordering::SeqCst);
                    if first {
                        first = false;
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    } else {
                        Poll::Ready(())
                    }
                })
                .await;
                let [Val::U32(value)] = params else {
                    panic!("resolved scalar input")
                };
                results[0] = Val::U32(value + 1);
                Ok(())
            })
        })
        .unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(2_000_000).unwrap();
    let instance = drive(linker.instantiate_async(&mut store, &component)).unwrap();
    let run = instance
        .get_typed_func::<(u32,), (u32,)>(&mut store, "run")
        .unwrap();
    for (index, input) in [40, 17, 0].into_iter().enumerate() {
        let output = drive(
            store.run_concurrent(async |accessor| run.call_concurrent(accessor, (input,)).await),
        )
        .unwrap()
        .unwrap();
        assert_eq!(output, (input + 3 + index as u32,));
        store.gc(None).unwrap();
    }
    assert!(
        polls.load(Ordering::SeqCst) >= 6,
        "each generated import genuinely suspended"
    );
}

fn mapping_fixture(
    source: &str,
) -> (
    tempfile::TempDir,
    std::path::PathBuf,
    std::path::PathBuf,
    AotOptions,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.sus");
    let wit = directory.path().join("api.wit");
    std::fs::write(&path, source).unwrap();
    std::fs::write(&wit, "package test:mappings; world api { import increment: async func(value: u32) -> u32; export run: async func(value: u32) -> u32; }").unwrap();
    let options = AotOptions {
        exports: vec![("run".into(), Symbol::namespaced("app", "run"))],
        async_imports: vec![AsyncImportMapping {
            wit_name: "increment".into(),
            source: Symbol::namespaced("host", "increment"),
        }],
    };
    (directory, path, wit, options)
}

#[test]
fn file_async_mappings_reject_core_spellings_before_macro_analysis() {
    for namespace in ["suss.core", "cljs.core"] {
        // Unresolved source would fail if preparation preceded mapping validation.
        let (root, source, wit, mut options) =
            mapping_fixture("(ns app) (unresolved-before-macro)");
        options.async_imports[0].source = Symbol::namespaced(namespace, "increment");
        let error = compile_file_with_options(
            &source,
            &wit,
            Some("api"),
            &[root.path().to_owned()],
            &options,
        )
        .unwrap_err();
        assert!(
            error.contains("outside core imports"),
            "{namespace}: {error}"
        );
    }
}

#[test]
fn file_async_mapping_rejects_source_owned_host_namespace_before_preparation() {
    let (root, source, wit, options) = mapping_fixture("(ns app) (unresolved-before-macro)");
    std::fs::write(root.path().join("host.sus"), "(ns host) (def helper 7)").unwrap();
    let error = compile_file_with_options(
        &source,
        &wit,
        Some("api"),
        &[root.path().to_owned()],
        &options,
    )
    .unwrap_err();
    assert!(error.contains("owned by a source module"), "{error}");
}

#[test]
fn file_async_mapping_rejects_import_overwrite_and_uninitialized_export() {
    for (source_text, expected) in [
        (
            "(ns host) (def increment (fn [x] x)) (ns app) (def run (fn [x] (suss.async/future* x)))",
            "import cell must not have a source definition",
        ),
        ("(ns app) (def run)", "completed source initializer"),
        (
            "(ns app) (def other (fn [x] x))",
            "literal source namespace definition",
        ),
    ] {
        let (root, source, wit, options) = mapping_fixture(source_text);
        let error = compile_file_with_options(
            &source,
            &wit,
            Some("api"),
            &[root.path().to_owned()],
            &options,
        )
        .unwrap_err();
        assert!(error.contains(expected), "expected {expected}: {error}");
    }
}

#[test]
fn file_async_mapping_cannot_redirect_literal_import_or_export_through_alias() {
    // The resolver rejects conflicting import namespace aliases before assembly;
    // the export-only case reaches literal identity validation.
    for import_alias in [true, false] {
        let (root, source, wit, mut options) = mapping_fixture(
            "(ns app (:require [other :as host])) (def run (fn [x] (suss.async/future* x)))",
        );
        std::fs::write(
            root.path().join("other.sus"),
            "(ns other) (def increment (fn [x] x)) (def run (fn [x] (suss.async/future* x)))",
        )
        .unwrap();
        if !import_alias {
            options.async_imports[0].source = Symbol::namespaced("native", "increment");
            options.exports[0].1 = Symbol::namespaced("host", "run");
        }
        let error = compile_file_with_options(
            &source,
            &wit,
            Some("api"),
            &[root.path().to_owned()],
            &options,
        )
        .unwrap_err();
        assert!(
            error.contains(if import_alias {
                "Ambiguous namespace alias host"
            } else {
                "literal source namespace definition"
            }),
            "{error}"
        );
    }
}

#[test]
fn file_async_host_namespace_ambiguity_is_not_treated_as_absent_source() {
    let (root, source, wit, options) = mapping_fixture("(ns app) (unresolved-before-macro)");
    for extension in ["sus", "cljs"] {
        std::fs::write(root.path().join(format!("host.{extension}")), "(ns host)").unwrap();
    }
    let error = compile_file_with_options(
        &source,
        &wit,
        Some("api"),
        &[root.path().to_owned()],
        &options,
    )
    .unwrap_err();
    assert!(
        error.contains("Ambiguous source for namespace host"),
        "{error}"
    );
}

#[cfg(unix)]
#[test]
fn file_async_broken_host_source_link_is_not_treated_as_absent_source() {
    let (root, source, wit, options) = mapping_fixture("(ns app) (unresolved-before-macro)");
    std::os::unix::fs::symlink(
        root.path().join("missing.sus"),
        root.path().join("host.sus"),
    )
    .unwrap();
    let error = compile_file_with_options(
        &source,
        &wit,
        Some("api"),
        &[root.path().to_owned()],
        &options,
    )
    .unwrap_err();
    assert!(error.contains("Cannot resolve namespace source"), "{error}");
}
