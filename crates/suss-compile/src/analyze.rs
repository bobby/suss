//! Semantic analysis for Suss source code
//!
//! This module handles:
//! - Symbol resolution
//! - Type inference
//! - Export detection (^:export metadata)
//! - Validation against WIT world

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
    pub imports: Vec<AnalyzedImport>,
    pub functions: Vec<AnalyzedFunction>,
    pub globals: Vec<AnalyzedGlobal>,
    /// Protocol definitions
    pub protocols: Vec<AnalyzedProtocol>,
    /// Type extensions (extend-type declarations)
    pub extensions: Vec<AnalyzedExtension>,
}

/// A protocol definition
#[derive(Debug, Clone)]
pub struct AnalyzedProtocol {
    /// Protocol name (e.g., "ICounted")
    pub name: String,
    /// Method signatures: (name, arities) where arities are the parameter counts for each arity
    pub methods: Vec<AnalyzedProtocolMethod>,
}

/// A protocol method signature
#[derive(Debug, Clone)]
pub struct AnalyzedProtocolMethod {
    /// Method name (e.g., "-count")
    pub name: String,
    /// Parameter lists for each arity (e.g., [[coll], [coll n], [coll n not-found]])
    pub arities: Vec<Vec<String>>,
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

#[derive(Debug)]
pub struct AnalyzedFunction {
    pub name: String,
    pub exported: bool,
    pub export_name: Option<String>,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    pub body: Edn,
}

#[derive(Debug)]
pub struct AnalyzedGlobal {
    pub name: String,
    pub ty: Type,
    pub init: Edn,
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
    functions: Vec<AnalyzedFunction>,
    globals: Vec<AnalyzedGlobal>,
    protocols: Vec<AnalyzedProtocol>,
    extensions: Vec<AnalyzedExtension>,
}

impl<'a> Analyzer<'a> {
    fn new(resolve: &'a Resolve, world_id: WorldId) -> Self {
        Self {
            resolve,
            world_id,
            namespace: None,
            world_target: None,
            imports: Vec::new(),
            functions: Vec::new(),
            globals: Vec::new(),
            protocols: Vec::new(),
            extensions: Vec::new(),
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
            functions: std::mem::take(&mut self.functions),
            globals: std::mem::take(&mut self.globals),
            protocols: std::mem::take(&mut self.protocols),
            extensions: std::mem::take(&mut self.extensions),
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
        if items.len() < 3 {
            return Err(CompileError::Parse("def requires name and value".into()));
        }

        let (name, _metadata, value_idx) = self.parse_name_with_metadata(&items[1..])?;
        let value = &items[value_idx + 1];

        let ty = self.infer_type(value)?;

        self.globals.push(AnalyzedGlobal {
            name,
            ty,
            init: value.clone(),
        });

        Ok(())
    }

    fn analyze_defn(&mut self, items: &[Edn]) -> CompileResult<()> {
        // (defn name [params] body) or (defn ^:export name [params] body)
        if items.len() < 4 {
            return Err(CompileError::Parse("defn requires name, params, and body".into()));
        }

        let (name, metadata, next_idx) = self.parse_name_with_metadata(&items[1..])?;

        let exported = metadata.iter().any(|s| s == "export");
        let export_name = if exported {
            Some(name.clone())
        } else {
            None
        };

        // Parse parameters
        // next_idx is relative to items[1..], so params are at items[1 + next_idx + 1]
        let params_idx = next_idx + 2;
        let params = self.parse_params(&items[params_idx])?;

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
            return_type,
            body,
        });

        Ok(())
    }

