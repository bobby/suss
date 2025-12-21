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
    pub functions: Vec<AnalyzedFunction>,
    pub globals: Vec<AnalyzedGlobal>,
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
    functions: Vec<AnalyzedFunction>,
    globals: Vec<AnalyzedGlobal>,
}

impl<'a> Analyzer<'a> {
    fn new(interner: &'a Interner, resolve: &'a Resolve, world_id: WorldId) -> Self {
        Self {
            interner,
            resolve,
            world_id,
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
                        _ => Ok(Type::Unknown),
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

    fn get_or_intern_symbol(&self, name: &str) -> SymbolId {
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
