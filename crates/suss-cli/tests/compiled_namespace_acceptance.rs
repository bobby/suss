//! Execute the same namespace resolution contract in the REPL and native AOT.
use std::io::Write;
use std::{
    path::Path,
    process::{Command, Output, Stdio},
    sync::OnceLock,
};
use wasmtime::component::{Component, Linker};
use wasmtime::{AsContextMut, Config, Engine, Store};

fn compile(root: &Path, namespace: &str, flags: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root)
        .args([
            "compile",
            "-n",
            namespace,
            "-w",
            "api.wit",
            "--wit-world",
            "api-world",
            "--export",
            "api#calculate=app.core/calculate",
            "--export",
            "api#effects=app.core/effects",
            "-o",
            "app.wasm",
        ])
        .args(flags)
        .output()
        .unwrap()
}
fn engine() -> Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let mut config = Config::new();
            config
                .wasm_gc(true)
                .wasm_function_references(true)
                .wasm_tail_call(true)
                .wasm_exceptions(true)
                .wasm_component_model(true)
                .wasm_component_model_implements(true)
                .consume_fuel(true)
                .cranelift_opt_level(wasmtime::OptLevel::None);
            Engine::new(&config).unwrap()
        })
        .clone()
}
fn wit(root: &Path) {
    std::fs::write(root.join("api.wit"), "package test:namespace; interface operations { calculate: func(x: u32) -> u32; effects: func() -> f64; } world api-world { export api: operations; } world other {}").unwrap();
}

#[test]
fn namespace_session_aliases_refers_renames_exclusions_and_phases_agree_in_repl_and_aot() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("src");
    std::fs::create_dir_all(source.join("app")).unwrap();
    std::fs::write(source.join("dep.cljs"), "(ns dep) (def answer (fn [] 7))").unwrap();
    std::fs::write(source.join("tools.sus"),
        "(ns tools (:require [dep :as d :refer [answer] :rename {answer bias}])) (defmacro twice [x] `(+ ~x ~x ~(bias)))").unwrap();
    std::fs::write(
        source.join("app/core.cljc"),
        r#"
#?(:cljs
   (ns app.core
     (:refer-clojure :exclude [+])
     (:require [dep :as d :refer [answer] :rename {answer renamed}]
               [cljs.core :as c])
     (:require-macros [tools :as d :refer [twice] :rename {twice doubled}]))
   :suss (ns wrong))
(def + (fn [x y] (c/- x y)))
(def calculate (fn [x] (c/+ (d/twice x) (d/answer) (renamed) (doubled x) (+ 20 3))))
(def effects (fn [] 1))
"#,
    )
    .unwrap();
    // Both matching reader features must select the first textual branch.
    // Runtime and Macro aliases deliberately share the spelling d.
    let mut child = Command::new(env!("CARGO_BIN_EXE_suss"))
        .arg("repl")
        .current_dir(root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b":load app.core\n(app.core/calculate 2)\n(app.core/calculate 3)\n:quit\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "nil\n53\n57\n");

    wit(root.path());
    let output = compile(root.path(), "app.core", &[]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let engine = engine();
    let component = Component::from_file(&engine, root.path().join("app.wasm")).unwrap();
    let interface = component.get_export_index(None, "api").unwrap();
    let calculate_index = component
        .get_export_index(Some(&interface), "calculate")
        .unwrap();
    let mut store = Store::new(&engine, ());
    store.set_fuel(40_000_000).unwrap();
    let instance = Linker::<()>::new(&engine)
        .instantiate(&mut store, &component)
        .unwrap();
    let calculate = instance
        .get_typed_func::<(u32,), (u32,)>(&mut store, &calculate_index)
        .unwrap();
    assert_eq!(calculate.call(&mut store, (2,)).unwrap().0, 53);
    store.gc(None).unwrap();
    assert_eq!(calculate.call(&mut store, (3,)).unwrap().0, 57);
}
