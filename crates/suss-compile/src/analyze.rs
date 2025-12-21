//! Semantic analysis for Suss source code
//!
//! This module handles:
//! - Symbol resolution
//! - Type inference
//! - Export detection (^:export metadata)
//! - Validation against WIT world

use suss_core::{Sexp, Interner, SymbolId};
use wit_parser::{Resolve, WorldId, WorldItem, TypeDefKind, WorldKey};

use crate::error::{CompileError, CompileResult};
use crate::ir::Type;

/// Analyzed module ready for lowering
#[derive(Debug)]
pub struct AnalyzedModule {
    pub imports: Vec<AnalyzedImport>,
    pub functions: Vec<AnalyzedFunction>,
    pub globals: Vec<AnalyzedGlobal>,
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
    pub name: SymbolId,
    pub exported: bool,
    pub export_name: Option<String>,
    pub params: Vec<(SymbolId, Type)>,
    pub return_type: Type,
    pub body: Sexp,
}

#[derive(Debug)]
pub struct AnalyzedGlobal {
    pub name: SymbolId,
    pub ty: Type,
    pub init: Sexp,
}

/// Analyze Suss expressions and validate against WIT world
pub fn analyze(
    exprs: &[Sexp],
    interner: &Interner,
    resolve: &Resolve,
    world_id: WorldId,
) -> CompileResult<AnalyzedModule> {
    let mut analyzer = Analyzer::new(interner, resolve, world_id);
    analyzer.analyze_module(exprs)
}

struct Analyzer<'a> {
    interner: &'a Interner,
    resolve: &'a Resolve,
    world_id: WorldId,
    imports: Vec<AnalyzedImport>,
    functions: Vec<AnalyzedFunction>,
    globals: Vec<AnalyzedGlobal>,
}

impl<'a> Analyzer<'a> {
    fn new(interner: &'a Interner, resolve: &'a Resolve, world_id: WorldId) -> Self {
        Self {
            interner,
            resolve,
            world_id,
            imports: Vec::new(),
            functions: Vec::new(),
            globals: Vec::new(),
        }
    }

    fn analyze_module(&mut self, exprs: &[Sexp]) -> CompileResult<AnalyzedModule> {
        // First pass: collect all definitions
        for expr in exprs {
            self.analyze_top_level(expr)?;
        }

        // Validate exports against WIT world
        self.validate_exports()?;

        Ok(AnalyzedModule {
            imports: std::mem::take(&mut self.imports),
            functions: std::mem::take(&mut self.functions),
            globals: std::mem::take(&mut self.globals),
        })
    }

