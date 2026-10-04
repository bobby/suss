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
    /// Compile a source file to a WASM component (file mode with WIT)
    CompileFile {
        source: String,
        world_wit: String,
        wit_world: Option<String>,
        src_paths: Vec<String>,
        exports: Vec<String>,
        output: String,
        optimize: bool,
    },
    /// Compile a source file to a CLI command component (main mode)
    CompileMain {
        source: String,
        namespace: String,
        src_paths: Vec<String>,
        output: String,
        optimize: bool,
    },
    /// Compile from a namespace with multi-file support
    CompileNamespace {
        /// Entry namespace (e.g., "myapp.core")
        namespace: String,
        /// Source directories to search
        src_paths: Vec<String>,
        /// WIT world definition file
        world_wit: String,
        wit_world: Option<String>,
        exports: Vec<String>,
        /// Output path
        output: String,
        optimize: bool,
    },
    /// Compile a project from deps.sus
    CompileProject {
        /// Optional specific world to compile (if None, compile all)
        world: Option<String>,
        /// Optional config file path (defaults to deps.sus)
        config_path: Option<String>,
        optimize: bool,
    },
    /// Run a compiled WASM component
    Run {
        /// Path to the WASM component
        component_path: String,
        /// Function to invoke
        invoke: String,
        /// Default command invocation sends arguments to WASI; explicit calls
        /// parse them against the selected function's component types.
        command_mode: bool,
        /// Arguments to pass to the function
        args: Vec<String>,
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
                "run" => parse_run(&mut parser),
                _ => Ok(Command::RunFile { path: val_str }),
            }
        }

        Some(arg) => Err(arg.unexpected()),
    }
}

