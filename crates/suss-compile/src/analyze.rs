//! Semantic analysis for Suss source code
//!
//! This module handles:
//! - Symbol resolution
//! - Type inference
//! - Export detection (^:export metadata)
//! - Validation against WIT world
//! - Namespace management and cross-namespace requires

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use num_traits::ToPrimitive;
use suss_core::Edn;
use wit_parser::{Resolve, WorldId, WorldItem, TypeDefKind, WorldKey};

use crate::error::{CompileError, CompileResult};
use crate::ir::Type;

/// Analyzed module ready for lowering
#[derive(Debug)]
pub struct AnalyzedModule {
    /// Namespace name (from ns declaration)
    pub namespace: Option<String>,
    /// Target world (from gen-world in ns declaration)
    pub world_target: Option<String>,
    /// WASI/WIT interface imports
    pub imports: Vec<AnalyzedImport>,
    /// Suss namespace requires (for cross-namespace calls)
    pub suss_requires: Vec<AnalyzedRequire>,
    pub functions: Vec<AnalyzedFunction>,
    pub globals: Vec<AnalyzedGlobal>,
    /// Protocol definitions
    pub protocols: Vec<AnalyzedProtocol>,
    /// Type extensions (extend-type declarations)
    pub extensions: Vec<AnalyzedExtension>,
    /// User-defined types (deftype declarations)
    pub deftypes: Vec<AnalyzedDeftype>,
}

/// A protocol definition
#[derive(Debug, Clone)]
pub struct AnalyzedProtocol {
    /// Protocol name (e.g., "ICounted")
    pub name: String,
    /// Method signatures: (name, arities) where arities are the parameter counts for each arity
    pub methods: Vec<AnalyzedProtocolMethod>,
}

/// A parameter in a protocol method with optional type hint
#[derive(Debug, Clone)]
pub struct ProtocolParam {
    /// Parameter name (e.g., "coll")
    pub name: String,
    /// Type hint (e.g., Some("i32"), Some("eqref"), None for default eqref)
    pub type_hint: Option<String>,
}

/// A protocol method signature
#[derive(Debug, Clone)]
pub struct AnalyzedProtocolMethod {
    /// Method name (e.g., "-count")
    pub name: String,
    /// Parameter lists for each arity with typed params (e.g., [[coll], [coll n]])
    pub arities: Vec<Vec<ProtocolParam>>,
    /// Return type hint (e.g., Some("i32") for -count, None for eqref default)
    pub return_type: Option<String>,
}

/// An extend-type declaration
#[derive(Debug, Clone)]
pub struct AnalyzedExtension {
    /// Type name being extended (e.g., "PersistentVector")
    pub type_name: String,
    /// Protocol implementations
    pub implementations: Vec<AnalyzedProtocolImpl>,
}

/// Implementation of a protocol for a type
#[derive(Debug, Clone)]
pub struct AnalyzedProtocolImpl {
    /// Protocol name (e.g., "ICounted")
    pub protocol_name: String,
    /// Method implementations
    pub methods: Vec<AnalyzedMethodImpl>,
}

/// Implementation of a single protocol method
#[derive(Debug, Clone)]
pub struct AnalyzedMethodImpl {
    /// Method name (e.g., "-count")
    pub name: String,
    /// Parameter names
    pub params: Vec<String>,
    /// Method body
    pub body: Edn,
}

/// A field in a deftype declaration
#[derive(Debug, Clone)]
pub struct DeftypeField {
    /// Field name (e.g., "x")
    pub name: String,
    /// Type hint (e.g., Some("i32"), Some("f64"), None for eqref)
    pub type_hint: Option<String>,
    /// Whether this field is mutable (from ^:mutable metadata)
    pub is_mutable: bool,
}

/// A deftype declaration
#[derive(Debug, Clone)]
pub struct AnalyzedDeftype {
    /// Type name (e.g., "Point")
    pub name: String,
    /// Fields with optional type hints
    pub fields: Vec<DeftypeField>,
    /// Protocol implementations (same structure as extend-type)
    pub implementations: Vec<AnalyzedProtocolImpl>,
    /// Reserved type ID via ^:type-id N metadata
    pub reserved_type_id: Option<u32>,
}

/// An analyzed import from a WIT interface
#[derive(Debug, Clone)]
pub struct AnalyzedImport {
    /// Alias for qualified access (e.g., "random" for random/get-random-u64)
    pub alias: String,
    /// Full WIT interface path
    pub wit_interface: String,
    /// Function name in the interface
    pub function_name: String,
    /// Parameter types
    pub params: Vec<Type>,
    /// Return type
    pub return_type: Type,
}

#[derive(Debug, Clone)]
pub struct AnalyzedFunction {
    pub name: String,
    pub exported: bool,
    pub export_name: Option<String>,
    pub params: Vec<(String, Type)>,
    /// Rest parameter name for variadic functions (e.g., "args" in [a b & args])
    pub rest_param: Option<String>,
    pub return_type: Type,
    /// Explicit return type hint from ^type metadata (e.g., "i32", "i64", "f64")
    pub return_type_hint: Option<String>,
    /// Docstring for documentation (e.g., "Returns the sum of a and b")
    pub docstring: Option<String>,
    pub body: Edn,
}

#[derive(Debug)]
pub struct AnalyzedGlobal {
    pub name: String,
    pub ty: Type,
    pub init: Edn,
    /// Docstring for documentation
    pub docstring: Option<String>,
}

// ============================================================================
// Namespace System Types
// ============================================================================

/// Kind of definition in a namespace
#[derive(Debug, Clone, PartialEq)]
pub enum DefKind {
    Function,
    Protocol,
    Deftype,
    Macro,
    Global,
}

/// Information about a public definition
#[derive(Debug, Clone)]
pub struct PublicDef {
    pub name: String,
    pub kind: DefKind,
    /// Arity for functions (None for non-functions)
    pub arity: Option<usize>,
    /// Whether this is a variadic function
    pub is_variadic: bool,
}

/// Source of a require - either WASI/WIT or a Suss namespace
#[derive(Debug, Clone)]
pub enum RequireSource {
    /// WASI/WIT interface import (e.g., "wasi:random/random")
    WitInterface { interface: String },
    /// Suss namespace (e.g., "myapp.utils")
    SussNamespace { namespace: String },
}

