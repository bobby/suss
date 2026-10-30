//! Persistent build directory and caches across separate CLI processes (#272).
//! Every run uses its own temporary build directory, seeded with the shipped
//! sessions' native code so tests pay one cold bootstrap compilation.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::OnceLock,
};

fn suss(cwd: &Path, build_dir: Option<&Path>, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_suss"));
    command
        .current_dir(cwd)
        .env("SUSS_CACHE_TRACE", "1")
        .env_remove("SUSS_BUILD_DIR")
        .env_remove("SUSS_NO_CACHE");
    if let Some(build_dir) = build_dir {
        command.arg("--build-dir").arg(build_dir);
    }
    command.args(args).output().unwrap()
}
fn success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
/// Trace events by `layer event`.
fn events(output: &Output) -> BTreeMap<String, usize> {
    let mut events = BTreeMap::new();
    for line in String::from_utf8_lossy(&output.stderr).lines() {
        if let Some(rest) = line.strip_prefix("suss-cache: ") {
            let mut words = rest.split_whitespace();
            let key = format!("{} {}", words.next().unwrap(), words.next().unwrap());
            *events.entry(key).or_default() += 1;
        }
    }
    events
}
fn count(output: &Output, event: &str) -> usize {
    events(output).get(event).copied().unwrap_or(0)
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}
/// A build directory holding native code for the shipped sessions.
fn seeded(root: &Path) -> PathBuf {
    static SEED: OnceLock<tempfile::TempDir> = OnceLock::new();
    let seed = SEED.get_or_init(|| {
        let seed = tempfile::tempdir().unwrap();
        let output = suss(seed.path(), Some(&seed.path().join("build")), &["-e", "(+ 1 2)"]);
        success(&output);
        seed
    });
    let build = root.join("build");
    copy_tree(&seed.path().join("build"), &build);
    build
}

