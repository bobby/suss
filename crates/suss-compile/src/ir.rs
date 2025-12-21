//! Intermediate Representation for the Suss compiler
//!
//! The IR is a simplified, typed representation of Suss code that maps
//! closely to WASM instructions.

use suss_core::SymbolId;

/// A compiled module containing all definitions
#[derive(Debug)]
pub struct Module {
    /// All function definitions
    pub functions: Vec<Function>,
    /// Global constant definitions
    pub globals: Vec<Global>,
    /// String literals (stored in data section)
    pub strings: Vec<String>,
}

impl Module {
    pub fn new() -> Self {
        Self {
            functions: Vec::new(),
            globals: Vec::new(),
            strings: Vec::new(),
        }
    }

    /// Intern a string literal, returning its index
    pub fn intern_string(&mut self, s: &str) -> u32 {
        if let Some(idx) = self.strings.iter().position(|x| x == s) {
            return idx as u32;
        }
        let idx = self.strings.len() as u32;
        self.strings.push(s.to_string());
        idx
    }
}

impl Default for Module {
    fn default() -> Self {
        Self::new()
    }
}

/// A function definition
#[derive(Debug)]
pub struct Function {
    /// Function name (symbol)
    pub name: SymbolId,
    /// Whether this function is exported via WIT
    pub exported: bool,
    /// Export name (may differ from internal name)
    pub export_name: Option<String>,
    /// Parameter types
    pub params: Vec<(SymbolId, Type)>,
    /// Return type
    pub return_type: Type,
    /// Local variable types (including parameters)
    pub locals: Vec<Type>,
    /// Function body
    pub body: Expr,
}

/// A global constant
#[derive(Debug)]
pub struct Global {
    /// Name
    pub name: SymbolId,
    /// Type
    pub ty: Type,
    /// Initializer (must be constant)
    pub init: Expr,
}

/// Types supported by the compiler
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// Unit/nil type
    Unit,
    /// Boolean
    Bool,
    /// 32-bit signed integer
    I32,
    /// 64-bit signed integer
    I64,
    /// 64-bit float
    F64,
    /// String (pointer + length in linear memory)
    String,
    /// List of elements (pointer + length)
    List(Box<Type>),
    /// Fixed-size vector
    Vector(Box<Type>),
    /// Function type
    Func {
        params: Vec<Type>,
        result: Box<Type>,
    },
    /// Unknown type (for type inference)
    Unknown,
}

impl Type {
    /// Get the WASM representation size in bytes
    pub fn wasm_size(&self) -> u32 {
        match self {
            Type::Unit => 0,
            Type::Bool | Type::I32 => 4,
            Type::I64 => 8,
            Type::F64 => 8,
            Type::String => 8, // ptr + len
            Type::List(_) => 8, // ptr + len
            Type::Vector(_) => 8, // ptr + len
            Type::Func { .. } => 4, // function table index
            Type::Unknown => 0,
        }
    }

    /// Check if this is a numeric type
    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::I32 | Type::I64 | Type::F64)
    }
}

/// Expressions in the IR
#[derive(Debug, Clone)]
pub enum Expr {
    /// Unit/nil literal
    Unit,

    /// Boolean literal
    Bool(bool),

    /// Integer literal (fits in i64)
    Int(i64),

    /// Float literal
    Float(f64),

    /// String literal (index into string table)
    String(u32),

    /// Local variable reference
    LocalGet(u32),

    /// Local variable assignment
    LocalSet(u32, Box<Expr>),

    /// Global variable reference
    GlobalGet(u32),

    /// Global variable assignment
    GlobalSet(u32, Box<Expr>),

    /// Binary operation
    BinOp {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
        ty: Type,
    },

    /// Unary operation
    UnOp {
        op: UnOp,
        operand: Box<Expr>,
        ty: Type,
    },

    /// Function call
    Call {
        func: u32, // function index
        args: Vec<Expr>,
    },

    /// Conditional
    If {
        cond: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
        ty: Type,
    },

    /// Sequence of expressions (returns last)
    Block(Vec<Expr>),

    /// Let binding
    Let {
        bindings: Vec<(u32, Expr)>, // local index, value
        body: Box<Expr>,
    },

    /// Loop with recur
    Loop {
        bindings: Vec<(u32, Expr)>,
        body: Box<Expr>,
    },

    /// Recur (jump back to loop)
    Recur(Vec<Expr>),

    /// String concatenation
    StrConcat(Vec<Expr>),

    /// Type coercion
    Coerce {
        expr: Box<Expr>,
        from: Type,
        to: Type,
    },
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Rem,

    // Comparison
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,

    // Logical
    And,
    Or,
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}