    fn analyze_top_level(&mut self, expr: &Sexp) -> CompileResult<()> {
        match expr {
            Sexp::List(items) if !items.is_empty() => {
                if let Sexp::Symbol(sym) = &items[0] {
                    let name = self.interner.symbol_name(*sym);
                    match name {
                        "def" => self.analyze_def(items)?,
                        "defn" => self.analyze_defn(items)?,
                        "require" => self.analyze_require(items)?,
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

    fn analyze_def(&mut self, items: &[Sexp]) -> CompileResult<()> {
        // (def name value) or (def ^:export name value)
        if items.len() < 3 {
            return Err(CompileError::Parse("def requires name and value".into()));
        }

        let (name_sym, _metadata, value_idx) = self.parse_name_with_metadata(&items[1..])?;
        let value = &items[value_idx + 1];

        let ty = self.infer_type(value)?;

        self.globals.push(AnalyzedGlobal {
            name: name_sym,
            ty,
            init: value.clone(),
        });

        Ok(())
    }

    fn analyze_defn(&mut self, items: &[Sexp]) -> CompileResult<()> {
        // (defn name [params] body) or (defn ^:export name [params] body)
        if items.len() < 4 {
            return Err(CompileError::Parse("defn requires name, params, and body".into()));
        }

        let (name_sym, metadata, next_idx) = self.parse_name_with_metadata(&items[1..])?;

        let exported = metadata.contains(&"export");
        let export_name = if exported {
            Some(self.interner.symbol_name(name_sym).to_string())
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
            let do_sym = Sexp::Symbol(self.get_or_intern_symbol("do"));
            let mut body_items = vec![do_sym];
            body_items.extend(items[params_idx + 1..].iter().cloned());
            Sexp::List(body_items)
        };

        // Infer return type from body
        let return_type = self.infer_type(&body)?;

        self.functions.push(AnalyzedFunction {
            name: name_sym,
            exported,
            export_name,
            params,
            return_type,
            body,
        });

        Ok(())
    }

    fn analyze_require(&mut self, items: &[Sexp]) -> CompileResult<()> {
        // (require '[wasi:random/random :as random])
        // (require '[wasi:cli/stdout :refer [print]])
        if items.len() < 2 {
            return Err(CompileError::Parse("require needs a spec".into()));
        }

        // The spec should be a quoted vector: '[...]
        let spec = match &items[1] {
            Sexp::List(quote_items) if quote_items.len() == 2 => {
                // Check if first element is 'quote symbol
                if let Sexp::Symbol(sym) = &quote_items[0] {
                    let name = self.interner.symbol_name(*sym);
                    if name == "quote" {
                        if let Sexp::Vector(vec_items) = &quote_items[1] {
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
            Sexp::Vector(vec_items) => vec_items, // Allow unquoted for simplicity
            _ => return Err(CompileError::Parse("require spec must be a vector".into())),
        };

        if spec.is_empty() {
            return Err(CompileError::Parse("require spec cannot be empty".into()));
        }

        // First element is the interface name
        let interface_name = match &spec[0] {
            Sexp::Symbol(sym) => self.interner.symbol_name(*sym).to_string(),
            _ => return Err(CompileError::Parse("require spec first element must be interface name".into())),
        };

        // Parse options (:as alias or :refer [fns])
        let mut alias: Option<String> = None;
        let mut refer_fns: Vec<String> = Vec::new();
        let mut i = 1;

        while i < spec.len() {
            match &spec[i] {
                Sexp::Keyword(kw) => {
                    let kw_name = self.interner.keyword_name(*kw);
                    match kw_name {
                        "as" => {
                            i += 1;
                            if i >= spec.len() {
                                return Err(CompileError::Parse(":as requires an alias".into()));
                            }
                            if let Sexp::Symbol(sym) = &spec[i] {
                                alias = Some(self.interner.symbol_name(*sym).to_string());
                            } else {
                                return Err(CompileError::Parse(":as alias must be a symbol".into()));
                            }
                        }
                        "refer" => {
                            i += 1;
                            if i >= spec.len() {
                                return Err(CompileError::Parse(":refer requires a vector of names".into()));
                            }
                            if let Sexp::Vector(fns) = &spec[i] {
                                for f in fns {
                                    if let Sexp::Symbol(sym) = f {
                                        refer_fns.push(self.interner.symbol_name(*sym).to_string());
                                    } else {
                                        return Err(CompileError::Parse(":refer elements must be symbols".into()));
                                    }
                                }
                            } else {
                                return Err(CompileError::Parse(":refer requires a vector".into()));
                            }
                        }
                        _ => {
                            return Err(CompileError::Parse(format!("Unknown require option :{}", kw_name)));
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
                    wit_parser::Results::Anon(ty) => wit_type_to_ir(self.resolve, &ty),
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
                    wit_parser::Results::Anon(ty) => wit_type_to_ir(self.resolve, &ty),
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

    fn parse_name_with_metadata(&self, items: &[Sexp]) -> CompileResult<(SymbolId, Vec<&str>, usize)> {
        let mut metadata = Vec::new();
        let mut idx = 0;

        // Check for metadata (^:keyword)
        while idx < items.len() {
            if let Sexp::Symbol(sym) = &items[idx] {
                let name = self.interner.symbol_name(*sym);
                if name.starts_with("^:") {
                    metadata.push(&name[2..]);
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
            Sexp::Symbol(sym) => Ok((*sym, metadata, idx)),
            _ => Err(CompileError::Parse("Expected symbol for name".into())),
        }
    }

    fn parse_params(&self, params: &Sexp) -> CompileResult<Vec<(SymbolId, Type)>> {
        match params {
            Sexp::Vector(items) => {
                items
                    .iter()
                    .map(|item| {
                        match item {
                            Sexp::Symbol(sym) => {
                                // Type will be inferred from WIT or usage
                                Ok((*sym, Type::Unknown))
                            }
                            _ => Err(CompileError::Parse("Expected symbol in parameter list".into())),
                        }
                    })
                    .collect()
            }
            _ => Err(CompileError::Parse("Expected vector for parameters".into())),
        }
    }

    fn infer_type(&self, expr: &Sexp) -> CompileResult<Type> {
        match expr {
            Sexp::Nil => Ok(Type::Unit),
            Sexp::Bool(_) => Ok(Type::Bool),
            Sexp::Number(n) => {
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
            Sexp::String(_) => Ok(Type::String),
            Sexp::Char(_) => Ok(Type::I32), // chars as i32
            Sexp::Symbol(_) => Ok(Type::Unknown), // Need context
            Sexp::List(items) if !items.is_empty() => {
                if let Sexp::Symbol(sym) = &items[0] {
                    let name = self.interner.symbol_name(*sym);
                    match name {
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
                            if let Some(slash_pos) = name.find('/') {
                                let alias = &name[..slash_pos];
                                let func_name = &name[slash_pos + 1..];

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

    fn get_or_intern_symbol(&self, _name: &str) -> SymbolId {
        // This is a bit of a hack - we need a mutable interner
        // For now, assume common symbols are already interned
        // In practice, we'd need to handle this differently
        SymbolId(0) // Placeholder
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