/// Parse the compile subcommand arguments
///
/// Supports four modes:
/// 1. File mode: `suss compile src.sus -w world.wit -o out.wasm`
/// 2. Main mode: `suss compile src.sus -m namespace -o out.wasm`
/// 3. Namespace mode: `suss compile -n myapp.core --src src -w world.wit -o out.wasm`
/// 4. Project mode: `suss compile` or `suss compile --world :app/v1`
fn parse_compile(parser: &mut lexopt::Parser) -> Result<Command, lexopt::Error> {
    let mut source: Option<String> = None;
    let mut world_wit: Option<String> = None;
    let mut main_ns: Option<String> = None;
    let mut entry_ns: Option<String> = None;
    let mut src_paths: Vec<String> = Vec::new();
    let mut output: Option<String> = None;
    let mut world_target: Option<String> = None;
    let mut config_path: Option<String> = None;
    let mut optimize = false;
    let mut wit_world = None;
    let mut exports = Vec::new();

    while let Some(arg) = parser.next()? {
        match arg {
            Short('w') | Long("wit") => {
                // -w/--wit for WIT file in file mode
                world_wit = Some(parser.value()?.string()?);
            }
            Short('m') | Long("main") => {
                // -m/--main for main mode (CLI command component)
                main_ns = Some(parser.value()?.string()?);
            }
            Short('n') | Long("namespace") => {
                // -n/--namespace for namespace mode (multi-file)
                entry_ns = Some(parser.value()?.string()?);
            }
            Long("src") | Long("src-path") => {
                // --src/--src-path for source directories in namespace mode
                src_paths.push(parser.value()?.string()?);
            }
            Long("world") => {
                // --world for world name in project mode
                world_target = Some(parser.value()?.string()?);
            }
            Long("wit-world") => wit_world = Some(parser.value()?.string()?),
            Long("export") => exports.push(parser.value()?.string()?),
            Short('o') | Long("output") => {
                output = Some(parser.value()?.string()?);
            }
            Short('c') | Long("config") => {
                config_path = Some(parser.value()?.string()?);
            }
            Short('O') | Long("optimize") => {
                optimize = true;
            }
            Value(path) if source.is_none() => {
                source = Some(path.string()?);
            }
            _ => return Err(arg.unexpected()),
        }
    }

    if entry_ns.is_some() && (source.is_some() || main_ns.is_some()) {
        return Err(lexopt::Error::Custom(
            "--namespace cannot be combined with a source file or --main".into(),
        ));
    }
    if source.is_some()
        && main_ns.is_some()
        && (world_wit.is_some() || world_target.is_some() || config_path.is_some())
    {
        return Err(lexopt::Error::Custom(
            "--main uses the official command world and cannot be combined with --wit, --world or --config".into(),
        ));
    }
    if entry_ns.is_some() && world_wit.is_none() {
        return Err(lexopt::Error::Custom(
            "--namespace requires -w/--wit".into(),
        ));
    }
    if entry_ns.is_some() && (world_target.is_some() || config_path.is_some()) {
        return Err(lexopt::Error::Custom(
            "--namespace cannot be combined with project --world or --config".into(),
        ));
    }
    if (wit_world.is_some() || !exports.is_empty())
        && !(world_wit.is_some() && main_ns.is_none() && (source.is_some() || entry_ns.is_some()))
    {
        return Err(lexopt::Error::Custom(
            "--wit-world and --export require file or namespace mode with -w/--wit".into(),
        ));
    }
    // Determine mode based on arguments
    if entry_ns.is_some() && world_wit.is_some() {
        // Namespace mode: compile from entry namespace with multi-file support
        let namespace = entry_ns.unwrap();
        let world_wit = world_wit.unwrap();

        // Default src-path to "src" if not specified
        if src_paths.is_empty() {
            src_paths.push("src".to_string());
        }

        // Default output based on namespace
        let output = output.unwrap_or_else(|| {
            let name = namespace.split('.').last().unwrap_or(&namespace);
            format!("{}.wasm", name)
        });

        Ok(Command::CompileNamespace {
            namespace,
            src_paths,
            world_wit,
            wit_world,
            exports,
            output,
            optimize,
        })
    } else if source.is_some() && main_ns.is_some() {
        // Main mode: compile with -main function
        let source = source.unwrap();
        let output = output.unwrap_or_else(|| source.replace(".sus", ".wasm"));
        Ok(Command::CompileMain {
            source,
            namespace: main_ns.unwrap(),
            src_paths,
            output,
            optimize,
        })
    } else if source.is_some() && world_wit.is_some() {
        // File mode: explicit source and WIT
        let source = source.unwrap();
        let output = output.unwrap_or_else(|| source.replace(".sus", ".wasm"));
        Ok(Command::CompileFile {
            source,
            world_wit: world_wit.unwrap(),
            wit_world,
            src_paths,
            exports,
            output,
            optimize,
        })
    } else if source.is_none() || world_wit.is_none() {
        // Project mode: no source file, or source without -w/-m
        if main_ns.is_some() {
            return Err(lexopt::Error::Custom(
                "--main requires a source file".into(),
            ));
        }
        if source.is_some() && config_path.is_some() {
            return Err(lexopt::Error::Custom(
                "Project mode cannot combine a positional configuration with --config".into(),
            ));
        }
        if world_wit.is_some() || !src_paths.is_empty() || output.is_some() {
            return Err(lexopt::Error::Custom(
                "Project mode takes WIT, source paths and outputs from configuration; -w/--wit, --src and -o/--output require file or namespace mode".into(),
            ));
        }
        if source.is_some() && config_path.is_none() {
            // Treat the positional arg as config path if no -c specified
            config_path = source;
        }
        Ok(Command::CompileProject {
            world: world_target,
            config_path,
            optimize,
        })
    } else {
        // Invalid combination
        Err(lexopt::Error::MissingValue {
            option: Some("-o/--output or -m/--main or -w/--wit required".to_string()),
        })
    }
}