/// An analyzed require statement (for both WASI and Suss namespaces)
#[derive(Debug, Clone)]
pub struct AnalyzedRequire {
    /// Source of the require
    pub source: RequireSource,
    /// Alias for qualified access (e.g., "utils" for utils/func)
    pub alias: Option<String>,
    /// Specifically referred symbols (for :refer [sym1 sym2])
    pub refers: Vec<String>,
    /// Whether :refer :all was specified
    pub refer_all: bool,
}

/// Information about a namespace and its exports
#[derive(Debug, Clone, Default)]
pub struct NamespaceInfo {
    /// Namespace name (e.g., "myapp.core")
    pub name: String,
    /// Public definitions exported from this namespace
    pub publics: HashMap<String, PublicDef>,
    /// Private definitions (marked with ^:private)
    pub privates: HashSet<String>,
    /// Requires from this namespace
    pub requires: Vec<AnalyzedRequire>,
    /// Source file path (if known)
    pub source_file: Option<PathBuf>,
}

impl NamespaceInfo {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            ..Default::default()
        }
    }

    /// Check if a symbol is private
    pub fn is_private(&self, name: &str) -> bool {
        self.privates.contains(name)
    }

    /// Check if a symbol is public (exists and not private)
    pub fn is_public(&self, name: &str) -> bool {
        self.publics.contains_key(name) && !self.privates.contains(name)
    }

    /// Add a public definition
    pub fn add_public(&mut self, name: &str, kind: DefKind, arity: Option<usize>, is_variadic: bool) {
        self.publics.insert(
            name.to_string(),
            PublicDef {
                name: name.to_string(),
                kind,
                arity,
                is_variadic,
            },
        );
    }

    /// Mark a symbol as private
    pub fn add_private(&mut self, name: &str) {
        self.privates.insert(name.to_string());
    }
}

/// Registry of all loaded namespaces
#[derive(Debug, Clone, Default)]
pub struct NamespaceRegistry {
    /// All known namespaces: name -> info
    pub namespaces: HashMap<String, NamespaceInfo>,
    /// Namespace to source file mapping
    pub ns_to_file: HashMap<String, PathBuf>,
}

impl NamespaceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if a namespace is loaded
    pub fn contains(&self, ns: &str) -> bool {
        self.namespaces.contains_key(ns)
    }

    /// Get namespace info
    pub fn get(&self, ns: &str) -> Option<&NamespaceInfo> {
        self.namespaces.get(ns)
    }

    /// Get mutable namespace info
    pub fn get_mut(&mut self, ns: &str) -> Option<&mut NamespaceInfo> {
        self.namespaces.get_mut(ns)
    }

    /// Register a namespace
    pub fn register(&mut self, info: NamespaceInfo) {
        if let Some(ref path) = info.source_file {
            self.ns_to_file.insert(info.name.clone(), path.clone());
        }
        self.namespaces.insert(info.name.clone(), info);
    }

    /// Create an empty namespace (for REPL in-ns)
    pub fn create_empty(&mut self, name: &str) {
        if !self.contains(name) {
            self.register(NamespaceInfo::new(name));
        }
    }

    /// Check if a symbol is visible from one namespace to another
    pub fn is_visible(&self, source_ns: &str, symbol: &str, from_ns: &str) -> bool {
        // Same namespace - always visible
        if source_ns == from_ns {
            return true;
        }
        // Check if symbol is private in source namespace
        if let Some(info) = self.get(source_ns) {
            !info.is_private(symbol)
        } else {
            false
        }
    }
}

/// Analyze Suss expressions and validate against WIT world
pub fn analyze(
    exprs: &[Edn],
    resolve: &Resolve,
    world_id: WorldId,
) -> CompileResult<AnalyzedModule> {
    let mut analyzer = Analyzer::new(resolve, world_id);
    analyzer.analyze_module(exprs)
}

/// Infer the type of a standalone expression (without WIT context)
///
/// This is used by `compile_expr()` to determine the return type of an expression
/// when compiling without a WIT world definition.
pub fn infer_expr_type(expr: &Edn) -> CompileResult<Type> {
    infer_type_standalone(expr)
}

/// Standalone type inference without WIT context
fn infer_type_standalone(expr: &Edn) -> CompileResult<Type> {
    match expr {
        Edn::Nil => Ok(Type::Unit),
        Edn::Bool(_) => Ok(Type::Bool),
        Edn::Number(n) => {
            use suss_core::Number;
            match n {
                Number::Integer(i) => {
                    // Check if it fits in i32
                    if let Some(val) = i.to_i64() {
                        if val >= i32::MIN as i64 && val <= i32::MAX as i64 {
                            Ok(Type::I32)
                        } else {
                            Ok(Type::I64)
                        }
                    } else {
                        // BigInt too large for i64
                        Err(CompileError::Unsupported("Integer too large for WASM".into()))
                    }
                }
                Number::Float(_) => Ok(Type::F64),
                Number::Ratio(_) => {
                    Err(CompileError::Unsupported("Ratios not supported in static compilation".into()))
                }
            }
        }
        Edn::String(_) => Ok(Type::String),
        Edn::Char(_) => Ok(Type::I32),
        Edn::Symbol(_) => Ok(Type::Unknown),
        Edn::Keyword(_) => Ok(Type::Unknown),
        Edn::Vector(_) => {
            // Collections are serialized to EDN strings in lowering
            Ok(Type::String)
        }
        Edn::Map(_) => {
            // Maps are serialized to EDN strings in lowering
            Ok(Type::String)
        }
        Edn::Set(_) => {
            // Sets are serialized to EDN strings in lowering
            Ok(Type::String)
        }
        Edn::List(items) if !items.is_empty() => {
            if let Edn::Symbol(sym) = &items[0] {
                match sym.name.as_str() {
                    "if" => {
                        if items.len() >= 3 {
                            infer_type_standalone(&items[2])
                        } else {
                            Ok(Type::Unknown)
                        }
                    }
                    "do" => {
                        if items.len() > 1 {
                            infer_type_standalone(items.last().unwrap())
                        } else {
                            Ok(Type::Unit)
                        }
                    }
                    "let" => {
                        if items.len() > 2 {
                            infer_type_standalone(items.last().unwrap())
                        } else {
                            Ok(Type::Unit)
                        }
                    }
                    "str" => Ok(Type::String),
                    "+" | "-" | "*" | "/" | "rem" | "mod" => {
                        // Numeric - infer from operands
                        if items.len() > 1 {
                            infer_type_standalone(&items[1])
                        } else {
                            Ok(Type::I32)
                        }
                    }
                    "<" | ">" | "<=" | ">=" | "=" | "not=" => Ok(Type::Bool),
                    "and" | "or" | "not" => Ok(Type::Bool),
                    "loop" => {
                        // Loop returns the type of its body (when it exits via recur target)
                        // For now, return Unknown - will be refined in lowering
                        Ok(Type::Unknown)
                    }
                    "fn" => {
                        // Lambda - return function type
                        Ok(Type::Func {
                            params: Vec::new(),
                            result: Box::new(Type::Unknown),
                        })
                    }
                    _ => Ok(Type::Unknown),
                }
            } else {
                Ok(Type::Unknown)
            }
        }
        _ => Ok(Type::Unknown),
    }
}

