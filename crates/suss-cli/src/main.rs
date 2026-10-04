//! Suss CLI - Command-line interface for the Suss language
//!
//! Uses WASM compilation + wasmtime for all expression evaluation.

mod args;
// Retained prototype regression fixtures; the shipped native REPL uses Session.
#[cfg(test)]
mod completer;
#[cfg(test)]
mod repl;
#[cfg(test)]
mod session;

#[cfg(all(feature = "component", target_family = "wasm"))]
mod component;

use rustyline::{error::ReadlineError, DefaultEditor};

fn main() {
    match args::parse_args() {
        Ok(cmd) => run_command(cmd),
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn run_command(cmd: args::Command) {
    match cmd {
        args::Command::Repl => {
            #[cfg(all(feature = "component", target_family = "wasm"))]
            component::run_repl();
            #[cfg(not(all(feature = "component", target_family = "wasm")))]
            run_repl();
        }
        args::Command::Eval { expr } => {
            #[cfg(all(feature = "component", target_family = "wasm"))]
            component::run_eval(&expr);
            #[cfg(not(all(feature = "component", target_family = "wasm")))]
            run_eval(&expr);
        }
        args::Command::RunFile { path } => {
            #[cfg(all(feature = "component", target_family = "wasm"))]
            component::run_file(&path);
            #[cfg(not(all(feature = "component", target_family = "wasm")))]
            run_file(&path);
        }
        args::Command::CompileFile { source, world_wit, wit_world, src_paths, exports, output, optimize } => {
            compile_source(Some(&source), None, &world_wit, wit_world.as_deref(), &src_paths, &exports, &output, optimize);
        }
        args::Command::CompileMain {
            source,
            namespace,
            src_paths,
            output,
            optimize,
        } => {
            compile_main(&source, &namespace, &src_paths, &output, optimize);
        }
        args::Command::CompileNamespace { namespace, src_paths, world_wit, wit_world, exports, output, optimize } => {
            compile_source(None, Some(&namespace), &world_wit, wit_world.as_deref(), &src_paths, &exports, &output, optimize);
        }
        args::Command::CompileProject { world, config_path, optimize } => {
            compile_project(world.as_deref(), config_path.as_deref(), optimize);
        }
        args::Command::Run { component_path, invoke, command_mode, args } => {
            run_component(&component_path, &invoke, command_mode, &args);
        }
        args::Command::Help => args::print_help(),
        args::Command::Version => println!("suss {}", env!("CARGO_PKG_VERSION")),
    }
}

/// Evaluate a single expression using WASM compilation and wasmtime
#[cfg(not(all(feature = "component", target_family = "wasm")))]
fn run_eval(expr: &str) {
    match run_eval_compiled(expr, None) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

/// Execute native source with the same isolated compiled phases as the REPL.
#[cfg(not(all(feature = "component", target_family = "wasm")))]
fn run_eval_compiled(source: &str, path: Option<std::path::PathBuf>) -> Result<(), String> {
    use suss_cli::{portable_macros::CompiledMacros, portable_repl, portable_session::Session};
    let mut runtime = Session::new_repl().map_err(|error| error.to_string())?;
    let mut macros = CompiledMacros::new().map_err(|error| error.to_string())?;
    let result = portable_repl::evaluate_script_compiled(&mut runtime, &mut macros, source, path)
        .map_err(|error| portable_repl::error_display(&mut runtime, &error))?;
    println!("{result}");
    Ok(())
}

/// Run a Suss file using WASM compilation
#[cfg(not(all(feature = "component", target_family = "wasm")))]
fn run_file(path: &str) {
    let contents = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file '{}': {}", path, e);
            std::process::exit(1);
        }
    };

    // Each source form imports the same persistent runtime and compiled macro phase.
    if let Err(e) = run_eval_compiled(&contents, Some(std::path::PathBuf::from(path))) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

/// Run wasm-opt on a compiled WASM file for size/performance optimization
fn run_wasm_opt(path: &str) -> Result<(), String> {
    let original_size = std::fs::metadata(path).map(|m| m.len())
        .map_err(|e| format!("{}", e))?;

    let status = std::process::Command::new("wasm-opt")
        .args(["-O3", "--enable-gc", "--enable-reference-types",
               "--enable-multivalue", "--enable-bulk-memory",
               "--enable-tail-call", "-o", path, path])
        .status();

    match status {
        Ok(s) if s.success() => {
            let opt_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            let pct = if original_size > 0 {
                100.0 * (1.0 - opt_size as f64 / original_size as f64)
            } else { 0.0 };
            println!("Optimized: {} bytes -> {} bytes ({:.0}% smaller)",
                     original_size, opt_size, pct);
            Ok(())
        }
        Ok(s) => Err(format!("wasm-opt exited with status: {}", s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err("wasm-opt not found. Install: brew install binaryen".into())
        }
        Err(e) => Err(format!("Failed to run wasm-opt: {}", e)),
    }
}

/// Compile native file or namespace input through the shared portable pipeline.
fn compile_source(
    source_path: Option<&str>,
    namespace: Option<&str>,
    wit_path: &str,
    wit_world: Option<&str>,
    source_paths: &[String],
    exports: &[String],
    output_path: &str,
    optimize: bool,
) {
    #[cfg(not(target_family = "wasm"))]
    let result = (|| -> Result<Vec<u8>, String> {
        use std::path::{Path, PathBuf};
        use suss_reader::Symbol;
        let mappings = exports
            .iter()
            .map(|mapping| {
                let (path, var) = mapping.split_once('=').ok_or_else(|| {
                    format!("Invalid export mapping {mapping:?}; expected WIT-PATH=SUSS-VAR")
                })?;
                if path.is_empty()
                    || var.is_empty()
                    || var.contains('=')
                    || var.chars().any(char::is_whitespace)
                    || (var != "/" && var.split('/').any(str::is_empty))
                    || var.matches('/').count() > 1
                {
                    return Err(format!(
                        "Invalid export mapping {mapping:?}; expected WIT-PATH=SUSS-VAR"
                    ));
                }
                Ok((path.to_owned(), Symbol::parse(var)))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let source_paths = source_paths.iter().map(PathBuf::from).collect::<Vec<_>>();
        if let Some(namespace) = namespace {
            suss_cli::portable_aot::compile_namespace(
                namespace,
                Path::new(wit_path),
                wit_world,
                &source_paths,
                &mappings,
            )
        } else {
            suss_cli::portable_aot::compile_file(
                Path::new(source_path.expect("file input")),
                Path::new(wit_path),
                wit_world,
                &source_paths,
                &mappings,
            )
        }
    })();
    #[cfg(target_family = "wasm")]
    let result: Result<Vec<u8>, String> = Err(
        "Portable file/namespace compilation in the component-host target remains unimplemented"
            .into(),
    );
    match result {
        Ok(wasm) => {
            if let Err(error) = std::fs::write(output_path, &wasm) {
                eprintln!("Error writing output file '{output_path}': {error}");
                std::process::exit(1);
            }
            println!(
                "Compiled {} -> {output_path} ({} bytes)",
                namespace.or(source_path).expect("source input"),
                wasm.len()
            );
            if optimize {
                if let Err(error) = run_wasm_opt(output_path) {
                    eprintln!("Optimization error: {error}");
                    std::process::exit(1);
                }
            }
        }
        Err(error) => {
            eprintln!("Compilation error: {error}");
            std::process::exit(1);
        }
    }
}

/// Compile a Suss file with -main function to a CLI command component
fn compile_main(
    source_path: &str,
    namespace: &str,
    source_paths: &[String],
    output_path: &str,
    optimize: bool,
) {
    #[cfg(not(target_family = "wasm"))]
    let result = suss_cli::portable_aot::compile_main(
        std::path::Path::new(source_path),
        namespace,
        &source_paths
            .iter()
            .map(std::path::PathBuf::from)
            .collect::<Vec<_>>(),
    );
    #[cfg(target_family = "wasm")]
    let result: Result<Vec<u8>, String> = Err(
        "Portable official command compilation in the component-host target remains unimplemented"
            .into(),
    );
    match result {
        Ok(wasm) => {
            if let Err(e) = std::fs::write(output_path, &wasm) {
                eprintln!("Error writing output file '{}': {}", output_path, e);
                std::process::exit(1);
            }
            println!(
                "Compiled {} -> {} ({} bytes)",
                source_path,
                output_path,
                wasm.len()
            );
            println!("Run with: suss run {}", output_path);
            if optimize {
                if let Err(e) = run_wasm_opt(output_path) {
                    eprintln!("Optimization error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Compilation error: {}", e);
            std::process::exit(1);
        }
    }
}

/// Compile a project from deps.sus configuration
fn compile_project(world: Option<&str>, config_path: Option<&str>, optimize: bool) {
    use std::path::Path;

    let config_path = config_path.unwrap_or("deps.sus");

    // Load configuration
    let config = match suss_compile::SussConfig::load(Path::new(config_path)) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Error loading config '{}': {}", config_path, e);
            std::process::exit(1);
        }
    };

    #[cfg(not(target_family = "wasm"))]
    let result = suss_cli::portable_project::compile_project(&config, world);
    #[cfg(target_family = "wasm")]
    let result: Result<std::collections::BTreeMap<String, Vec<u8>>, String> = Err(
        "Portable project compilation in the component-host target remains unimplemented".into(),
    );
    match result {
        Ok(results) => {
            for (world_name, wasm) in &results {
                // Get the output path from config
                match config.output_path(world_name) {
                    Ok(output_path) => {
                        // Create parent directories if needed
                        if let Some(parent) = output_path.parent() {
                            if !parent.exists() {
                                if let Err(e) = std::fs::create_dir_all(parent) {
                                    eprintln!("Error creating directory '{}': {}", parent.display(), e);
                                    std::process::exit(1);
                                }
                            }
                        }

                        if let Err(e) = std::fs::write(&output_path, wasm) {
                            eprintln!("Error writing '{}': {}", output_path.display(), e);
                            std::process::exit(1);
                        }
                        println!("Compiled {} -> {} ({} bytes)",
                                 world_name, output_path.display(), wasm.len());
                        if optimize {
                            if let Err(e) = run_wasm_opt(&output_path.display().to_string()) {
                                eprintln!("Optimization error: {}", e);
                                std::process::exit(1);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }

            if results.is_empty() {
                println!("No worlds to compile");
            } else {
                println!("\nSuccessfully compiled {} world(s)", results.len());
            }
        }
        Err(e) => {
            eprintln!("Compilation error: {}", e);
            std::process::exit(1);
        }
    }
}

/// Run a compiled WASM component with WASI support
#[cfg(not(all(feature = "component", target_family = "wasm")))]
fn run_component(path: &str, invoke: &str, command_mode: bool, args: &[String]) {
    match run_component_impl(path, invoke, command_mode, args) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

/// Implementation of component runner with wasmtime
#[cfg(not(all(feature = "component", target_family = "wasm")))]
fn run_component_impl(
    path: &str,
    invoke: &str,
    command_mode: bool,
    args: &[String],
) -> Result<(), String> {
    use wasmtime::component::{Component, Linker, ResourceTable, Val};
    use wasmtime::{Config, Engine, Store};
    use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

    // State for the component store - implements WasiView
    struct ComponentState {
        wasi: WasiCtx,
        table: ResourceTable,
    }

    impl WasiView for ComponentState {
        fn ctx(&mut self) -> WasiCtxView<'_> {
            WasiCtxView {
                ctx: &mut self.wasi,
                table: &mut self.table,
            }
        }
    }

    // Enable component model and GC
    let mut config = Config::new();
    config.wasm_component_model(true);
    config.wasm_component_model_implements(true);
    config.wasm_component_model_async(true);
    config.wasm_gc(true);
    config.wasm_function_references(true);
    config.wasm_tail_call(true);
    config.wasm_exceptions(true);
    let engine = Engine::new(&config).map_err(|e| format!("Failed to create engine: {}", e))?;

    // Load component from file
    let component = Component::from_file(&engine, path)
        .map_err(|e| format!("Failed to load component '{}': {}", path, e))?;

    let official_command = component
        .get_export_index(None, "wasi:cli/run@0.3.1")
        .is_some()
        && (command_mode
            || invoke == "wasi:cli/run@0.3.1#run"
            || (invoke == "run" && component.get_export_index(None, "run").is_none()));
    let invoke = if official_command {
        "wasi:cli/run@0.3.1#run"
    } else {
        invoke
    };

    // Reject invalid selection and typed inputs before any guest initializer
    // can run. Component export types are available without a Store/instance.
    // Resolve the exact public path, including versioned interface instances.
    let (item, export) = if let Some((interface, function)) = invoke.split_once('#') {
        let interface = component
            .get_export_index(None, interface)
            .ok_or_else(|| format!("Interface in {invoke:?} not found in component"))?;
        component.get_export(Some(&interface), function)
    } else {
        component.get_export(None, invoke)
    }
    .ok_or_else(|| format!("Function {invoke:?} not found in component"))?;
    let wasmtime::component::types::ComponentItem::ComponentFunc(func_ty) = item else {
        return Err(format!("Export {invoke:?} is not a function"));
    };
    let asynchronous = func_ty.async_();
    let parameters = func_ty.params().collect::<Vec<_>>();
    if official_command {
        let results = func_ty.results().collect::<Vec<_>>();
        let valid_result = matches!(results.as_slice(), [wasmtime::component::Type::Result(result)]
            if result.ok().is_none() && result.err().is_none());
        if !asynchronous || !parameters.is_empty() || !valid_result {
            return Err("Official command requires async func() -> result".into());
        }
    }
    let supplied = if command_mode || official_command {
        &[][..]
    } else {
        args
    };
    if parameters.len() != supplied.len() {
        return Err(format!(
            "Function {invoke:?} expects {} arguments, got {}",
            parameters.len(),
            supplied.len()
        ));
    }
    let func_args = parameters
        .iter()
        .zip(supplied)
        .map(|((name, ty), value)| {
            parse_component_argument(ty, value)
                .map_err(|error| format!("Parameter {name:?}: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Create WASI context
    // Default command invocation supplies WASI argv. Explicit invocation
    // parses parameters against the selected component function type.
    let mut wasi_builder = WasiCtxBuilder::new();
    wasi_builder.inherit_stdio();
    wasi_builder.inherit_env();
    if command_mode || official_command {
        // Prepend the component path as argv[0], then the user args
        let mut argv: Vec<String> = vec![path.to_string()];
        argv.extend(args.iter().cloned());
        wasi_builder.args(&argv);
    } else {
        wasi_builder.inherit_args();
    }

    let state = ComponentState {
        wasi: wasi_builder.build(),
        table: ResourceTable::new(),
    };

    // Create store with WASI state
    let mut store = Store::new(&engine, state);

    // Create linker and add WASI
    let mut linker: Linker<ComponentState> = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
        .map_err(|e| format!("Failed to add WASI to linker: {}", e))?;
    wasmtime_wasi::p3::add_to_linker(&mut linker)
        .map_err(|e| format!("Failed to add asynchronous WASI to linker: {e}"))?;

    // Instantiate component
    let runtime = if asynchronous {
        Some(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("Failed to create asynchronous component runtime: {e}"))?,
        )
    } else {
        None
    };
    let instance = if let Some(runtime) = &runtime {
        runtime.block_on(linker.instantiate_async(&mut store, &component))
    } else {
        linker.instantiate(&mut store, &component)
    }
    .map_err(|e| {
        if official_command {
            if let Some(exit) = e.downcast_ref::<wasmtime_wasi::I32Exit>() {
                std::process::exit(exit.0);
            }
        }
        format!("Failed to instantiate component: {e}")
    })?;

    let func = instance
        .get_func(&mut store, &export)
        .ok_or_else(|| format!("Export {invoke:?} is not a function"))?;

    // Prepare results buffer based on function type
    let results_len = func_ty.results().len();
    let mut results = vec![Val::S32(0); results_len];

    // Call the function
    if let Some(runtime) = &runtime {
        runtime.block_on(func.call_async(&mut store, &func_args, &mut results))
    } else {
        func.call(&mut store, &func_args, &mut results)
    }
    .map_err(|e| {
        if official_command {
            if let Some(exit) = e.downcast_ref::<wasmtime_wasi::I32Exit>() {
                std::process::exit(exit.0);
            }
        }
        format!("Failed to call '{invoke}': {e}")
    })?;

    if official_command {
        return match results.as_slice() {
            [Val::Result(Ok(None))] => Ok(()),
            [Val::Result(Err(None))] => Err("Command completed with failure".into()),
            _ => Err("Official command returned an incompatible completion result".into()),
        };
    }

    // Print result
    if results.is_empty() {
        // No return value
    } else if results.len() == 1 {
        match &results[0] {
            Val::S8(v) => println!("{}", v),
            Val::U8(v) => println!("{}", v),
            Val::S16(v) => println!("{}", v),
            Val::U16(v) => println!("{}", v),
            Val::S32(v) => println!("{}", v),
            Val::S64(v) => println!("{}", v),
            Val::U32(v) => println!("{}", v),
            Val::U64(v) => println!("{}", v),
            Val::Float32(v) => println!("{}", v),
            Val::Float64(v) => println!("{}", v),
            Val::Bool(v) => println!("{}", v),
            Val::Char(v) => println!("{}", v),
            Val::String(v) => println!("{}", v),
            _ => println!("{:?}", results[0]),
        }
    } else {
        // Multiple results - print as tuple
        let parts: Vec<String> = results.iter().map(|v| format!("{:?}", v)).collect();
        println!("({})", parts.join(", "));
    }

    Ok(())
}

/// Parse against the component's declared parameter type, with no fallback value.
#[cfg(not(all(feature = "component", target_family = "wasm")))]
fn parse_component_argument(
    ty: &wasmtime::component::Type,
    value: &str,
) -> Result<wasmtime::component::Val, String> {
    use wasmtime::component::{Type, Val};
    macro_rules! parsed {
        ($ty:ty, $variant:ident, $name:literal) => {
            value
                .parse::<$ty>()
                .map(Val::$variant)
                .map_err(|_| format!("Invalid {} argument {value:?}", $name))
        };
    }
    match ty {
        Type::Bool => parsed!(bool, Bool, "bool"),
        Type::S8 => parsed!(i8, S8, "s8"),
        Type::U8 => parsed!(u8, U8, "u8"),
        Type::S16 => parsed!(i16, S16, "s16"),
        Type::U16 => parsed!(u16, U16, "u16"),
        Type::S32 => parsed!(i32, S32, "s32"),
        Type::U32 => parsed!(u32, U32, "u32"),
        Type::S64 => parsed!(i64, S64, "s64"),
        Type::U64 => parsed!(u64, U64, "u64"),
        Type::Float32 => parsed!(f32, Float32, "f32"),
        Type::Float64 => parsed!(f64, Float64, "f64"),
        Type::String => Ok(Val::String(value.to_owned())),
        Type::Char => {
            let mut chars = value.chars();
            match (chars.next(), chars.next()) {
                (Some(character), None) => Ok(Val::Char(character)),
                _ => Err(format!(
                    "Invalid char argument {value:?}; expected one Unicode scalar"
                )),
            }
        }
        _ => {
            Err("This component parameter type is not supported by command-line invocation".into())
        }
    }
}

/// Run each input in the same Store using independently compiled fragments.
#[cfg(not(all(feature = "component", target_family = "wasm")))]
fn run_repl() {
    use std::io::{self, BufRead, IsTerminal};
    use suss_cli::{portable_macros::CompiledMacros, portable_repl, portable_session::Session};

    let mut session = match Session::new_repl() {
        Ok(session) => session,
        Err(error) => {
            eprintln!("Error: {error}");
            return;
        }
    };
    let mut macros = match CompiledMacros::new() {
        Ok(macros) => macros,
        Err(error) => {
            eprintln!("Error: {error}");
            return;
        }
    };
    let mut display = portable_repl::NativeDisplay::default();
    let interactive = io::stdin().is_terminal();
    let mut editor = if interactive {
        println!("Suss v{} - compiled REPL", env!("CARGO_PKG_VERSION"));
        println!("Type :quit to exit, :reset to reset the session.");
        match DefaultEditor::new() {
            Ok(editor) => Some(editor),
            Err(error) => {
                eprintln!("Error: {error}");
                return;
            }
        }
    } else {
        None
    };
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    let mut input = String::new();
    loop {
        let line = if let Some(editor) = editor.as_mut() {
            let prompt = if input.is_empty() {
                format!("{}=> ", session.current_namespace())
            } else {
                "...=> ".into()
            };
            match editor.readline(&prompt) {
                Ok(line) => {
                    let _ = editor.add_history_entry(&line);
                    line
                }
                Err(ReadlineError::Interrupted) => {
                    input.clear();
                    println!("^C");
                    continue;
                }
                Err(ReadlineError::Eof) => break,
                Err(error) => {
                    eprintln!("Error: {error}");
                    break;
                }
            }
        } else {
            match lines.next() {
                Some(Ok(line)) => line,
                Some(Err(error)) => {
                    eprintln!("Error: {error}");
                    break;
                }
                None => break,
            }
        };
        if input.is_empty() {
            match line.trim() {
                "" => continue,
                ":quit" => break,
                ":reset" => {
                    match portable_repl::reset_compiled(&mut session, &mut macros) {
                        Ok(()) => { display.clear(); println!("nil"); },
                        Err(error) => eprintln!("Error: {error}"),
                    };
                    continue;
                }
                _ => {}
            }
        }
        if input.is_empty() {
            let mut words = line.split_whitespace();
            let command = words.next().unwrap_or("");
            if matches!(command, ":load" | ":reload" | ":reload-all" | ":in-ns") {
                let Some(namespace) = words.next().filter(|_| words.next().is_none()) else {
                    eprintln!("Error: {command} expects one namespace name");
                    continue;
                };
                let result = match command {
                    ":load" => session
                        .load_namespace_with_macros(namespace, &mut macros)
                        .map(|_| ()),
                    ":reload" => session
                        .reload_namespace_with_macros(namespace, false, &mut macros)
                        .map(|_| ()),
                    ":reload-all" => session
                        .reload_namespace_with_macros(namespace, true, &mut macros)
                        .map(|_| ()),
                    ":in-ns" => session.enter_namespace(namespace),
                    _ => unreachable!(),
                };
                match result {
                    Ok(()) => println!("nil"),
                    Err(error) => eprintln!(
                        "Error: {}",
                        portable_repl::error_display(&mut session, &error)
                    ),
                }
                continue;
            }
        }
        input.push_str(&line);
        input.push('\n');
        match suss_reader::forms::read_forms(&input) {
            Ok(forms) if forms.is_empty() => {
                input.clear();
                continue;
            }
            Err(error) if error.expected.iter().any(|hint| hint == "more input") => continue,
            _ => {}
        }
        match portable_repl::evaluate_compiled_with_display(&mut session, &mut macros, &input, &mut display) {
            Ok(text) => println!("{text}"),
            Err(error) => eprintln!(
                "Error: {}",
                portable_repl::error_display(&mut session, &error)
            ),
        }
        input.clear();
    }
    if !input.is_empty() {
        // Incomplete EOF is a located compilation failure, never a partial eval.
        if let Err(error) = portable_repl::evaluate_compiled_with_display(&mut session, &mut macros, &input, &mut display) {
            eprintln!(
                "Error: {}",
                portable_repl::error_display(&mut session, &error)
            );
        }
    }
}
