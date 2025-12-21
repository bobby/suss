//! WASM Component Model bindings for suss-eval
//!
//! This module implements the evaluator WIT interface.

use std::cell::RefCell;

wit_bindgen::generate!({
    world: "evaluator-world",
    path: "../../wit",
});

use crate::Runtime;
use suss_core::{Sexp as InternalSexp, print_sexp};

// Imported types from reader component
use suss::lang::types::{Sexp as WitSexp, EvalError};
use suss::lang::reader;

// Exported evaluator trait
use exports::suss::lang::evaluator::Guest as EvaluatorGuest;

// Global state for the evaluator component
thread_local! {
    static RUNTIME: RefCell<Runtime> = RefCell::new(Runtime::new());
}

struct Component;

impl EvaluatorGuest for Component {
    fn eval(expr: &WitSexp) -> Result<WitSexp, EvalError> {
        RUNTIME.with(|runtime| {
            let mut runtime = runtime.borrow_mut();

            // Get the string representation of the input sexp
            let input_str = expr.to_string();

            // Evaluate using the runtime's eval_string method
            match runtime.eval_string(&input_str) {
                Ok(result) => {
                    // Convert result back to WIT sexp using reader's factory functions
                    Ok(internal_sexp_to_wit(&result, &runtime.interner))
                }
                Err(msg) => Err(EvalError {
                    message: msg,
                    backtrace: vec![],
                }),
            }
        })
    }

    fn eval_to_string(expr: &WitSexp) -> Result<String, EvalError> {
        RUNTIME.with(|runtime| {
            let mut runtime = runtime.borrow_mut();

            // Get the string representation of the input sexp
            let input_str = expr.to_string();

            // Evaluate using the runtime's eval_string method
            match runtime.eval_string(&input_str) {
                Ok(result) => {
                    Ok(print_sexp(&result, &runtime.interner))
                }
                Err(msg) => Err(EvalError {
                    message: msg,
                    backtrace: vec![],
                }),
            }
        })
    }

    fn define(name: String, value: &WitSexp) -> Result<WitSexp, EvalError> {
        RUNTIME.with(|runtime| {
            let mut runtime = runtime.borrow_mut();

            // Parse the value sexp
            let value_str = value.to_string();
            let mut state = suss_reader::ParserState::new("suss");
            state.interner = std::mem::take(&mut runtime.interner);

            let parsed = match suss_reader::parse_and_intern(&value_str, &mut state) {
                Ok(s) => s,
                Err(e) => {
                    runtime.interner = state.interner;
                    return Err(EvalError {
                        message: e.to_string(),
                        backtrace: vec![],
                    });
                }
            };

            runtime.interner = state.interner;

            // Get or create the symbol for the name
            let sym_id = runtime.interner.intern_symbol(&name);

            // Define in environment
            runtime.env.define(sym_id, parsed.clone());

            // Return the defined value
            Ok(internal_sexp_to_wit(&parsed, &runtime.interner))
        })
    }

    fn lookup(name: String) -> Option<WitSexp> {
        RUNTIME.with(|runtime| {
            let mut runtime = runtime.borrow_mut();
            // Intern the symbol name to get its ID (creates it if it doesn't exist)
            let sym_id = runtime.interner.intern_symbol(&name);
            // Try to look it up in the environment
            let value = runtime.env.lookup(sym_id)?;
            Some(internal_sexp_to_wit(value, &runtime.interner))
        })
    }

    fn reset() {
        RUNTIME.with(|runtime| {
            *runtime.borrow_mut() = Runtime::new();
        })
    }
}

// Convert internal Sexp to WIT Sexp using reader's factory functions
fn internal_sexp_to_wit(sexp: &InternalSexp, interner: &suss_core::Interner) -> WitSexp {
    match sexp {
        InternalSexp::Nil => reader::make_nil(),
        InternalSexp::Bool(b) => reader::make_bool(*b),
        InternalSexp::Symbol(id) => reader::make_symbol(id.0),
        InternalSexp::Keyword(id) => reader::make_keyword(id.0),
        InternalSexp::Number(n) => {
            let wit_num = internal_number_to_wit(n);
            reader::make_number(&wit_num)
        }
        InternalSexp::Char(c) => reader::make_char(*c),
        InternalSexp::String(s) => reader::make_string(s),
        InternalSexp::List(items) => {
            let wit_items: Vec<WitSexp> = items
                .iter()
                .map(|s| internal_sexp_to_wit(s, interner))
                .collect();
            reader::make_list(wit_items)
        }
        InternalSexp::Vector(items) => {
            let wit_items: Vec<WitSexp> = items
                .iter()
                .map(|s| internal_sexp_to_wit(s, interner))
                .collect();
            reader::make_vector(wit_items)
        }
        // For types that don't have factory functions, fall back to string representation
        InternalSexp::Set(_) | InternalSexp::Map(_) | InternalSexp::ReaderConditional(_)
        | InternalSexp::Primitive(_) | InternalSexp::Function { .. } => {
            let s = print_sexp(sexp, interner);
            reader::make_string(&s)
        }
    }
}

fn internal_number_to_wit(n: &suss_core::Number) -> suss::lang::types::Number {
    use suss::lang::types::{Number as WitNumber, Bigint, Ratio};

    match n {
        suss_core::Number::Integer(i) => {
            WitNumber::Integer(bigint_to_wit(i))
        }
        suss_core::Number::Ratio(r) => {
            WitNumber::Ratio(Ratio {
                numer: bigint_to_wit(r.numer()),
                denom: bigint_to_wit(r.denom()),
            })
        }
        suss_core::Number::Float(f) => WitNumber::Float(*f),
    }
}

fn bigint_to_wit(n: &num_bigint::BigInt) -> suss::lang::types::Bigint {
    let (sign, bytes) = n.to_bytes_le();
    suss::lang::types::Bigint {
        bytes,
        negative: sign == num_bigint::Sign::Minus,
    }
}

export!(Component);
