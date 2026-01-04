//! Suss CLI - Command-line interface for the Suss language
//!
//! Uses WASM compilation + wasmtime for all expression evaluation.

mod args;
mod repl;

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
        args::Command::CompileFile { source, world_wit, output } => {
            compile_file(&source, &world_wit, &output);
        }
        args::Command::CompileMain { source, namespace, output } => {
            compile_main(&source, &namespace, &output);
        }
        args::Command::CompileNamespace { namespace, src_paths, world_wit, output } => {
            compile_namespace(&namespace, &src_paths, &world_wit, &output);
        }
        args::Command::CompileProject { world, config_path } => {
            compile_project(world.as_deref(), config_path.as_deref());
        }
        args::Command::Run { component_path, invoke, args } => {
            run_component(&component_path, &invoke, &args);
        }
        args::Command::Help => args::print_help(),
        args::Command::Version => println!("suss {}", env!("CARGO_PKG_VERSION")),
    }
}

/// Evaluate a single expression using WASM compilation and wasmtime
#[cfg(not(all(feature = "component", target_family = "wasm")))]
fn run_eval(expr: &str) {
    match run_eval_wasm(expr) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

/// Run expression via WASM compilation + wasmtime
///
/// Automatically detects WASI calls and uses component model when needed.
fn run_eval_wasm(expr: &str) -> Result<(), String> {
    // Compile expression to WASM
    let mut compiler = suss_compile::Compiler::new();
    let compiled = compiler.compile_expr_with_info(expr)
        .map_err(|e| format!("{}", e))?;

    if compiled.is_component {
        // WASI expression - use component model runtime
        run_eval_component(&compiled.wasm)
    } else {
        // Pure expression - use core module runtime
        run_eval_core_module(&compiled.wasm)
    }
}

/// GC sentinel values for decoding i31refs
mod gc_sentinels {
    pub const NIL_SENTINEL: i32 = 0;
    pub const FALSE_SENTINEL: i32 = 2;
    pub const TRUE_SENTINEL: i32 = 4;

    pub fn decode_i31(raw: i32) -> i64 {
        (raw >> 1) as i64
    }
}

/// Run a compiled core module (no WASI)
fn run_eval_core_module(wasm_bytes: &[u8]) -> Result<(), String> {
    use wasmtime::{Config, Engine, Linker, Module, Store, Val};

    // Create wasmtime engine with GC and related features enabled
    let mut config = Config::new();
    config.wasm_gc(true);
    config.wasm_function_references(true);
    config.wasm_tail_call(true);
    let engine = Engine::new(&config)
        .map_err(|e| format!("Engine creation error: {}", e))?;

    let module = Module::new(&engine, wasm_bytes)
        .map_err(|e| format!("WASM module error: {}", e))?;

    // Create store (no WASI context needed for pure expressions)
    let mut store = Store::new(&engine, ());

    // Create linker
    let linker: Linker<()> = Linker::new(&engine);

    // Instantiate via linker
    let instance = linker.instantiate(&mut store, &module)
        .map_err(|e| format!("Instantiation error: {}", e))?;

    // Get the eval function
    let eval_fn = instance.get_func(&mut store, "eval")
        .ok_or_else(|| "eval function not found".to_string())?;

    // Call the function with eqref result
    let mut results = vec![Val::null_any_ref()];
    eval_fn.call(&mut store, &[], &mut results)
        .map_err(|e| format!("Call error: {}", e))?;

    // Print result based on GC type
    match &results[0] {
        Val::AnyRef(Some(anyref)) => {
            // Try to extract as i31
            match anyref.as_i31(&store) {
                Ok(Some(i31)) => {
                    let raw = i31.get_i32();
                    match raw {
                        gc_sentinels::NIL_SENTINEL => println!("nil"),
                        gc_sentinels::FALSE_SENTINEL => println!("false"),
                        gc_sentinels::TRUE_SENTINEL => println!("true"),
                        _ => {
                            // Small integer
                            let val = gc_sentinels::decode_i31(raw);
                            println!("{}", val);
                        }
                    }
                }
                Ok(None) => {
                    // Struct or array - try to extract as FLOAT struct
                    match anyref.as_struct(&store) {
                        Ok(Some(struct_ref)) => {
                            // GC type 1 is FLOAT (single f64 field)
                            // Try to read field 0 as f64
                            match struct_ref.field(&mut store, 0) {
                                Ok(wasmtime::Val::F64(bits)) => {
                                    let f = f64::from_bits(bits);
                                    if f.fract() == 0.0 {
                                        println!("{}.0", f as i64);
                                    } else {
                                        println!("{}", f);
                                    }
                                }
                                Ok(wasmtime::Val::I64(v)) => {
                                    // LARGE_INT struct (type 0)
                                    println!("{}", v);
                                }
                                _ => println!("<gc-struct>"),
                            }
                        }
                        Ok(None) => {
                            // Array - could be STRING (type 2)
                            println!("<gc-array>");
                        }
                        Err(_) => println!("<gc-object>"),
                    }
                }
                Err(e) => return Err(format!("Error extracting i31: {}", e)),
            }
        }
        Val::AnyRef(None) => println!("nil"),
        _ => println!("{:?}", results[0]),
    }

    Ok(())
}

/// Run a compiled component with WASI support (for expressions using wasi.*)
fn run_eval_component(wasm_bytes: &[u8]) -> Result<(), String> {
    use wasmtime::{Config, Engine, Store};
    use wasmtime::component::{Component, Linker, ResourceTable, Val};
    use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

    // State for the component store - implements WasiView
    struct EvalState {
        wasi: WasiCtx,
        table: ResourceTable,
    }

    impl WasiView for EvalState {
        fn ctx(&mut self) -> WasiCtxView<'_> {
            WasiCtxView {
                ctx: &mut self.wasi,
                table: &mut self.table,
            }
        }
    }

    // Enable component model
    let mut config = Config::new();
    config.wasm_component_model(true);
    let engine = Engine::new(&config)
        .map_err(|e| format!("Failed to create engine: {}", e))?;

    // Load component
    let component = Component::new(&engine, wasm_bytes)
        .map_err(|e| format!("Failed to load component: {}", e))?;

    // Create WASI context
    let state = EvalState {
        wasi: WasiCtxBuilder::new()
            .inherit_stdio()
            .inherit_env()
            .build(),
        table: ResourceTable::new(),
    };

    // Create store with WASI state
    let mut store = Store::new(&engine, state);

    // Create linker and add WASI
    let mut linker: Linker<EvalState> = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
        .map_err(|e| format!("Failed to add WASI to linker: {}", e))?;

    // Instantiate component
    let instance = linker.instantiate(&mut store, &component)
        .map_err(|e| format!("Failed to instantiate component: {}", e))?;

    // Get eval function
    let eval_fn = instance.get_func(&mut store, "eval")
        .ok_or_else(|| "eval function not found in component".to_string())?;

    // Call with no args
    let func_ty = eval_fn.ty(&store);
    let results_len = func_ty.results().len();
    let mut results = vec![Val::S32(0); results_len];

    eval_fn.call(&mut store, &[], &mut results)
        .map_err(|e| format!("Call error: {}", e))?;

    // Print result
    if results.is_empty() {
        println!("nil");
    } else if results.len() == 1 {
        match &results[0] {
            Val::S32(v) => println!("{}", v),
            Val::S64(v) => println!("{}", v),
            Val::U32(v) => println!("{}", v),
            Val::U64(v) => println!("{}", v),
            Val::Float32(v) => println!("{}", v),
            Val::Float64(v) => println!("{}", v),
            Val::Bool(v) => println!("{}", v),
            Val::Char(v) => println!("{}", v),
            Val::String(v) => println!("\"{}\"", v),
            _ => println!("{:?}", results[0]),
        }
    } else {
        let parts: Vec<String> = results.iter().map(|v| format!("{:?}", v)).collect();
        println!("({})", parts.join(", "));
    }

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

    // Compile entire file as a single expression
    // This allows deftype and other definitions to be visible to later expressions
    if let Err(e) = run_eval_wasm(&contents) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

/// Compile a Suss file to a WASM component (file mode)
fn compile_file(source_path: &str, wit_path: &str, output_path: &str) {
    let mut compiler = suss_compile::Compiler::new();

    match compiler.compile_files(source_path, wit_path) {
        Ok(wasm) => {
            if let Err(e) = std::fs::write(output_path, &wasm) {
                eprintln!("Error writing output file '{}': {}", output_path, e);
                std::process::exit(1);
            }
            println!("Compiled {} -> {} ({} bytes)", source_path, output_path, wasm.len());
        }
        Err(e) => {
            eprintln!("Compilation error: {}", e);
            std::process::exit(1);
        }
    }
}

/// Compile a Suss file with -main function to a CLI command component
fn compile_main(source_path: &str, namespace: &str, output_path: &str) {
    let source = match std::fs::read_to_string(source_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading file '{}': {}", source_path, e);
            std::process::exit(1);
        }
    };

    let mut compiler = suss_compile::Compiler::new();

    match compiler.compile_for_main(&source, namespace) {
        Ok(wasm) => {
            if let Err(e) = std::fs::write(output_path, &wasm) {
                eprintln!("Error writing output file '{}': {}", output_path, e);
                std::process::exit(1);
            }
            println!("Compiled {} -> {} ({} bytes)", source_path, output_path, wasm.len());
            println!("Run with: suss run {} --invoke run", output_path);
        }
        Err(e) => {
            eprintln!("Compilation error: {}", e);
            std::process::exit(1);
        }
    }
}

