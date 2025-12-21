//! Suss CLI - Command-line interface for the Suss language
//!
//! The main entry point for the Suss language.

mod args;

#[cfg(all(feature = "component", target_family = "wasm"))]
mod component;

#[cfg(not(all(feature = "component", target_family = "wasm")))]
use suss_eval::Runtime;
#[cfg(not(all(feature = "component", target_family = "wasm")))]
use suss_reader::print_sexp;

#[cfg(not(target_family = "wasm"))]
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
        args::Command::Compile { source, world_wit, output } => {
            compile(&source, &world_wit, &output);
        }
        args::Command::Help => args::print_help(),
        args::Command::Version => println!("suss {}", env!("CARGO_PKG_VERSION")),
    }
}

/// Evaluate a single expression and print the result
#[cfg(not(all(feature = "component", target_family = "wasm")))]
fn run_eval(expr: &str) {
    let mut runtime = Runtime::new();
    match runtime.eval_string(expr) {
        Ok(result) => {
            println!("{}", print_sexp(&result, &runtime.interner));
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

/// Run a Suss file
#[cfg(not(all(feature = "component", target_family = "wasm")))]
fn run_file(path: &str) {
    let contents = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file '{}': {}", path, e);
            std::process::exit(1);
        }
    };

    let mut runtime = Runtime::new();

    // Parse all expressions in the file
    let mut state = suss_reader::ParserState::new("suss");
    state.interner = std::mem::take(&mut runtime.interner);

    let exprs = match suss_reader::parse_all_and_intern(&contents, &mut state) {
        Ok(exprs) => exprs,
        Err(e) => {
            eprintln!("Parse error in '{}': {}", path, e);
            std::process::exit(1);
        }
    };

    runtime.interner = state.interner;

    // Evaluate each expression
    let mut last_result = None;
    for expr in exprs {
        match runtime.eval(&expr) {
            Ok(result) => last_result = Some(result),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }

    // Print the last result (if any)
    if let Some(result) = last_result {
        println!("{}", print_sexp(&result, &runtime.interner));
    }
}

/// Compile a Suss file to a WASM component
fn compile(source_path: &str, wit_path: &str, output_path: &str) {
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

/// Start the REPL
#[cfg(not(all(feature = "component", target_family = "wasm")))]
fn run_repl() {
    println!("Suss v{} - A Clojure dialect for WASM", env!("CARGO_PKG_VERSION"));
    println!("Type (help) for help, Ctrl-C to exit");
    println!();

    #[cfg(not(target_family = "wasm"))]
    run_repl_native();

    #[cfg(all(target_family = "wasm", not(feature = "component")))]
    run_repl_wasm();
}

#[cfg(not(target_family = "wasm"))]
fn run_repl_native() {
    let mut runtime = Runtime::new();
    let mut rl = match DefaultEditor::new() {
        Ok(editor) => editor,
        Err(e) => {
            eprintln!("Failed to initialize readline: {}", e);
            return;
        }
    };

    loop {
        match rl.readline("suss> ") {
            Ok(line) => {
                if line.trim().is_empty() {
                    continue;
                }

                let _ = rl.add_history_entry(&line);

                match runtime.eval_string(&line) {
                    Ok(result) => {
                        println!("{}", print_sexp(&result, &runtime.interner));
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                break;
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

#[cfg(all(target_family = "wasm", not(feature = "component")))]
fn run_repl_wasm() {
    // Simplified REPL for WASM environment
    // Uses stdin/stdout directly
    use std::io::{self, BufRead, Write};

    let mut runtime = Runtime::new();
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("suss> ");
        let _ = stdout.flush();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                println!("Goodbye!");
                break;
            }
            Ok(_) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                match runtime.eval_string(line) {
                    Ok(result) => {
                        println!("{}", print_sexp(&result, &runtime.interner));
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading input: {}", e);
                break;
            }
        }
    }
}