struct Analyzer<'a> {
    resolve: &'a Resolve,
    world_id: WorldId,
    namespace: Option<String>,
    world_target: Option<String>,
    imports: Vec<AnalyzedImport>,
    suss_requires: Vec<AnalyzedRequire>,
    functions: Vec<AnalyzedFunction>,
    globals: Vec<AnalyzedGlobal>,
    protocols: Vec<AnalyzedProtocol>,
    extensions: Vec<AnalyzedExtension>,
    deftypes: Vec<AnalyzedDeftype>,
}

impl<'a> Analyzer<'a> {
    fn new(resolve: &'a Resolve, world_id: WorldId) -> Self {
        Self {
            resolve,
            world_id,
            namespace: None,
            world_target: None,
            imports: Vec::new(),
            suss_requires: Vec::new(),
            functions: Vec::new(),
            globals: Vec::new(),
            protocols: Vec::new(),
            extensions: Vec::new(),
            deftypes: Vec::new(),
        }
    }

    fn analyze_module(&mut self, exprs: &[Edn]) -> CompileResult<AnalyzedModule> {
        // First pass: collect all definitions
        for expr in exprs {
            self.analyze_top_level(expr)?;
        }

        // Validate exports against WIT world
        self.validate_exports()?;

        Ok(AnalyzedModule {
            namespace: self.namespace.take(),
            world_target: self.world_target.take(),
            imports: std::mem::take(&mut self.imports),
            suss_requires: std::mem::take(&mut self.suss_requires),
            functions: std::mem::take(&mut self.functions),
            globals: std::mem::take(&mut self.globals),
            protocols: std::mem::take(&mut self.protocols),
            extensions: std::mem::take(&mut self.extensions),
            deftypes: std::mem::take(&mut self.deftypes),
        })
    }

    fn analyze_top_level(&mut self, expr: &Edn) -> CompileResult<()> {
        match expr {
            Edn::List(items) if !items.is_empty() => {
                if let Edn::Symbol(sym) = &items[0] {
                    match sym.name.as_str() {
                        "ns" => self.analyze_ns(items)?,
                        "def" => self.analyze_def(items)?,
                        "defn" => self.analyze_defn(items)?,
                        "require" => self.analyze_require(items)?,
                        "defprotocol" => self.analyze_defprotocol(items)?,
                        "extend-type" => self.analyze_extend_type(items)?,
                        "deftype" => self.analyze_deftype(items)?,
                        _ => {
                            // Top-level expression - ignore for now
                        }
                    }
                }
            }
            _ => {
                // Non-list at top level - ignore
            }
        }
        Ok(())
    }

