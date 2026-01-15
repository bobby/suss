//! Default WIT world templates for different compilation modes
//!
//! Provides sensible default worlds for:
//! - CLI command components (WASI CLI)
//! - Embeddable REPL components
//! - Script/expression evaluation

/// WASI CLI command world - for `compile -m ns` output
///
/// This world exports the wasi:cli/run interface and imports standard WASI interfaces.
/// Runnable with `wasmtime run` or `suss run`.
pub const CLI_COMMAND_WORLD: &str = r#"
package suss:generated;

world command {
    // Core CLI imports
    import wasi:cli/environment@0.2.4;
    import wasi:cli/exit@0.2.4;
    import wasi:cli/stdin@0.2.4;
    import wasi:cli/stdout@0.2.4;
    import wasi:cli/stderr@0.2.4;

    // Commonly used WASI imports
    import wasi:random/random@0.2.4;
    import wasi:clocks/wall-clock@0.2.4;
    import wasi:clocks/monotonic-clock@0.2.4;
    import wasi:io/error@0.2.4;
    import wasi:io/streams@0.2.4;

    // Export the proper WASI CLI run interface
    export wasi:cli/run@0.2.4;
}
"#;

/// Embeddable REPL world - for `compile source.sus` without -w or -m
///
/// Provides an eval interface that can be called by host applications
/// or composed with other components.
pub const REPL_COMPONENT_WORLD: &str = r#"
package suss:repl;

interface repl {
    /// Evaluate a Suss expression, return result as string
    eval: func(expr: string) -> result<string, string>;

    /// Read-eval-print one expression (for driving a REPL)
    rep: func(expr: string) -> result<string, string>;
}

world embeddable-repl {
    import wasi:random/random@0.2.4;
    import wasi:clocks/wall-clock@0.2.4;

    export repl;
}
"#;

/// Script/expression world template - auto-detected WASI imports
///
/// Used for `-e` expressions and script execution. The {imports} and
/// {return_type} placeholders are filled in based on code analysis.
pub const SCRIPT_WORLD_TEMPLATE: &str = r#"
package suss:generated;

world script {
{imports}
    export eval: func() -> {return_type};
}
"#;

/// Build a script world with specific imports and return type
pub fn build_script_world(imports: &[String], return_type: &str) -> String {
    let import_lines: String = imports
        .iter()
        .map(|imp| format!("    import {};\n", imp))
        .collect();

    SCRIPT_WORLD_TEMPLATE
        .replace("{imports}", &import_lines)
        .replace("{return_type}", return_type)
}

/// Build the CLI command world (returns the constant)
pub fn build_cli_world() -> String {
    CLI_COMMAND_WORLD.to_string()
}

/// Build the embeddable REPL world (returns the constant)
pub fn build_repl_component_world() -> String {
    REPL_COMPONENT_WORLD.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_script_world() {
        let imports = vec![
            "wasi:random/random@0.2.4".to_string(),
            "wasi:clocks/wall-clock@0.2.4".to_string(),
        ];
        let world = build_script_world(&imports, "s64");

        assert!(world.contains("package suss:generated;"));
        assert!(world.contains("world script {"));
        assert!(world.contains("import wasi:random/random@0.2.4;"));
        assert!(world.contains("import wasi:clocks/wall-clock@0.2.4;"));
        assert!(world.contains("export eval: func() -> s64;"));
    }

    #[test]
    fn test_cli_world_has_run_export() {
        assert!(CLI_COMMAND_WORLD.contains("export wasi:cli/run@0.2.4;"));
    }

    #[test]
    fn test_repl_world_has_eval_export() {
        assert!(REPL_COMPONENT_WORLD.contains("eval: func(expr: string) -> result<string, string>;"));
    }
}
