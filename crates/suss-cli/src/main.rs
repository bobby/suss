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
        args::Command::CompileFile { source, world_wit, output, optimize } => {
            compile_file(&source, &world_wit, &output, optimize);
        }
        args::Command::CompileMain { source, namespace, output, optimize } => {
            compile_main(&source, &namespace, &output, optimize);
        }
        args::Command::CompileNamespace { namespace, src_paths, world_wit, output, optimize } => {
            compile_namespace(&namespace, &src_paths, &world_wit, &output, optimize);
        }
        args::Command::CompileProject { world, config_path, optimize } => {
            compile_project(world.as_deref(), config_path.as_deref(), optimize);
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
    // Inject *ns* binding for consistency with REPL
    let full_expr = format!("(def *ns* 'user)\n{}", expr);

    // First check if expression uses WASI (component model)
    let mut compiler = suss_compile::Compiler::new();
    let probe = compiler.compile_expr_with_info(&full_expr)
        .map_err(|e| format!("{}", e))?;

    if probe.is_component {
        // WASI expression - use component model runtime (can't use pr-str with WIT types)
        run_eval_component(&probe.wasm)
    } else {
        // Try to wrap in pr-str for pretty printing
        // This fails if expr contains definitions (defn/def inside pr-str)
        let wrapped_expr = format!("(def *ns* 'user)\n(pr-str {})", expr);
        let mut compiler2 = suss_compile::Compiler::new();
        match compiler2.compile_expr_with_info(&wrapped_expr) {
            Ok(compiled) => {
                // pr-str compilation succeeded - print as string
                run_eval_core_module_string(&compiled.wasm)
            }
            Err(_) => {
                // pr-str failed (likely has definitions) - run directly
                run_eval_core_module(&probe.wasm)
            }
        }
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
    config.wasm_exceptions(true);
    let engine = Engine::new(&config)
        .map_err(|e| format!("Engine creation error: {}", e))?;

    let module = Module::new(&engine, wasm_bytes)
        .map_err(|e| format!("WASM module error: {}", e))?;

    // Create store (no WASI context needed for pure expressions)
    let mut store = Store::new(&engine, ());

    // Create linker with print_str support
    let mut linker: Linker<()> = Linker::new(&engine);
    linker.func_wrap("suss", "print_str", |mut caller: wasmtime::Caller<'_, ()>, ptr: i32, len: i32| {
        if let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) {
            let mut buf = vec![0u8; len as usize];
            if memory.read(&caller, ptr as usize, &mut buf).is_ok() {
                use std::io::Write;
                let _ = std::io::stdout().write_all(&buf);
                let _ = std::io::stdout().flush();
            }
        }
    }).map_err(|e| format!("Linker error: {}", e))?;

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