const WIT: &str = "package test:cache@1.0.0; interface operations { calculate: func(x: u32) -> u32; } world app { export api: operations; }";
fn tools(bias: u32) -> String {
    format!("(ns tools (:require [dep :as d])) (defmacro twice [x] `(let [y# ~x] (+ y# y# ~(+ {bias} (- d/counter 1)))))")
}
/// A project whose entry uses a macro namespace that requires a runtime one.
fn project(root: &Path) {
    let write = |path: &str, text: &str| {
        let path = root.join("project").join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write("src/dep.cljs", "(ns dep) (def counter 0) (set! counter (+ counter 1))");
    write("src/tools.cljs", &tools(0));
    write(
        "src/app/core.cljc",
        "(ns app.core (:require-macros [tools :refer [twice]])) (def calculate (fn [x] (twice (#(+ % 2) x))))",
    );
    write("api.wit", WIT);
    write(
        "deps.sus",
        "{:worlds {:app {:namespace app.core :wit \"api.wit\" :exports {\"api#calculate\" app.core/calculate} :output \"out/app.wasm\"}}}",
    );
}
fn compile(root: &Path, build: Option<&Path>, flags: &[&str]) -> Output {
    let mut args = vec!["compile", "--config", "project/deps.sus"];
    args.extend(flags);
    suss(root, build, &args)
}
fn output(root: &Path) -> Vec<u8> {
    std::fs::read(root.join("project/out/app.wasm")).unwrap()
}
fn calculate(root: &Path, build: &Path, x: &str) -> String {
    let output = suss(
        root,
        Some(build),
        &["run", "project/out/app.wasm", "--invoke", "api#calculate", "--", x],
    );
    success(&output);
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

#[test]
fn warm_processes_reuse_native_code_instead_of_compiling() {
    let root = tempfile::tempdir().unwrap();
    let build = root.path().join("build");
    let cold = suss(root.path(), Some(&build), &["-e", "(+ 1 2)"]);
    success(&cold);
    assert!(count(&cold, "native-module store") > 0, "{:?}", events(&cold));
    assert_eq!(count(&cold, "native-module hit"), 0);
    let warm = suss(root.path(), Some(&build), &["-e", "(+ 1 2)"]);
    success(&warm);
    assert_eq!(warm.stdout, cold.stdout);
    assert_eq!(count(&warm, "native-module hit"), count(&cold, "native-module store"));
    assert_eq!(count(&warm, "native-module miss"), 0, "{:?}", events(&warm));
    // Bypass reads and writes nothing.
    let bypass = root.path().join("bypass");
    let disabled = suss(root.path(), Some(&bypass), &["--no-cache", "-e", "(+ 1 2)"]);
    success(&disabled);
    assert_eq!(disabled.stdout, cold.stdout);
    assert!(events(&disabled).is_empty() && !bypass.exists());
}

#[test]
fn warm_builds_reuse_identical_outputs_until_an_input_changes() {
    let root = tempfile::tempdir().unwrap();
    let build = seeded(root.path());
    project(root.path());
    let cold = compile(root.path(), Some(&build), &[]);
    success(&cold);
    assert_eq!(count(&cold, "build-manifest miss"), 1, "{:?}", events(&cold));
    assert_eq!(count(&cold, "build-manifest store"), 1);
    let first = output(root.path());
    assert_eq!(calculate(root.path(), &build, "5"), "14");

    // Unchanged inputs: the whole build is reused and no compiler runs.
    std::fs::remove_file(root.path().join("project/out/app.wasm")).unwrap();
    let warm = compile(root.path(), Some(&build), &[]);
    success(&warm);
    assert_eq!(count(&warm, "build-manifest hit"), 1, "{:?}", events(&warm));
    assert_eq!(count(&warm, "native-module miss") + count(&warm, "native-module hit"), 0);
    assert_eq!(output(root.path()), first);
    assert_eq!(warm.stdout, cold.stdout);
    // The reused output equals an uncached build's.
    success(&suss(root.path(), None, &["--no-cache", "compile", "--config", "project/deps.sus"]));
    assert_eq!(output(root.path()), first);

    // A changed macro namespace invalidates its user, whose source is unchanged.
    std::fs::write(root.path().join("project/src/tools.cljs"), tools(100)).unwrap();
    let macro_change = compile(root.path(), Some(&build), &[]);
    success(&macro_change);
    assert_eq!(count(&macro_change, "build-manifest stale"), 1, "{:?}", events(&macro_change));
    assert_eq!(calculate(root.path(), &build, "5"), "114");

    // The entry source.
    std::fs::write(
        root.path().join("project/src/app/core.cljc"),
        "(ns app.core (:require-macros [tools :refer [twice]])) (def calculate (fn [x] (twice x)))",
    )
    .unwrap();
    let source_change = compile(root.path(), Some(&build), &[]);
    success(&source_change);
    assert_eq!(count(&source_change, "build-manifest stale"), 1);
    assert_eq!(calculate(root.path(), &build, "5"), "110");

    // The WIT package.
    std::fs::write(
        root.path().join("project/api.wit"),
        WIT.replace("world app {", "world app { export extra: func() -> u32;"),
    )
    .unwrap();
    let wit_change = compile(root.path(), Some(&build), &[]);
    assert_eq!(count(&wit_change, "build-manifest stale"), 1, "{:?}", events(&wit_change));
    std::fs::write(root.path().join("project/api.wit"), WIT).unwrap();

    // A new file that could shadow a probed namespace path.
    std::fs::write(root.path().join("project/src/tools.sus"), tools(0)).unwrap();
    let shadowed = compile(root.path(), Some(&build), &[]);
    assert!(!shadowed.status.success(), "two sources now provide tools");
    assert_eq!(count(&shadowed, "build-manifest stale"), 1);
    std::fs::remove_file(root.path().join("project/src/tools.sus")).unwrap();

    // Options are part of the key: a different budget never reuses a build.
    let warm_again = compile(root.path(), Some(&build), &[]);
    assert_eq!(count(&warm_again, "build-manifest hit"), 1);
    let exhausted = compile(root.path(), Some(&build), &["--compile-fuel", "0"]);
    assert!(!exhausted.status.success());
    assert_eq!(count(&exhausted, "build-manifest miss"), 1);
    assert!(String::from_utf8_lossy(&exhausted.stderr).contains("fuel"));
    assert_eq!(calculate(root.path(), &build, "5"), "110", "failure kept the prior output");
}

#[test]
fn a_failed_build_keeps_its_completed_units_for_the_next_build() {
    let root = tempfile::tempdir().unwrap();
    let build = seeded(root.path());
    project(root.path());
    std::fs::create_dir_all(root.path().join("project/out")).unwrap();
    std::fs::write(root.path().join("project/out/app.wasm"), b"prior output").unwrap();
    // The macro namespace and its runtime dependency compile; the entry fails.
    std::fs::write(
        root.path().join("project/src/app/core.cljc"),
        "(ns app.core (:require-macros [tools :refer [twice]])) (def calculate (fn [x] (twice (#(+ % 2) x)))) (undefined-function)",
    )
    .unwrap();
    let failed = compile(root.path(), Some(&build), &[]);
    assert!(!failed.status.success());
    assert!(count(&failed, "native-module store") > 0, "{:?}", events(&failed));
    assert!(count(&failed, "fragment store") > 0, "{:?}", events(&failed));
    assert_eq!(count(&failed, "build-manifest store"), 0, "failures are never recorded");
    assert_eq!(output(root.path()), b"prior output");

    project(root.path());
    let resumed = compile(root.path(), Some(&build), &[]);
    success(&resumed);
    assert!(count(&resumed, "native-module hit") >= count(&failed, "native-module store"));
    assert!(count(&resumed, "fragment hit") > 0, "{:?}", events(&resumed));
    assert_eq!(calculate(root.path(), &build, "5"), "14");
}

#[test]
fn corrupt_and_untrusted_entries_are_misses_that_rebuild() {
    let root = tempfile::tempdir().unwrap();
    let build = seeded(root.path());
    project(root.path());
    success(&compile(root.path(), Some(&build), &[]));
    let expected = output(root.path());
    let mut entries = Vec::new();
    fn files(path: &Path, into: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(path).unwrap().flatten() {
            if entry.file_type().unwrap().is_dir() {
                files(&entry.path(), into);
            } else {
                into.push(entry.path());
            }
        }
    }
    files(&build.join("cache"), &mut entries);
    for (index, entry) in entries.iter().enumerate() {
        let mut bytes = std::fs::read(entry).unwrap();
        match index % 3 {
            0 => bytes.truncate(bytes.len() / 2),
            1 => *bytes.last_mut().unwrap() ^= 1,
            _ => bytes[3] ^= 1,
        }
        std::fs::write(entry, bytes).unwrap();
    }
    let recovered = compile(root.path(), Some(&build), &[]);
    success(&recovered);
    assert!(count(&recovered, "build-manifest reject") == 1, "{:?}", events(&recovered));
    assert!(count(&recovered, "native-module reject") > 0);
    assert_eq!(output(root.path()), expected);
    let warm = compile(root.path(), Some(&build), &[]);
    assert_eq!(count(&warm, "build-manifest hit"), 1, "{:?}", events(&warm));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Native code another user could have written is never loaded.
        let natives = build.join("cache/v1/native-module");
        let mode = std::fs::metadata(&natives).unwrap().permissions().mode();
        std::fs::set_permissions(&natives, std::fs::Permissions::from_mode(mode | 0o002)).unwrap();
        let untrusted = suss(root.path(), Some(&build), &["-e", "(+ 1 2)"]);
        success(&untrusted);
        assert_eq!(count(&untrusted, "native-module hit"), 0);
        assert!(String::from_utf8_lossy(&untrusted.stderr).contains("writable by other users"));
        std::fs::set_permissions(&natives, std::fs::Permissions::from_mode(mode)).unwrap();
    }
}

#[test]
fn concurrent_writers_publish_usable_entries() {
    let root = tempfile::tempdir().unwrap();
    let build = seeded(root.path());
    project(root.path());
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let root = root.path().to_owned();
            let build = build.clone();
            std::thread::spawn(move || compile(&root, Some(&build), &[]))
        })
        .collect();
    for handle in handles {
        success(&handle.join().unwrap());
    }
    let warm = compile(root.path(), Some(&build), &[]);
    assert_eq!(count(&warm, "build-manifest hit"), 1, "{:?}", events(&warm));
    assert_eq!(calculate(root.path(), &build, "5"), "14");
    let leftovers = std::fs::read_dir(build.join("tmp")).unwrap().count();
    assert_eq!(leftovers, 0, "completed writers leave no temporaries");
}

