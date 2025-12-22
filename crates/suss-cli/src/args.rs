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
    /// Compile a source file to a WASM component (file mode)
    CompileFile {
        source: String,
        world_wit: String,
        output: String,
    },
    /// Compile a project from deps.suss
    CompileProject {
        /// Optional specific world to compile (if None, compile all)
        world: Option<String>,
        /// Optional config file path (defaults to deps.suss)
        config_path: Option<String>,
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
///
/// Supports two modes:
/// 1. File mode: `suss compile src.suss -w world.wit -o out.wasm`
/// 2. Project mode: `suss compile` or `suss compile --world :app/v1`
fn parse_compile(parser: &mut lexopt::Parser) -> Result<Command, lexopt::Error> {
    let mut source: Option<String> = None;
    let mut world_wit: Option<String> = None;
    let mut output: Option<String> = None;
    let mut world_target: Option<String> = None;
    let mut config_path: Option<String> = None;

    while let Some(arg) = parser.next()? {
        match arg {
            Short('w') | Long("wit") => {
                // -w/--wit for WIT file in file mode
                world_wit = Some(parser.value()?.string()?);
            }
            Long("world") => {
                // --world for world name in project mode
                world_target = Some(parser.value()?.string()?);
            }
            Short('o') | Long("output") => {
                output = Some(parser.value()?.string()?);
            }
            Short('c') | Long("config") => {
                config_path = Some(parser.value()?.string()?);
            }
            Value(path) if source.is_none() => {
                source = Some(path.string()?);
            }
            _ => return Err(arg.unexpected()),
        }
    }

    // Determine mode based on arguments
    if source.is_some() && world_wit.is_some() && output.is_some() {
        // File mode: explicit source, WIT, and output
        Ok(Command::CompileFile {
            source: source.unwrap(),
            world_wit: world_wit.unwrap(),
            output: output.unwrap(),
        })
    } else if source.is_none() || (source.is_some() && world_wit.is_none()) {
        // Project mode: no source file, or source without -w (treat as config path)
        if source.is_some() && config_path.is_none() {
            // Treat the positional arg as config path if no -c specified
            config_path = source;
        }
        Ok(Command::CompileProject {
            world: world_target,
            config_path,
        })
    } else {
        // Invalid combination
        Err(lexopt::Error::MissingValue {
            option: Some("-o/--output (required for file mode)".to_string()),
        })
    }
}

/// Print help message
pub fn print_help() {
    println!(
        "\
Suss - A Clojure dialect for WASM

USAGE:
    suss [OPTIONS] [FILE]
    suss compile [OPTIONS]                              (project mode)
    suss compile <FILE> -w <WORLD.wit> -o <OUTPUT.wasm> (file mode)

OPTIONS:
    -r              Start the REPL (default if no arguments)
    -e <EXPR>       Evaluate an expression and print the result
    -h, --help      Print this help message
    -V, --version   Print version information

COMMANDS:
    compile         Compile Suss source to WASM components

    Project mode (reads deps.suss):
        --world <NAME>       Compile specific world (e.g., :my-app/v1)
        -c, --config <FILE>  Config file path (default: deps.suss)

    File mode (single file compilation):
        -w, --wit <FILE>     WIT world definition file
        -o, --output <FILE>  Output WASM file path

EXAMPLES:
    suss                           Start the REPL
    suss -e '(+ 1 2)'              Evaluate an expression
    suss script.suss               Run a file
    suss compile                   Compile all worlds from deps.suss
    suss compile --world :app/v1   Compile specific world from deps.suss
    suss compile src.suss -w world.wit -o out.wasm  (file mode)
"
    );
}