/// Run a compiled core module that returns a STRING array (from pr-str)
fn run_eval_core_module_string(wasm_bytes: &[u8]) -> Result<(), String> {
    use wasmtime::{Config, Engine, Linker, Module, Store, Val};

    // Create wasmtime engine with GC and related features enabled
    let mut config = Config::new();
    config.wasm_gc(true);
    config.wasm_function_references(true);
    config.wasm_tail_call(true);
    config.wasm_exceptions(true);
    let engine = Engine::new(&config)
        .map_err(|e| format!("Engine creation error: {}", e))?;

    let module = Module::new(&engine, wasm_bytes)
        .map_err(|e| format!("WASM module error: {}", e))?;

    // Create store (no WASI context needed for pure expressions)
    let mut store = Store::new(&engine, ());

    // Create linker with print_str support
    let mut linker: Linker<()> = Linker::new(&engine);
    linker.func_wrap("suss", "print_str", |mut caller: wasmtime::Caller<'_, ()>, ptr: i32, len: i32| {
        if let Some(memory) = caller.get_export("memory").and_then(|e| e.into_memory()) {
            let mut buf = vec![0u8; len as usize];
            if memory.read(&caller, ptr as usize, &mut buf).is_ok() {
                use std::io::Write;
                let _ = std::io::stdout().write_all(&buf);
                let _ = std::io::stdout().flush();
            }
        }
    }).map_err(|e| format!("Linker error: {}", e))?;

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

    // Result should be a STRING array (array<i8>)
    match &results[0] {
        Val::AnyRef(Some(anyref)) => {
            // Try to get as array
            match anyref.as_array(&store) {
                Ok(Some(array_ref)) => {
                    let len = array_ref.len(&store)
                        .map_err(|e| format!("Error getting array length: {}", e))?;
                    let mut bytes = Vec::with_capacity(len as usize);
                    for i in 0..len {
                        match array_ref.get(&mut store, i) {
                            Ok(Val::I32(b)) => bytes.push(b as u8),
                            _ => return Err("Invalid string array element".to_string()),
                        }
                    }
                    // Print as UTF-8 string
                    match String::from_utf8(bytes) {
                        Ok(s) => println!("{}", s),
                        Err(e) => return Err(format!("Invalid UTF-8: {}", e)),
                    }
                }
                Ok(None) => println!("nil"),
                Err(e) => return Err(format!("Error extracting array: {}", e)),
            }
        }
        Val::AnyRef(None) => println!("nil"),
        _ => return Err(format!("Unexpected result type: {:?}", results[0])),
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

/// Compile a Suss file to a WASM component (file mode)
fn compile_file(source_path: &str, wit_path: &str, output_path: &str, optimize: bool) {
    let mut compiler = suss_compile::Compiler::new();

    match compiler.compile_files(source_path, wit_path) {
        Ok(wasm) => {
            if let Err(e) = std::fs::write(output_path, &wasm) {
                eprintln!("Error writing output file '{}': {}", output_path, e);
                std::process::exit(1);
            }
            println!("Compiled {} -> {} ({} bytes)", source_path, output_path, wasm.len());
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

/// Compile a Suss file with -main function to a CLI command component
fn compile_main(source_path: &str, namespace: &str, output_path: &str, optimize: bool) {
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

/// Compile from an entry namespace with multi-file support
fn compile_namespace(entry_ns: &str, src_paths: &[String], wit_path: &str, output_path: &str, optimize: bool) {
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
    // For "run" (WASI CLI entry point), pass extra args as WASI argv
    // so the guest can read them via wasi:cli/environment#get-arguments.
    // For other functions, args are passed as function parameters instead.
    let is_run = invoke == "run";
    let mut wasi_builder = WasiCtxBuilder::new();
    wasi_builder.inherit_stdio();
    wasi_builder.inherit_env();
    if is_run && !args.is_empty() {
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

    // Instantiate component
    let instance = linker.instantiate(&mut store, &component)
        .map_err(|e| format!("Failed to instantiate component: {}", e))?;

    // Get exported function
    let func = instance.get_func(&mut store, invoke)
        .ok_or_else(|| format!("Function '{}' not found in component", invoke))?;

    // For "run", the WASI CLI entry point takes no parameters.
    // For other functions, parse args as i32 values.
    let func_args: Vec<Val> = if is_run {
        Vec::new()
    } else {
        args.iter()
            .map(|s| {
                let v: i32 = s.parse().unwrap_or(0);
                Val::S32(v)
            })
            .collect()
    };

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

/// Run each input in the same Store using independently compiled fragments.
#[cfg(not(all(feature = "component", target_family = "wasm")))]
fn run_repl() {
    use std::io::{self, BufRead, IsTerminal};
    use suss_cli::{portable_repl, portable_session::Session};

    let mut session = match Session::new_repl() {
        Ok(session) => session,
        Err(error) => {
            eprintln!("Error: {error}");
            return;
        }
    };
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
                    match session.reset() {
                        Ok(()) => println!("nil"),
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
                    ":load" => session.load_namespace(namespace).map(|_| ()),
                    ":reload" => session.reload_namespace(namespace, false).map(|_| ()),
                    ":reload-all" => session.reload_namespace(namespace, true).map(|_| ()),
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
        match session.eval(&input) {
            Ok(value) => match portable_repl::display(&mut session, &value) {
                Ok(text) => println!("{text}"),
                Err(error) => eprintln!("Display error: {error}"),
            },
            Err(error) => eprintln!(
                "Error: {}",
                portable_repl::error_display(&mut session, &error)
            ),
        }
        input.clear();
    }
    if !input.is_empty() {
        // Incomplete EOF is a located compilation failure, never a partial eval.
        if let Err(error) = session.eval(&input) {
            eprintln!(
                "Error: {}",
                portable_repl::error_display(&mut session, &error)
            );
        }
    }
}
