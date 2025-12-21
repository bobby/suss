//! Command-line argument parsing for Suss CLI
//!
//! Uses lexopt for minimal, WASM-compatible argument parsing.
//! Follows Clojure CLI conventions.

use lexopt::prelude::*;

/// Commands supported by the Suss CLI
#[derive(Debug)]
pub enum Command {
    /// Start the REPL (default)
    Repl,
    /// Evaluate an expression and print the result
    Eval { expr: String },
    /// Run a file
    RunFile { path: String },
    /// Compile a source file to a WASM component
    Compile {
        source: String,
        world_wit: String,
        output: String,
    },
    /// Print help message
    Help,
    /// Print version
    Version,
}

/// Parse command-line arguments into a Command
pub fn parse_args() -> Result<Command, lexopt::Error> {
    let mut parser = lexopt::Parser::from_env();

    // Check for subcommand or flags first
    match parser.next()? {
        None => Ok(Command::Repl),

        Some(Short('h')) | Some(Long("help")) => Ok(Command::Help),

        Some(Short('V')) | Some(Long("version")) => Ok(Command::Version),

        Some(Short('r')) => Ok(Command::Repl),

        Some(Short('e')) => {
            let expr = parser.value()?.string()?;
            Ok(Command::Eval { expr })
        }

        Some(Value(val)) => {
            let val_str = val.string()?;
            match val_str.as_str() {
                "repl" => Ok(Command::Repl),
                "compile" => parse_compile(&mut parser),
                _ => Ok(Command::RunFile { path: val_str }),
            }
        }

        Some(arg) => Err(arg.unexpected()),
    }
}

/// Parse the compile subcommand arguments
fn parse_compile(parser: &mut lexopt::Parser) -> Result<Command, lexopt::Error> {
    let mut source: Option<String> = None;
    let mut world_wit: Option<String> = None;
    let mut output: Option<String> = None;

    while let Some(arg) = parser.next()? {
        match arg {
            Short('w') | Long("world") => {
                world_wit = Some(parser.value()?.string()?);
            }
            Short('o') | Long("output") => {
                output = Some(parser.value()?.string()?);
            }
            Value(path) if source.is_none() => {
                source = Some(path.string()?);
            }
            _ => return Err(arg.unexpected()),
        }
    }

    let source = source.ok_or_else(|| lexopt::Error::MissingValue {
        option: Some("source file".to_string()),
    })?;
    let world_wit = world_wit.ok_or_else(|| lexopt::Error::MissingValue {
        option: Some("-w/--world".to_string()),
    })?;
    let output = output.ok_or_else(|| lexopt::Error::MissingValue {
        option: Some("-o/--output".to_string()),
    })?;

    Ok(Command::Compile {
        source,
        world_wit,
        output,
    })
}

/// Print help message
pub fn print_help() {
    println!(
        "\
Suss - A Clojure dialect for WASM

USAGE:
    suss [OPTIONS] [FILE]
    suss compile <FILE> -w <WORLD.wit> -o <OUTPUT.wasm>

OPTIONS:
    -r              Start the REPL (default if no arguments)
    -e <EXPR>       Evaluate an expression and print the result
    -h, --help      Print this help message
    -V, --version   Print version information

COMMANDS:
    compile         Compile Suss source to a WASM component
        -w, --world <FILE>   WIT world definition file
        -o, --output <FILE>  Output WASM file path

EXAMPLES:
    suss                           Start the REPL
    suss -e '(+ 1 2)'              Evaluate an expression
    suss script.suss               Run a file
    suss compile main.suss -w world.wit -o out.wasm
"
    );
}