#[test]
fn build_directory_precedence_and_clean() {
    let root = tempfile::tempdir().unwrap();
    project(root.path());
    let project_dir = root.path().join("project");
    // Default: target/suss beside deps.sus, found from a nested directory.
    let nested = project_dir.join("src/app");
    let default = suss(&nested, None, &["cache", "info"]);
    success(&default);
    assert!(
        String::from_utf8_lossy(&default.stdout)
            .contains(&project_dir.join("target/suss").display().to_string()),
        "{}",
        String::from_utf8_lossy(&default.stdout)
    );
    // The environment, then the flag, override it; --no-cache disables.
    let environment = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(&nested)
        .env("SUSS_BUILD_DIR", root.path().join("from-env"))
        .args(["cache", "info"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&environment.stdout).contains("from-env"));
    let flag = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(&nested)
        .env("SUSS_BUILD_DIR", root.path().join("from-env"))
        .args(["--build-dir", "from-flag", "cache", "info"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&flag.stdout).contains("from-flag"));
    let disabled = suss(&nested, None, &["--no-cache", "cache", "info"]);
    assert!(String::from_utf8_lossy(&disabled.stdout).contains("disabled"));

    // A project's :build-dir applies to its compilation.
    std::fs::write(
        project_dir.join("deps.sus"),
        "{:build-dir \"cache-here\" :worlds {:app {:namespace app.core :wit \"api.wit\" :exports {\"api#calculate\" app.core/calculate} :output \"out/app.wasm\"}}}",
    )
    .unwrap();
    let seed = seeded(root.path());
    copy_tree(&seed, &project_dir.join("cache-here"));
    let configured = compile(root.path(), None, &[]);
    success(&configured);
    assert!(project_dir.join("cache-here/cache/v1/build-manifest").exists());
    assert!(!project_dir.join("target").exists());

    // Clean removes cached state only, and refuses untagged directories.
    let build = project_dir.join("cache-here");
    let cleaned = suss(root.path(), Some(&build), &["cache", "clean"]);
    success(&cleaned);
    assert!(!build.join("cache").exists());
    assert!(build.join("CACHEDIR.TAG").exists());
    assert!(project_dir.join("src/app/core.cljc").exists());
    let refused = suss(root.path(), Some(&project_dir.join("src")), &["cache", "clean"]);
    assert!(!refused.status.success());
    assert!(project_dir.join("src/app/core.cljc").exists());
}
