//! Core evaluation logic

use crate::primitives::apply_primitive;
use suss_core::{Env, Interner, Sexp, SymbolId};

/// An evaluation error
#[derive(Debug, Clone)]
pub struct EvalError {
    pub message: String,
    pub backtrace: Vec<String>,
}

impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for EvalError {}

impl EvalError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            backtrace: vec![],
        }
    }
}

/// Evaluate an s-expression
pub fn eval(expr: &Sexp, env: &mut Env, interner: &mut Interner) -> Result<Sexp, EvalError> {
    match expr {
        // Self-evaluating forms
        Sexp::Nil | Sexp::Bool(_) | Sexp::Number(_) | Sexp::Char(_) | Sexp::String(_) => {
            Ok(expr.clone())
        }

        // Keywords are self-evaluating
        Sexp::Keyword(_) => Ok(expr.clone()),

        // Primitives and functions are self-evaluating
        Sexp::Primitive(_) | Sexp::Function { .. } => Ok(expr.clone()),

        // Symbol lookup
        Sexp::Symbol(id) => env
            .lookup(*id)
            .cloned()
            .ok_or_else(|| EvalError::new(format!("Undefined symbol: {}", interner.symbol_name(*id)))),

        // Vector literal - evaluate elements
        Sexp::Vector(items) => {
            let evaluated: Result<Vec<_>, _> = items
                .iter()
                .map(|item| eval(item, env, interner))
                .collect();
            Ok(Sexp::Vector(evaluated?))
        }

        // Set literal - evaluate elements
        Sexp::Set(items) => {
            let evaluated: Result<Vec<_>, _> = items
                .iter()
                .map(|item| eval(item, env, interner))
                .collect();
            Ok(Sexp::Set(evaluated?))
        }

        // Map literal - evaluate keys and values
        Sexp::Map(pairs) => {
            let evaluated: Result<Vec<_>, _> = pairs
                .iter()
                .map(|(k, v)| {
                    let ek = eval(k, env, interner)?;
                    let ev = eval(v, env, interner)?;
                    Ok((ek, ev))
                })
                .collect();
            Ok(Sexp::Map(evaluated?))
        }

        // Reader conditional - select based on platform
        Sexp::ReaderConditional(clauses) => {
            let suss_kw = interner.intern_keyword("suss");
            let default_kw = interner.intern_keyword("default");

            // First try :suss
            for (platform, expr) in clauses {
                if *platform == suss_kw {
                    return eval(expr, env, interner);
                }
            }

            // Fall back to :default
            for (platform, expr) in clauses {
                if *platform == default_kw {
                    return eval(expr, env, interner);
                }
            }

            Ok(Sexp::Nil)
        }

        // List - function application or special form
        Sexp::List(items) => {
            if items.is_empty() {
                return Ok(Sexp::List(vec![]));
            }

            // Check for special forms (now using Symbol variant after interning)
            if let Sexp::Symbol(id) = &items[0] {
                let name = interner.symbol_name(*id).to_string();
                if is_special_form(&name) {
                    return eval_special_form(&name, &items[1..], env, interner);
                }
            }

            // Evaluate the operator
            let operator = eval(&items[0], env, interner)?;

            // Evaluate arguments
            let args: Result<Vec<_>, _> = items[1..]
                .iter()
                .map(|arg| eval(arg, env, interner))
                .collect();
            let args = args?;

            // Apply the function
            match operator {
                Sexp::Primitive(name) => {
                    apply_primitive(&name, &args)
                        .map_err(EvalError::new)
                }
                Sexp::Function { params, body, env: captured_env } => {
                    apply_function(params, *body, *captured_env, args, interner)
                }
                other => Err(EvalError::new(format!(
                    "Cannot call non-function: {:?}", other
                ))),
            }
        }
    }
}

/// Check if a symbol name is a special form
fn is_special_form(name: &str) -> bool {
    matches!(name, "quote" | "if" | "do" | "def" | "let" | "fn")
}

/// Apply a user-defined function
fn apply_function(
    params: Vec<SymbolId>,
    body: Sexp,
    mut captured_env: Env,
    args: Vec<Sexp>,
    interner: &mut Interner,
) -> Result<Sexp, EvalError> {
    if params.len() != args.len() {
        return Err(EvalError::new(format!(
            "Expected {} arguments, got {}", params.len(), args.len()
        )));
    }

    // Bind parameters in a new scope
    captured_env.push_scope();
    for (param, arg) in params.iter().zip(args) {
        captured_env.define(*param, arg);
    }

    // Evaluate body
    let result = eval(&body, &mut captured_env, interner);

    captured_env.pop_scope();
    result
}

