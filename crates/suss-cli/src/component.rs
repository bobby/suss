//! WASM Component Model bindings for suss-cli
//!
//! This module provides implementations that call out to the reader and
//! evaluator components via WIT interfaces instead of direct library calls.

wit_bindgen::generate!({
    world: "suss-command",
    path: "../../wit/cli",
    generate_all,
});

use suss::lang::{reader, evaluator};

/// Evaluate a single expression and print the result using WIT bindings
pub fn run_eval(expr: &str) {
    // Parse the expression using the reader component
    match reader::read_string(expr, "suss") {
        Ok(sexp) => {
            // Evaluate using the evaluator component
            match evaluator::eval_to_string(&sexp) {
                Ok(result) => println!("{}", result),
                Err(e) => {
                    eprintln!("Error: {}", e.message);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Parse error: {}", e.message);
            std::process::exit(1);
        }
    }
}

/// Run a Suss file using WIT bindings
pub fn run_file(path: &str) {
    let contents = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file '{}': {}", path, e);
            std::process::exit(1);
        }
    };

    // Parse all expressions using the reader component
    let exprs = match reader::read_all(&contents, "suss") {
        Ok(exprs) => exprs,
        Err(e) => {
            eprintln!("Parse error in '{}': {}", path, e.message);
            std::process::exit(1);
        }
    };

    // Evaluate each expression
    let mut last_result: Option<String> = None;
    for sexp in &exprs {
        match evaluator::eval_to_string(sexp) {
            Ok(result) => last_result = Some(result),
            Err(e) => {
                eprintln!("Error: {}", e.message);
                std::process::exit(1);
            }
        }
    }

    // Print the last result (if any)
    if let Some(result) = last_result {
        println!("{}", result);
    }
}

/// REPL using WIT bindings
pub fn run_repl() {
    use std::io::{self, BufRead, Write};

    println!("Suss v{} - A Clojure dialect for WASM", env!("CARGO_PKG_VERSION"));
    println!("Type (help) for help, Ctrl-C to exit");
    println!();

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

                // Parse using reader component
                match reader::read_string(line, "suss") {
                    Ok(sexp) => {
                        // Evaluate using evaluator component
                        match evaluator::eval_to_string(&sexp) {
                            Ok(result) => println!("{}", result),
                            Err(e) => eprintln!("Error: {}", e.message),
                        }
                    }
                    Err(e) => eprintln!("Parse error: {}", e.message),
                }
            }
            Err(e) => {
                eprintln!("Error reading input: {}", e);
                break;
            }
        }
    }
}