    /// Analyze namespace declaration
    /// (ns my-app.core (gen-world :my-app/v1))
    fn analyze_ns(&mut self, items: &[Edn]) -> CompileResult<()> {
        if items.len() < 2 {
            return Err(CompileError::Parse("ns requires a namespace name".into()));
        }

        // Parse namespace name
        match &items[1] {
            Edn::Symbol(sym) => {
                self.namespace = Some(sym.name.clone());
            }
            _ => return Err(CompileError::Parse("ns name must be a symbol".into())),
        }

        // Parse optional clauses: (gen-world :world-name), (require ...), etc.
        for item in &items[2..] {
            if let Edn::List(clause) = item {
                if clause.is_empty() {
                    continue;
                }
                if let Edn::Symbol(sym) = &clause[0] {
                    match sym.name.as_str() {
                        "gen-world" => {
                            self.parse_gen_world(clause)?;
                        }
                        "require" => {
                            // ns-style require: (require '[foo :as f])
                            // For now, just delegate to the regular require handler
                            // with the clause items minus "require"
                            let require_items: Vec<Edn> = std::iter::once(Edn::Symbol(suss_core::Symbol::new("require")))
                                .chain(clause[1..].iter().cloned())
                                .collect();
                            self.analyze_require(&require_items)?;
                        }
                        _ => {
                            // Unknown clause - ignore for forward compatibility
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Parse (gen-world :world-name) clause
    fn parse_gen_world(&mut self, clause: &[Edn]) -> CompileResult<()> {
        if clause.len() < 2 {
            return Err(CompileError::Parse("gen-world requires a world name".into()));
        }

        match &clause[1] {
            Edn::Keyword(kw) => {
                // Store the full keyword string including the colon
                self.world_target = Some(kw.to_string());
            }
            _ => {
                return Err(CompileError::Parse(
                    "gen-world world name must be a keyword".into(),
                ))
            }
        }

        Ok(())
    }

    fn analyze_def(&mut self, items: &[Edn]) -> CompileResult<()> {
        // (def name value) or (def ^:export name value)
        // or (def name "docstring" (fn ...)) from defn expansion
        if items.len() < 3 {
            return Err(CompileError::Parse("def requires name and value".into()));
        }

        let (name, metadata, return_type_hint, name_idx) = self.parse_name_with_metadata(&items[1..])?;
        // name_idx is relative to items[1..], so the name is at items[1 + name_idx]
        // Check for optional docstring after name
        let mut value_idx = 1 + name_idx + 1; // Start after name
        let docstring = if let Some(Edn::String(doc)) = items.get(value_idx) {
            value_idx += 1;
            Some(doc.clone())
        } else {
            None
        };

        let value = items.get(value_idx)
            .ok_or_else(|| CompileError::Parse("def requires a value".into()))?;

        // Check if value is an fn form - if so, create an AnalyzedFunction
        if let Edn::List(fn_items) = value {
            if let Some(Edn::Symbol(sym)) = fn_items.first() {
                if sym.name == "fn" {
                    return self.analyze_def_fn(&name, &metadata, &return_type_hint, &docstring, fn_items);
                }
            }
        }

        // Regular global variable
        let ty = self.infer_type(value)?;

        self.globals.push(AnalyzedGlobal {
            name,
            ty,
            init: value.clone(),
            docstring,
        });

        Ok(())
    }

    /// Analyze a def with an fn value: (def name [docstring] (fn [params] body))
    fn analyze_def_fn(
        &mut self,
        name: &str,
        metadata: &[String],
        return_type_hint: &Option<String>,
        docstring: &Option<String>,
        fn_items: &[Edn],
    ) -> CompileResult<()> {
        // fn_items: [fn, [params], body...]
        if fn_items.len() < 3 {
            return Err(CompileError::Parse("fn requires params and body".into()));
        }

        let exported = metadata.iter().any(|s| s == "export");
        let export_name = if exported {
            Some(name.to_string())
        } else {
            None
        };

        // Parse parameters
        let (params, rest_param) = self.parse_params(&fn_items[1])?;

        // Body is everything after params
        let body = if fn_items.len() == 3 {
            fn_items[2].clone()
        } else {
            // Wrap multiple body forms in (do ...)
            let do_sym = Edn::Symbol(suss_core::Symbol::new("do"));
            let mut body_items = vec![do_sym];
            body_items.extend(fn_items[2..].iter().cloned());
            Edn::List(body_items)
        };

        // Infer return type from body
        let return_type = self.infer_type(&body)?;

        self.functions.push(AnalyzedFunction {
            name: name.to_string(),
            exported,
            export_name,
            params,
            rest_param,
            return_type,
            return_type_hint: return_type_hint.clone(),
            docstring: docstring.clone(),
            body,
        });

        Ok(())
    }

    fn analyze_defn(&mut self, items: &[Edn]) -> CompileResult<()> {
        // (defn name [params] body) or (defn ^:export name [params] body)
        // or (defn name "docstring" [params] body)
        if items.len() < 4 {
            return Err(CompileError::Parse("defn requires name, params, and body".into()));
        }

        let (name, metadata, return_type_hint, next_idx) = self.parse_name_with_metadata(&items[1..])?;

        let exported = metadata.iter().any(|s| s == "export");
        let export_name = if exported {
            Some(name.clone())
        } else {
            None
        };

        // Parse parameters
        // next_idx is relative to items[1..], so name is at items[1 + next_idx]
        // After name, there may be an optional docstring, then params vector
        let mut params_idx = next_idx + 2; // Start looking after name

        // Capture docstring if present
        let docstring = if params_idx < items.len() {
            if let Edn::String(doc) = &items[params_idx] {
                params_idx += 1;
                Some(doc.clone())
            } else {
                None
            }
        } else {
            None
        };

        if params_idx >= items.len() {
            return Err(CompileError::Parse("defn requires parameters vector".into()));
        }

        let (params, rest_param) = self.parse_params(&items[params_idx])?;

        // Body is everything after params
        let body = if items.len() - params_idx - 1 == 1 {
            items[params_idx + 1].clone()
        } else {
            // Wrap multiple body forms in (do ...)
            let do_sym = Edn::Symbol(suss_core::Symbol::new("do"));
            let mut body_items = vec![do_sym];
            body_items.extend(items[params_idx + 1..].iter().cloned());
            Edn::List(body_items)
        };

        // Infer return type from body
        let return_type = self.infer_type(&body)?;

        self.functions.push(AnalyzedFunction {
            name,
            exported,
            export_name,
            params,
            rest_param,
            return_type,
            return_type_hint,
            docstring,
            body,
        });

        Ok(())
    }

    fn analyze_require(&mut self, items: &[Edn]) -> CompileResult<()> {
        // WASI/WIT imports:
        //   (require '[wasi:random/random :as random])
        //   (require '[wasi:cli/stdout :refer [print]])
        // Suss namespace imports:
        //   (require '[myapp.utils :as utils])
        //   (require '[myapp.helpers :refer [helper-fn]])
        //   (require '[myapp.core :refer :all])
        if items.len() < 2 {
            return Err(CompileError::Parse("require needs a spec".into()));
        }

        // The spec should be a quoted vector: '[...]
        let spec = match &items[1] {
            Edn::List(quote_items) if quote_items.len() == 2 => {
                // Check if first element is 'quote symbol
                if let Edn::Symbol(sym) = &quote_items[0] {
                    if sym.name == "quote" {
                        if let Edn::Vector(vec_items) = &quote_items[1] {
                            vec_items
                        } else {
                            return Err(CompileError::Parse("require spec must be a vector".into()));
                        }
                    } else {
                        return Err(CompileError::Parse("require spec must be quoted".into()));
                    }
                } else {
                    return Err(CompileError::Parse("require spec must be quoted".into()));
                }
            }
            Edn::Vector(vec_items) => vec_items, // Allow unquoted for simplicity
            _ => return Err(CompileError::Parse("require spec must be a vector".into())),
        };

        if spec.is_empty() {
            return Err(CompileError::Parse("require spec cannot be empty".into()));
        }

        // First element is the namespace/interface name
        let name = match &spec[0] {
            Edn::Symbol(sym) => sym.name.clone(),
            _ => return Err(CompileError::Parse("require spec first element must be a symbol".into())),
        };

        // Parse options (:as alias, :refer [fns], :refer :all)
        let mut alias: Option<String> = None;
        let mut refer_fns: Vec<String> = Vec::new();
        let mut refer_all = false;
        let mut i = 1;

        while i < spec.len() {
            match &spec[i] {
                Edn::Keyword(kw) => {
                    match kw.name.as_str() {
                        "as" => {
                            i += 1;
                            if i >= spec.len() {
                                return Err(CompileError::Parse(":as requires an alias".into()));
                            }
                            if let Edn::Symbol(sym) = &spec[i] {
                                alias = Some(sym.name.clone());
                            } else {
                                return Err(CompileError::Parse(":as alias must be a symbol".into()));
                            }
                        }
                        "refer" => {
                            i += 1;
                            if i >= spec.len() {
                                return Err(CompileError::Parse(":refer requires a vector of names or :all".into()));
                            }
                            match &spec[i] {
                                Edn::Vector(fns) => {
                                    for f in fns {
                                        if let Edn::Symbol(sym) = f {
                                            refer_fns.push(sym.name.clone());
                                        } else {
                                            return Err(CompileError::Parse(":refer elements must be symbols".into()));
                                        }
                                    }
                                }
                                Edn::Keyword(kw) if kw.name == "all" => {
                                    refer_all = true;
                                }
                                _ => {
                                    return Err(CompileError::Parse(":refer requires a vector or :all".into()));
                                }
                            }
                        }
                        other => {
                            return Err(CompileError::Parse(format!("Unknown require option :{}", other)));
                        }
                    }
                }
                _ => {
                    return Err(CompileError::Parse("Expected keyword option in require spec".into()));
                }
            }
            i += 1;
        }

        // Discriminate: contains ':' = WASI/WIT import, otherwise = Suss namespace
        if name.contains(':') {
            self.analyze_wasi_require(&name, alias, refer_fns)?;
        } else {
            self.analyze_suss_require(&name, alias, refer_fns, refer_all)?;
        }

        Ok(())
    }

    /// Analyze a WASI/WIT interface require
    fn analyze_wasi_require(
        &mut self,
        interface_name: &str,
        alias: Option<String>,
        refer_fns: Vec<String>,
    ) -> CompileResult<()> {
        // Look up the interface in the WIT world imports
        let world = &self.resolve.worlds[self.world_id];
        let interface_id = self.find_interface_import(interface_name, world)?;

        // Get functions from the interface
        let interface = &self.resolve.interfaces[interface_id];

        if let Some(ref alias_name) = alias {
            // :as - import all functions with alias prefix
            for (func_name, func) in &interface.functions {
                let (params, return_type) = wit_function_signature(self.resolve, func)?;

                self.imports.push(AnalyzedImport {
                    alias: alias_name.clone(),
                    wit_interface: interface_name.to_string(),
                    function_name: func_name.clone(),
                    params,
                    return_type,
                });
            }
        } else if !refer_fns.is_empty() {
            // :refer - import specific functions without prefix
            for func_name in &refer_fns {
                let func = interface.functions.get(func_name)
                    .ok_or_else(|| CompileError::Parse(
                        format!("Function '{}' not found in interface '{}'", func_name, interface_name)
                    ))?;
                let (params, return_type) = wit_function_signature(self.resolve, func)?;

                self.imports.push(AnalyzedImport {
                    alias: String::new(), // No alias for :refer
                    wit_interface: interface_name.to_string(),
                    function_name: func_name.clone(),
                    params,
                    return_type,
                });
            }
        } else {
            return Err(CompileError::Parse("WASI require needs :as or :refer".into()));
        }

        Ok(())
    }

    /// Analyze a Suss namespace require
    fn analyze_suss_require(
        &mut self,
        namespace: &str,
        alias: Option<String>,
        refers: Vec<String>,
        refer_all: bool,
    ) -> CompileResult<()> {
        // For Suss namespaces, we just record the require info.
        // Actual symbol resolution happens in the lowering phase
        // after all namespaces have been analyzed.
        self.suss_requires.push(AnalyzedRequire {
            source: RequireSource::SussNamespace {
                namespace: namespace.to_string(),
            },
            alias,
            refers,
            refer_all,
        });

        Ok(())
    }

    fn find_interface_import(&self, name: &str, world: &wit_parser::World) -> CompileResult<wit_parser::InterfaceId> {
        // Look for the interface in world imports
        for (_key, item) in &world.imports {
            if let WorldItem::Interface { id, .. } = item {
                // Check if this matches the requested interface name
                let interface = &self.resolve.interfaces[*id];
                if let Some(ref iface_name) = interface.name {
                    // Build full interface path
                    let full_name = if let Some(pkg_id) = interface.package {
                        let pkg = &self.resolve.packages[pkg_id];
                        format!("{}:{}/{}", pkg.name.namespace, pkg.name.name, iface_name)
                    } else {
                        iface_name.clone()
                    };

                    if full_name == name || iface_name == name {
                        return Ok(*id);
                    }
                }
            }
        }

        Err(CompileError::Parse(format!(
            "Interface '{}' not found in world imports. Available imports: {:?}",
            name,
            world.imports.keys().collect::<Vec<_>>()
        )))
    }

    /// Parse function name with metadata prefixes.
    /// Returns (name, keyword_metadata, return_type_hint, idx)
    /// - `^:keyword` goes into keyword_metadata vector (e.g., "export")
    /// - `^type` without colon is a return type hint (e.g., "i32", "i64", "f64")
    fn parse_name_with_metadata(&self, items: &[Edn]) -> CompileResult<(String, Vec<String>, Option<String>, usize)> {
        let mut metadata = Vec::new();
        let mut return_type_hint = None;
        let mut idx = 0;

        // Check for metadata (^:keyword) and type hints (^type)
        while idx < items.len() {
            if let Edn::Symbol(sym) = &items[idx] {
                if sym.name.starts_with("^:") {
                    // ^:keyword - keyword metadata like ^:export
                    metadata.push(sym.name[2..].to_string());
                    idx += 1;
                } else if sym.name.starts_with('^') && sym.name.len() > 1 {
                    // ^type - return type hint like ^i32, ^i64, ^f64
                    return_type_hint = Some(sym.name[1..].to_string());
                    idx += 1;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        if idx >= items.len() {
            return Err(CompileError::Parse("Expected name after metadata".into()));
        }

        match &items[idx] {
            Edn::Symbol(sym) => Ok((sym.name.clone(), metadata, return_type_hint, idx)),
            _ => Err(CompileError::Parse("Expected symbol for name".into())),
        }
    }

    /// Parse parameters, returning (fixed_params, rest_param)
    /// Handles variadic syntax: [a b & rest]
    fn parse_params(&self, params: &Edn) -> CompileResult<(Vec<(String, Type)>, Option<String>)> {
        match params {
            Edn::Vector(items) => {
                let mut fixed_params = Vec::new();
                let mut rest_param = None;
                let mut saw_ampersand = false;

                for item in items {
                    match item {
                        Edn::Symbol(sym) => {
                            if sym.name == "&" {
                                if saw_ampersand {
                                    return Err(CompileError::Parse(
                                        "Multiple & in parameter list".into(),
                                    ));
                                }
                                saw_ampersand = true;
                            } else if saw_ampersand {
                                if rest_param.is_some() {
                                    return Err(CompileError::Parse(
                                        "Only one parameter allowed after &".into(),
                                    ));
                                }
                                rest_param = Some(sym.name.clone());
                            } else {
                                fixed_params.push((sym.name.clone(), Type::Unknown));
                            }
                        }
                        _ => {
                            return Err(CompileError::Parse(
                                "Expected symbol in parameter list".into(),
                            ))
                        }
                    }
                }

                if saw_ampersand && rest_param.is_none() {
                    return Err(CompileError::Parse(
                        "& must be followed by a parameter name".into(),
                    ));
                }

                Ok((fixed_params, rest_param))
            }
            _ => Err(CompileError::Parse("Expected vector for parameters".into())),
        }
    }

    fn infer_type(&self, expr: &Edn) -> CompileResult<Type> {
        match expr {
            Edn::Nil => Ok(Type::Unit),
            Edn::Bool(_) => Ok(Type::Bool),
            Edn::Number(n) => {
                use suss_core::Number;
                match n {
                    Number::Integer(_) => Ok(Type::I32), // Default to i32 for WIT compatibility
                    Number::Float(_) => Ok(Type::F64),
                    Number::Ratio(_) => {
                        // Ratios not directly supported - will need conversion
                        Err(CompileError::Unsupported("Ratios not supported in static compilation".into()))
                    }
                }
            }
            Edn::String(_) => Ok(Type::String),
            Edn::Char(_) => Ok(Type::I32), // chars as i32
            Edn::Symbol(_) => Ok(Type::Unknown), // Need context
            Edn::List(items) if !items.is_empty() => {
                if let Edn::Symbol(sym) = &items[0] {
                    match sym.name.as_str() {
                        "if" => {
                            if items.len() >= 3 {
                                // Type is the type of the then branch
                                self.infer_type(&items[2])
                            } else {
                                Ok(Type::Unknown)
                            }
                        }
                        "do" => {
                            if items.len() > 1 {
                                self.infer_type(items.last().unwrap())
                            } else {
                                Ok(Type::Unit)
                            }
                        }
                        "let" => {
                            // Type is type of last body expression
                            if items.len() > 2 {
                                self.infer_type(items.last().unwrap())
                            } else {
                                Ok(Type::Unit)
                            }
                        }
                        "str" => Ok(Type::String),
                        "+" | "-" | "*" | "/" => {
                            // Numeric - default to i32 for WIT compatibility
                            if items.len() > 1 {
                                self.infer_type(&items[1])
                            } else {
                                Ok(Type::I32)
                            }
                        }
                        "<" | ">" | "<=" | ">=" | "=" | "not=" => Ok(Type::Bool),
                        "and" | "or" | "not" => Ok(Type::Bool),
                        _ => {
                            // Check if it's a qualified import call (e.g., "random/get-random-u64")
                            if let Some(slash_pos) = sym.name.find('/') {
                                let alias = &sym.name[..slash_pos];
                                let func_name = &sym.name[slash_pos + 1..];

                                // Look up in imports
                                for import in &self.imports {
                                    if import.alias == alias && import.function_name == func_name {
                                        return Ok(import.return_type.clone());
                                    }
                                }
                            }
                            Ok(Type::Unknown)
                        }
                    }
                } else {
                    Ok(Type::Unknown)
                }
            }
            _ => Ok(Type::Unknown),
        }
    }

    /// Analyze defprotocol declaration
    /// (defprotocol ICounted (^i32 -count [^eqref coll]))
    /// (defprotocol IIndexed (^eqref -nth [^eqref coll ^i32 n]))
    fn analyze_defprotocol(&mut self, items: &[Edn]) -> CompileResult<()> {
        if items.len() < 2 {
            return Err(CompileError::Parse("defprotocol requires a name".into()));
        }

        // Parse protocol name
        let name = match &items[1] {
            Edn::Symbol(s) => s.name.clone(),
            _ => return Err(CompileError::Parse("defprotocol name must be a symbol".into())),
        };

        // Parse method signatures
        let mut methods = Vec::new();
        for item in &items[2..] {
            if let Edn::List(method_items) = item {
                if method_items.is_empty() {
                    continue;
                }

                // Parse method with optional return type hint
                // Format: (^type -method [params]) or (-method [params])
                let (method_name, return_type, start_idx) = match &method_items[0] {
                    Edn::Symbol(s) if s.name.starts_with('^') => {
                        // ^type hint before method name
                        let type_hint = s.name[1..].to_string();
                        if method_items.len() < 2 {
                            return Err(CompileError::Parse("Expected method name after type hint".into()));
                        }
                        let name = match &method_items[1] {
                            Edn::Symbol(s) => s.name.clone(),
                            _ => return Err(CompileError::Parse("method name must be a symbol".into())),
                        };
                        (name, Some(type_hint), 2)
                    }
                    Edn::Symbol(s) => (s.name.clone(), None, 1),
                    _ => return Err(CompileError::Parse("method name must be a symbol".into())),
                };

                // Parse arities - each remaining item should be a vector of params
                let mut arities = Vec::new();
                for arity_item in &method_items[start_idx..] {
                    if let Edn::Vector(params) = arity_item {
                        let typed_params = self.parse_typed_params(params)?;
                        arities.push(typed_params);
                    }
                }

                methods.push(AnalyzedProtocolMethod {
                    name: method_name,
                    arities,
                    return_type,
                });
            }
        }

        self.protocols.push(AnalyzedProtocol { name, methods });
        Ok(())
    }

    /// Parse typed parameters from a vector
    /// Format: [^type param ^type param ...] or [param param ...]
    fn parse_typed_params(&self, params: &[Edn]) -> CompileResult<Vec<ProtocolParam>> {
        let mut result = Vec::new();
        let mut pending_type_hint: Option<String> = None;

        for item in params {
            if let Edn::Symbol(s) = item {
                if s.name.starts_with('^') {
                    // Type hint for next parameter
                    pending_type_hint = Some(s.name[1..].to_string());
                } else {
                    // Parameter name
                    result.push(ProtocolParam {
                        name: s.name.clone(),
                        type_hint: pending_type_hint.take(),
                    });
                }
            }
        }

        if pending_type_hint.is_some() {
            return Err(CompileError::Parse("Type hint without following parameter".into()));
        }

        Ok(result)
    }

    /// Analyze extend-type declaration
    /// (extend-type PersistentVector
    ///   ICounted
    ///   (-count [coll] (.-cnt coll))
    ///   IIndexed
    ///   (-nth [coll n] ...))
    fn analyze_extend_type(&mut self, items: &[Edn]) -> CompileResult<()> {
        if items.len() < 2 {
            return Err(CompileError::Parse("extend-type requires a type name".into()));
        }

        // Parse type name
        let type_name = match &items[1] {
            Edn::Symbol(s) => s.name.clone(),
            _ => return Err(CompileError::Parse("extend-type type must be a symbol".into())),
        };

        // Parse protocol implementations
        let mut implementations = Vec::new();
        let mut current_protocol: Option<String> = None;
        let mut current_methods: Vec<AnalyzedMethodImpl> = Vec::new();

        for item in &items[2..] {
            match item {
                // Protocol name (bare symbol)
                Edn::Symbol(s) => {
                    // Save previous protocol if any
                    if let Some(protocol_name) = current_protocol.take() {
                        implementations.push(AnalyzedProtocolImpl {
                            protocol_name,
                            methods: std::mem::take(&mut current_methods),
                        });
                    }
                    current_protocol = Some(s.name.clone());
                }
                // Method implementation
                Edn::List(method_items) if !method_items.is_empty() => {
                    // (method-name [params] body)
                    let method_name = match &method_items[0] {
                        Edn::Symbol(s) => s.name.clone(),
                        _ => return Err(CompileError::Parse("method name must be a symbol".into())),
                    };

                    if method_items.len() < 3 {
                        return Err(CompileError::Parse("method requires params and body".into()));
                    }

                    // Parse params
                    let params = match &method_items[1] {
                        Edn::Vector(p) => p.iter()
                            .filter_map(|x| if let Edn::Symbol(s) = x { Some(s.name.clone()) } else { None })
                            .collect(),
                        _ => return Err(CompileError::Parse("method params must be a vector".into())),
                    };

                    // Body is everything after params wrapped in do
                    let body = if method_items.len() == 3 {
                        method_items[2].clone()
                    } else {
                        Edn::List(
                            std::iter::once(Edn::Symbol(suss_core::Symbol::new("do")))
                                .chain(method_items[2..].iter().cloned())
                                .collect()
                        )
                    };

                    current_methods.push(AnalyzedMethodImpl {
                        name: method_name,
                        params,
                        body,
                    });
                }
                _ => {}
            }
        }

        // Save last protocol
        if let Some(protocol_name) = current_protocol {
            implementations.push(AnalyzedProtocolImpl {
                protocol_name,
                methods: current_methods,
            });
        }

        self.extensions.push(AnalyzedExtension {
            type_name,
            implementations,
        });

        Ok(())
    }

    /// Analyze deftype declaration
    /// (deftype Point [x y])
    /// (deftype ^:type-id 5 Point [^i32 x ^i32 y])
    /// (deftype Point [x y] IEquiv (-equiv [this other] ...))
    fn analyze_deftype(&mut self, items: &[Edn]) -> CompileResult<()> {
        if items.len() < 3 {
            return Err(CompileError::Parse("deftype requires name and fields".into()));
        }

        let mut idx = 1;
        let mut reserved_type_id: Option<u32> = None;

        // Check for ^:type-id N metadata
        if idx < items.len() {
            if let Edn::Symbol(sym) = &items[idx] {
                if sym.name == "^:type-id" {
                    idx += 1;
                    if idx >= items.len() {
                        return Err(CompileError::Parse("^:type-id requires a number".into()));
                    }
                    if let Edn::Number(n) = &items[idx] {
                        reserved_type_id = n.to_i64().map(|v| v as u32);
                    } else {
                        return Err(CompileError::Parse("^:type-id value must be a number".into()));
                    }
                    idx += 1;
                }
            }
        }

        // Parse type name
        let name = match &items[idx] {
            Edn::Symbol(s) => s.name.clone(),
            _ => return Err(CompileError::Parse("deftype name must be a symbol".into())),
        };
        idx += 1;

        // Parse fields vector
        let fields = match &items[idx] {
            Edn::Vector(field_items) => self.parse_deftype_fields(field_items)?,
            _ => return Err(CompileError::Parse("deftype fields must be a vector".into())),
        };
        idx += 1;

        // Parse protocol implementations (same as extend-type)
        let mut implementations = Vec::new();
        let mut current_protocol: Option<String> = None;
        let mut current_methods: Vec<AnalyzedMethodImpl> = Vec::new();

        for item in &items[idx..] {
            match item {
                // Protocol name (bare symbol)
                Edn::Symbol(s) => {
                    // Save previous protocol if any
                    if let Some(protocol_name) = current_protocol.take() {
                        implementations.push(AnalyzedProtocolImpl {
                            protocol_name,
                            methods: std::mem::take(&mut current_methods),
                        });
                    }
                    current_protocol = Some(s.name.clone());
                }
                // Method implementation
                Edn::List(method_items) if !method_items.is_empty() => {
                    let method_name = match &method_items[0] {
                        Edn::Symbol(s) => s.name.clone(),
                        _ => return Err(CompileError::Parse("method name must be a symbol".into())),
                    };

                    if method_items.len() < 3 {
                        return Err(CompileError::Parse("method requires params and body".into()));
                    }

                    let params = match &method_items[1] {
                        Edn::Vector(p) => p.iter()
                            .filter_map(|x| if let Edn::Symbol(s) = x { Some(s.name.clone()) } else { None })
                            .collect(),
                        _ => return Err(CompileError::Parse("method params must be a vector".into())),
                    };

                    let body = if method_items.len() == 3 {
                        method_items[2].clone()
                    } else {
                        Edn::List(
                            std::iter::once(Edn::Symbol(suss_core::Symbol::new("do")))
                                .chain(method_items[2..].iter().cloned())
                                .collect()
                        )
                    };

                    current_methods.push(AnalyzedMethodImpl {
                        name: method_name,
                        params,
                        body,
                    });
                }
                _ => {}
            }
        }

        // Save last protocol
        if let Some(protocol_name) = current_protocol {
            implementations.push(AnalyzedProtocolImpl {
                protocol_name,
                methods: current_methods,
            });
        }

        self.deftypes.push(AnalyzedDeftype {
            name,
            fields,
            implementations,
            reserved_type_id,
        });

        Ok(())
    }

    /// Parse deftype fields with optional type hints and mutability
    /// [x y] or [^i32 x ^i64 y z] or [^:mutable val]
    fn parse_deftype_fields(&self, items: &[Edn]) -> CompileResult<Vec<DeftypeField>> {
        let mut fields = Vec::new();
        let mut pending_type_hint: Option<String> = None;
        let mut pending_mutable = false;

        for item in items {
            match item {
                Edn::Symbol(sym) => {
                    // Check for type hint metadata (^i32, ^i64, ^f64, ^eqref) or ^:mutable
                    if sym.name.starts_with('^') {
                        let hint = &sym.name[1..];
                        match hint {
                            "i32" | "i64" | "f64" | "eqref" => {
                                pending_type_hint = Some(hint.to_string());
                            }
                            ":mutable" => {
                                pending_mutable = true;
                            }
                            _ => {
                                return Err(CompileError::Parse(
                                    format!("Unknown type hint ^{}, expected ^i32, ^i64, ^f64, ^eqref, or ^:mutable", hint)
                                ));
                            }
                        }
                    } else {
                        // Regular field name
                        fields.push(DeftypeField {
                            name: sym.name.clone(),
                            type_hint: pending_type_hint.take(),
                            is_mutable: pending_mutable,
                        });
                        pending_mutable = false;
                    }
                }
                _ => {
                    return Err(CompileError::Parse("deftype field must be a symbol".into()));
                }
            }
        }

        if pending_type_hint.is_some() {
            return Err(CompileError::Parse("Type hint without field name".into()));
        }
        if pending_mutable {
            return Err(CompileError::Parse("^:mutable without field name".into()));
        }

        Ok(fields)
    }

    fn validate_exports(&mut self) -> CompileResult<()> {
        let world = &self.resolve.worlds[self.world_id];

        // Check that all WIT exports have corresponding Suss exports
        // and apply WIT types to function params/returns
        for (key, item) in &world.exports {
            match item {
                WorldItem::Function(func) => {
                    let name = match key {
                        WorldKey::Name(n) => n.as_str(),
                        WorldKey::Interface(_) => continue, // Skip interface exports
                    };
                    let found_idx = self.functions.iter().position(|f| {
                        f.exported && f.export_name.as_deref() == Some(name)
                    });
                    match found_idx {
                        None => {
                            return Err(CompileError::ExportMismatch(
                                format!("WIT world requires export '{}' but no matching ^:export function found", name)
                            ));
                        }
                        Some(idx) => {
                            // Apply WIT types to function params
                            let (wit_params, wit_return) = wit_function_signature(self.resolve, func)?;

                            // Update param types from WIT (params keep their Suss names)
                            let suss_func = &mut self.functions[idx];
                            for (i, wit_ty) in wit_params.iter().enumerate() {
                                if i < suss_func.params.len() {
                                    suss_func.params[i].1 = wit_ty.clone();
                                }
                            }
                            suss_func.return_type = wit_return;
                        }
                    }
                }
                WorldItem::Interface { .. } | WorldItem::Type { .. } => {
                    // Interface/type exports not yet supported
                }
            }
        }

        Ok(())
    }
}

/// Validate the prototype adapter profile separately from parser capability.
fn wit_function_signature(
    resolve: &Resolve,
    function: &wit_parser::Function,
) -> CompileResult<(Vec<Type>, Type)> {
    if function.kind.is_async() {
        return Err(CompileError::Unsupported(format!(
            "WIT async function '{}' requires continuation/canonical async lowering",
            function.name
        )));
    }
    if function.external_id.is_some() {
        return Err(CompileError::Unsupported(format!(
            "WIT external-id on '{}' requires generated binding resolution",
            function.name
        )));
    }
    let params = function
        .params
        .iter()
        .map(|param| wit_type_to_ir(resolve, &param.ty))
        .collect::<CompileResult<Vec<_>>>()?;
    let result = function
        .result
        .as_ref()
        .map(|ty| wit_type_to_ir(resolve, ty))
        .transpose()?
        .unwrap_or(Type::Unit);
    Ok((params, result))
}

/// Convert supported WIT types, rejecting unimplemented shapes before codegen.
pub fn wit_type_to_ir(resolve: &Resolve, ty: &wit_parser::Type) -> CompileResult<Type> {
    use wit_parser::Type as WitType;

    Ok(match ty {
        WitType::Bool => Type::Bool,
        WitType::S8 | WitType::S16 | WitType::S32 | WitType::U8 | WitType::U16 | WitType::U32 => {
            Type::I32
        }
        WitType::S64 | WitType::U64 => Type::I64,
        WitType::F32 | WitType::F64 => Type::F64,
        WitType::Char => Type::I32,
        WitType::String => Type::String,
        WitType::ErrorContext => {
            return Err(CompileError::Unsupported(
                "WIT error-context boundary adapters".into(),
            ));
        }
        WitType::Id(id) => {
            let typedef = &resolve.types[*id];
            match &typedef.kind {
                TypeDefKind::List(elem) => Type::List(Box::new(wit_type_to_ir(resolve, elem)?)),
                TypeDefKind::Result(result) => Type::Result {
                    ok: result
                        .ok
                        .as_ref()
                        .map(|t| wit_type_to_ir(resolve, t).map(Box::new))
                        .transpose()?,
                    err: result
                        .err
                        .as_ref()
                        .map(|t| wit_type_to_ir(resolve, t).map(Box::new))
                        .transpose()?,
                },
                TypeDefKind::Option(inner) => {
                    Type::Option(Box::new(wit_type_to_ir(resolve, inner)?))
                }
                TypeDefKind::Type(inner) => return wit_type_to_ir(resolve, inner),
                unsupported => {
                    return Err(CompileError::Unsupported(format!(
                        "WIT {} boundary adapters",
                        unsupported.as_str()
                    )));
                }
            }
        }
    })
}