/// Compile from an entry namespace with multi-file support
fn compile_namespace(entry_ns: &str, src_paths: &[String], wit_path: &str, output_path: &str) {
    use std::path::PathBuf;

    let src_path_bufs: Vec<PathBuf> = src_paths.iter().map(PathBuf::from).collect();

    let mut compiler = suss_compile::Compiler::new();

    match compiler.compile_with_namespaces(entry_ns, &src_path_bufs, wit_path) {
        Ok(wasm) => {
            if let Err(e) = std::fs::write(output_path, &wasm) {
                eprintln!("Error writing output file '{}': {}", output_path, e);
                std::process::exit(1);
            }
            println!(
                "Compiled namespace {} -> {} ({} bytes)",
                entry_ns, output_path, wasm.len()
            );
        }
        Err(e) => {
            eprintln!("Compilation error: {}", e);
            std::process::exit(1);
        }
    }
}

/// Compile a project from deps.sus configuration
fn compile_project(world: Option<&str>, config_path: Option<&str>) {
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

    let mut compiler = suss_compile::Compiler::new();

    // Compile project
    match compiler.compile_project(&config, world) {
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
fn run_component(path: &str, invoke: &str, args: &[String]) {
    match run_component_impl(path, invoke, args) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

/// Implementation of component runner with wasmtime
#[cfg(not(all(feature = "component", target_family = "wasm")))]
fn run_component_impl(path: &str, invoke: &str, args: &[String]) -> Result<(), String> {
    use wasmtime::{Config, Engine, Store};
    use wasmtime::component::{Component, Linker, ResourceTable, Val};
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
    config.wasm_gc(true);
    let engine = Engine::new(&config)
        .map_err(|e| format!("Failed to create engine: {}", e))?;

    // Load component from file
    let component = Component::from_file(&engine, path)
        .map_err(|e| format!("Failed to load component '{}': {}", path, e))?;

    // Create WASI context
    let state = ComponentState {
        wasi: WasiCtxBuilder::new()
            .inherit_stdio()
            .inherit_env()
            .build(),
        table: ResourceTable::new(),
    };

    // Create store with WASI state
    let mut store = Store::new(&engine, state);

    // Create linker and add WASI
    let mut linker: Linker<ComponentState> = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
        .map_err(|e| format!("Failed to add WASI to linker: {}", e))?;

    // Instantiate component
    let instance = linker.instantiate(&mut store, &component)
        .map_err(|e| format!("Failed to instantiate component: {}", e))?;

    // Get exported function
    let func = instance.get_func(&mut store, invoke)
        .ok_or_else(|| format!("Function '{}' not found in component", invoke))?;

    // Parse arguments as i32 values (for now, simple integer args)
    let func_args: Vec<Val> = args.iter()
        .map(|s| {
            // Try parsing as i32, fall back to 0
            let v: i32 = s.parse().unwrap_or(0);
            Val::S32(v)
        })
        .collect();

    // Prepare results buffer based on function type
    let func_ty = func.ty(&store);
    let results_len = func_ty.results().len();
    let mut results = vec![Val::S32(0); results_len];

    // Call the function
    func.call(&mut store, &func_args, &mut results)
        .map_err(|e| format!("Failed to call '{}': {}", invoke, e))?;

    // Print result
    if results.is_empty() {
        // No return value
    } else if results.len() == 1 {
        match &results[0] {
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

/// Start the REPL with stateful evaluation
///
/// The REPL maintains state across expressions:
/// - Accumulated definitions (defn, def, deftype, etc.)
/// - Current namespace context
/// - Loaded namespaces from require
#[cfg(not(all(feature = "component", target_family = "wasm")))]
fn run_repl() {
    println!("Suss v{} - A Clojure dialect for WASM", env!("CARGO_PKG_VERSION"));
    println!("Type (help) for help, Ctrl-C to exit");
    println!();

    let mut state = repl::ReplState::new();
    let mut rl = match DefaultEditor::new() {
        Ok(editor) => editor,
        Err(e) => {
            eprintln!("Failed to initialize readline: {}", e);
            return;
        }
    };

    loop {
        // Dynamic prompt with current namespace
        let prompt = format!("{}=> ", state.current_ns);

        match rl.readline(&prompt) {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let _ = rl.add_history_entry(&line);

                // Handle special forms
                if trimmed.starts_with("(in-ns ") {
                    match repl::handle_in_ns(&mut state, trimmed) {
                        Ok(_ns) => println!("nil"),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                    continue;
                }

                if trimmed.starts_with("(require ") {
                    match repl::handle_require(&mut state, trimmed) {
                        Ok(_msg) => println!("nil"),
                        Err(e) => eprintln!("Error: {}", e),
                    }
                    continue;
                }

                // Check if this is a definition to accumulate
                if repl::is_definition(trimmed) {
                    state.accumulate_definition(trimmed);
                }

                // Build full source and evaluate
                let full_source = state.build_source(trimmed);
                match run_eval_wasm(&full_source) {
                    Ok(()) => {}
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
            }
            Err(ReadlineError::Eof) => {
                println!("Goodbye!");
                break;
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                break;
            }
        }
    }
}