    fn analyze_require(&mut self, items: &[Edn]) -> CompileResult<()> {
        // (require '[wasi:random/random :as random])
        // (require '[wasi:cli/stdout :refer [print]])
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

        // First element is the interface name
        let interface_name = match &spec[0] {
            Edn::Symbol(sym) => sym.name.clone(),
            _ => return Err(CompileError::Parse("require spec first element must be interface name".into())),
        };

        // Parse options (:as alias or :refer [fns])
        let mut alias: Option<String> = None;
        let mut refer_fns: Vec<String> = Vec::new();
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
                                return Err(CompileError::Parse(":refer requires a vector of names".into()));
                            }
                            if let Edn::Vector(fns) = &spec[i] {
                                for f in fns {
                                    if let Edn::Symbol(sym) = f {
                                        refer_fns.push(sym.name.clone());
                                    } else {
                                        return Err(CompileError::Parse(":refer elements must be symbols".into()));
                                    }
                                }
                            } else {
                                return Err(CompileError::Parse(":refer requires a vector".into()));
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

        // Look up the interface in the WIT world imports
        let world = &self.resolve.worlds[self.world_id];
        let interface_id = self.find_interface_import(&interface_name, world)?;

        // Get functions from the interface
        let interface = &self.resolve.interfaces[interface_id];

        if let Some(ref alias_name) = alias {
            // :as - import all functions with alias prefix
            for (func_name, func) in &interface.functions {
                let params = func.params.iter()
                    .map(|(_, ty)| wit_type_to_ir(self.resolve, ty))
                    .collect();
                let return_type = match &func.results {
                    wit_parser::Results::Named(results) if results.is_empty() => Type::Unit,
                    wit_parser::Results::Named(results) => {
                        wit_type_to_ir(self.resolve, &results[0].1)
                    }
                    wit_parser::Results::Anon(ty) => wit_type_to_ir(self.resolve, ty),
                };

                self.imports.push(AnalyzedImport {
                    alias: alias_name.clone(),
                    wit_interface: interface_name.clone(),
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
                let params = func.params.iter()
                    .map(|(_, ty)| wit_type_to_ir(self.resolve, ty))
                    .collect();
                let return_type = match &func.results {
                    wit_parser::Results::Named(results) if results.is_empty() => Type::Unit,
                    wit_parser::Results::Named(results) => {
                        wit_type_to_ir(self.resolve, &results[0].1)
                    }
                    wit_parser::Results::Anon(ty) => wit_type_to_ir(self.resolve, ty),
                };

                self.imports.push(AnalyzedImport {
                    alias: String::new(), // No alias for :refer
                    wit_interface: interface_name.clone(),
                    function_name: func_name.clone(),
                    params,
                    return_type,
                });
            }
        } else {
            return Err(CompileError::Parse("require needs :as or :refer".into()));
        }

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

    fn parse_name_with_metadata(&self, items: &[Edn]) -> CompileResult<(String, Vec<String>, usize)> {
        let mut metadata = Vec::new();
        let mut idx = 0;

        // Check for metadata (^:keyword)
        while idx < items.len() {
            if let Edn::Symbol(sym) = &items[idx] {
                if sym.name.starts_with("^:") {
                    metadata.push(sym.name[2..].to_string());
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
            Edn::Symbol(sym) => Ok((sym.name.clone(), metadata, idx)),
            _ => Err(CompileError::Parse("Expected symbol for name".into())),
        }
    }

    fn parse_params(&self, params: &Edn) -> CompileResult<Vec<(String, Type)>> {
        match params {
            Edn::Vector(items) => {
                items
                    .iter()
                    .map(|item| {
                        match item {
                            Edn::Symbol(sym) => {
                                // Type will be inferred from WIT or usage
                                Ok((sym.name.clone(), Type::Unknown))
                            }
                            _ => Err(CompileError::Parse("Expected symbol in parameter list".into())),
                        }
                    })
                    .collect()
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
    /// (defprotocol ICounted (-count [coll]))
    /// (defprotocol IIndexed (-nth [coll n] [coll n not-found]))
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

                // Method name
                let method_name = match &method_items[0] {
                    Edn::Symbol(s) => s.name.clone(),
                    _ => return Err(CompileError::Parse("method name must be a symbol".into())),
                };

                // Parse arities - each remaining item should be a vector of params
                let mut arities = Vec::new();
                for arity_item in &method_items[1..] {
                    if let Edn::Vector(params) = arity_item {
                        let param_names: Vec<String> = params.iter()
                            .filter_map(|p| {
                                if let Edn::Symbol(s) = p {
                                    Some(s.name.clone())
                                } else {
                                    None
                                }
                            })
                            .collect();
                        arities.push(param_names);
                    }
                }

                methods.push(AnalyzedProtocolMethod {
                    name: method_name,
                    arities,
                });
            }
        }

        self.protocols.push(AnalyzedProtocol { name, methods });
        Ok(())
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

    fn validate_exports(&self) -> CompileResult<()> {
        let world = &self.resolve.worlds[self.world_id];

        // Check that all WIT exports have corresponding Suss exports
        for (key, item) in &world.exports {
            match item {
                WorldItem::Function(_) => {
                    let name = match key {
                        WorldKey::Name(n) => n.as_str(),
                        WorldKey::Interface(_) => continue, // Skip interface exports
                    };
                    let found = self.functions.iter().any(|f| {
                        f.exported && f.export_name.as_deref() == Some(name)
                    });
                    if !found {
                        return Err(CompileError::ExportMismatch(
                            format!("WIT world requires export '{}' but no matching ^:export function found", name)
                        ));
                    }
                }
                WorldItem::Interface { .. } | WorldItem::Type(_) => {
                    // Interface/type exports not yet supported
                }
            }
        }

        Ok(())
    }
}

/// Convert WIT type to IR type
pub fn wit_type_to_ir(resolve: &Resolve, ty: &wit_parser::Type) -> Type {
    use wit_parser::Type as WitType;

    match ty {
        WitType::Bool => Type::Bool,
        WitType::S8 | WitType::S16 | WitType::S32 | WitType::U8 | WitType::U16 | WitType::U32 => Type::I32,
        WitType::S64 | WitType::U64 => Type::I64,
        WitType::F32 | WitType::F64 => Type::F64,
        WitType::Char => Type::I32,
        WitType::String => Type::String,
        WitType::Id(id) => {
            let typedef = &resolve.types[*id];
            match &typedef.kind {
                TypeDefKind::List(elem) => Type::List(Box::new(wit_type_to_ir(resolve, elem))),
                _ => Type::Unknown,
            }
        }
    }
}