/// Parse the run subcommand arguments
///
/// Usage: `suss run component.wasm [args...]`
/// Usage: `suss run component.wasm --invoke func_name [args...]`
///
/// When `--invoke` is omitted, defaults to `"run"` (the WASI CLI entry point).
/// Extra positional args after the wasm path become WASI argv (for `run`)
/// or function parameters (for explicit `--invoke`).
fn parse_run(parser: &mut lexopt::Parser) -> Result<Command, lexopt::Error> {
    let mut component_path: Option<String> = None;
    let mut invoke: Option<String> = None;
    let mut args: Vec<String> = Vec::new();

    while let Some(arg) = parser.next()? {
        match arg {
            Long("invoke") => {
                invoke = Some(parser.value()?.string()?);
            }
            Value(v) if component_path.is_none() => {
                component_path = Some(v.string()?);
            }
            Value(v) => {
                args.push(v.string()?);
            }
            _ => return Err(arg.unexpected()),
        }
    }

    let component_path = component_path.ok_or_else(|| lexopt::Error::MissingValue {
        option: Some("component path".to_string()),
    })?;

    // Default to "run" (WASI CLI entry point) when --invoke is not specified
    let command_mode = invoke.is_none();
    let invoke = invoke.unwrap_or_else(|| "run".to_string());

    Ok(Command::Run {
        component_path,
        invoke,
        command_mode,
        args,
    })
}

/// Print help message
pub fn print_help() {
    println!(
        "\
Suss - A Clojure dialect for WASM

USAGE:
    suss [OPTIONS] [FILE]
    suss compile [OPTIONS]                              (project mode)
    suss compile <FILE> -w <WIT> --export <PATH=VAR> -o <OUTPUT.wasm>
    suss compile -n <NS> -w <WORLD.wit> [--src <DIR>]   (namespace mode)
    suss run <COMPONENT.wasm> [ARGS...]                  (run CLI command)
    suss run <COMPONENT.wasm> --invoke <FUNC> [ARGS...]  (run specific function)

OPTIONS:
    -r              Start the REPL (default if no arguments)
    -e <EXPR>       Evaluate an expression and print the result
    -h, --help      Print this help message
    -V, --version   Print version information

COMMANDS:
    compile         Compile Suss source to WASM components
    run             Run a compiled WASM component

    compile - Project mode (reads deps.sus):
        --world <NAME>       Compile specific world (e.g., :my-app/v1)
        -c, --config <FILE>  Config file path (default: deps.sus)

    compile - Shared options:
        -O, --optimize       Run wasm-opt on output (requires wasm-opt in PATH)

    compile - File mode (single file compilation):
        -w, --wit <PATH>     WIT file or package directory (including deps)
        --wit-world <WORLD>  Select a world (required if the WIT package is ambiguous)
        --export <PATH=VAR>  Explicit WIT export path to Suss var (repeatable)
        --src <DIR>         Source dependency directory (repeatable)
        -o, --output <FILE>  Output WASM file path

    compile - Namespace mode (multi-file with dependency resolution):
        -n, --namespace <NS> Entry namespace (e.g., myapp.core)
        --src, --src-path <DIR>  Source directory (default: src, repeatable)
        -w, --wit <PATH>     WIT file or package directory (including deps)
        --wit-world <WORLD>  Select a world
        --export <PATH=VAR>  Explicit WIT export path to Suss var (repeatable)
        -o, --output <FILE>  Output WASM file path

    run - Execute a WASM component:
        --invoke <FUNC>      Function or INTERFACE#FUNCTION to invoke
        [ARGS...]            Scalar parameters; use -- before negative values

EXAMPLES:
    suss                           Start the REPL
    suss -e '(+ 1 2)'              Evaluate an expression
    suss script.sus                Run a file
    suss compile                   Compile all worlds from deps.sus
    suss compile --world :app/v1   Compile specific world from deps.sus
    suss compile src.sus -w world.wit --export add=app/add -o out.wasm
    suss compile -n myapp.core -w world.wit --export add=myapp.core/add
    suss run app.wasm hello world                   (run CLI command with args)
    suss run out.wasm --invoke add 3 5              (run specific function)
"
    );
}