/// Evaluate a special form
fn eval_special_form(
    name: &str,
    args: &[Sexp],
    env: &mut Env,
    interner: &mut Interner,
) -> Result<Sexp, EvalError> {
    match name {
        "quote" => {
            if args.len() != 1 {
                return Err(EvalError::new("quote requires exactly 1 argument"));
            }
            Ok(args[0].clone())
        }

        "if" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(EvalError::new("if requires 2 or 3 arguments"));
            }
            let cond = eval(&args[0], env, interner)?;
            if cond.is_truthy() {
                eval(&args[1], env, interner)
            } else if args.len() == 3 {
                eval(&args[2], env, interner)
            } else {
                Ok(Sexp::Nil)
            }
        }

        "do" => {
            let mut result = Sexp::Nil;
            for arg in args {
                result = eval(arg, env, interner)?;
            }
            Ok(result)
        }

        "def" => {
            if args.len() != 2 {
                return Err(EvalError::new("def requires exactly 2 arguments"));
            }

            // The name should be a symbol
            let sym_id = match &args[0] {
                Sexp::Symbol(id) => *id,
                _ => return Err(EvalError::new("def first argument must be a symbol")),
            };

            let value = eval(&args[1], env, interner)?;
            env.define_global(sym_id, value.clone());
            Ok(value)
        }

        "let" => {
            if args.len() < 2 {
                return Err(EvalError::new("let requires at least 2 arguments"));
            }

            // First arg should be a vector of bindings
            let bindings = match &args[0] {
                Sexp::Vector(b) => b,
                _ => return Err(EvalError::new("let bindings must be a vector")),
            };

            if bindings.len() % 2 != 0 {
                return Err(EvalError::new("let bindings must have even number of forms"));
            }

            env.push_scope();

            // Process bindings
            for chunk in bindings.chunks(2) {
                let sym_id = match &chunk[0] {
                    Sexp::Symbol(id) => *id,
                    _ => {
                        env.pop_scope();
                        return Err(EvalError::new("let binding name must be a symbol"));
                    }
                };

                let value = eval(&chunk[1], env, interner)?;
                env.define(sym_id, value);
            }

            // Evaluate body forms
            let mut result = Sexp::Nil;
            for body_form in &args[1..] {
                result = eval(body_form, env, interner)?;
            }

            env.pop_scope();
            Ok(result)
        }

        "fn" => {
            if args.is_empty() {
                return Err(EvalError::new("fn requires params and body"));
            }

            // Parse parameter vector
            let params = match &args[0] {
                Sexp::Vector(params) => {
                    params.iter().map(|p| match p {
                        Sexp::Symbol(id) => Ok(*id),
                        _ => Err(EvalError::new("fn params must be symbols")),
                    }).collect::<Result<Vec<_>, _>>()?
                }
                _ => return Err(EvalError::new("fn params must be a vector")),
            };

            // Body is wrapped in do if multiple forms
            let body = if args.len() == 2 {
                args[1].clone()
            } else if args.len() > 2 {
                let do_sym = interner.intern_symbol("do");
                Sexp::List(
                    std::iter::once(Sexp::Symbol(do_sym))
                        .chain(args[1..].iter().cloned())
                        .collect()
                )
            } else {
                Sexp::Nil
            };

            Ok(Sexp::Function {
                params,
                body: Box::new(body),
                env: Box::new(env.clone()),
            })
        }

        other => {
            Err(EvalError::new(format!("Unknown special form: {}", other)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suss_core::Number;

    #[test]
    fn test_eval_nil() {
        let mut env = Env::new();
        let mut interner = Interner::new();
        let result = eval(&Sexp::Nil, &mut env, &mut interner).unwrap();
        assert_eq!(result, Sexp::Nil);
    }

    #[test]
    fn test_eval_number() {
        let mut env = Env::new();
        let mut interner = Interner::new();
        let num = Sexp::Number(Number::Integer(42.into()));
        let result = eval(&num, &mut env, &mut interner).unwrap();
        assert_eq!(result, num);
    }
}
