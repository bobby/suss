//! Error types for the Suss compiler

use thiserror::Error;

/// Result type for compiler operations
pub type CompileResult<T> = Result<T, CompileError>;

/// Errors that can occur during compilation
#[derive(Debug, Error)]
pub enum CompileError {
    /// I/O error (reading files)
    #[error("I/O error: {0}")]
    Io(String),

    /// Parse error in Suss source
    #[error("Parse error: {0}")]
    Parse(String),

    /// WIT parsing/validation error
    #[error("WIT error: {0}")]
    Wit(String),

    /// Type error during analysis
    #[error("Type error: {0}")]
    Type(String),

    /// Semantic error (e.g., recur not in tail position)
    #[error("Semantic error: {0}")]
    Semantic(String),

    /// Unknown or undefined symbol
    #[error("Undefined symbol: {0}")]
    Undefined(String),

    /// Feature not supported in static compilation
    #[error("Unsupported feature: {0}")]
    Unsupported(String),

    /// Mismatch between Suss exports and WIT world
    #[error("Export mismatch: {0}")]
    ExportMismatch(String),

    /// Code generation error
    #[error("Codegen error: {0}")]
    Codegen(String),

    /// Component encoding error
    #[error("Component error: {0}")]
    Component(String),

    /// Configuration error (deps.suss)
    #[error("Config error: {0}")]
    Config(String),

    /// Macro expansion error
    #[error("Macro expansion error: {0}")]
    MacroExpansion(String),

    /// Compile-time macro evaluation error
    #[error("Macro evaluation error: {0}")]
    MacroEval(String),
}
