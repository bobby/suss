//! Primitive functions for Suss

use suss_core::{Env, Interner, Number, Sexp};
use num_bigint::BigInt;
use num_traits::Zero;

/// Load primitive functions into an environment
pub fn load_primitives(interner: &mut Interner) -> Env {
    let mut env = Env::new();

    let primitives = [
        "+", "-", "*", "/", "=", "<", ">", "<=", ">=",
        "inc", "dec", "zero?", "pos?", "neg?",
        "str", "pr-str", "println", "print",
        "list", "vector", "hash-map", "hash-set",
        "first", "rest", "cons", "conj",
        "count", "empty?", "nil?",
        "not", "and", "or",
    ];

    for name in primitives {
        let sym_id = interner.intern_symbol(name);
        env.define_global(sym_id, Sexp::Primitive(name.to_string()));
    }

    env
}

/// Apply a primitive function
pub fn apply_primitive(name: &str, args: &[Sexp]) -> Result<Sexp, String> {
    match name {
        "+" => numeric_fold(args, BigInt::zero(), |a, b| a + b),
        "-" => {
            if args.is_empty() {
                return Err("-: requires at least 1 argument".to_string());
            }
            if args.len() == 1 {
                // Unary minus
                match &args[0] {
                    Sexp::Number(Number::Integer(n)) => {
                        Ok(Sexp::Number(Number::Integer(-n.clone())))
                    }
                    _ => Err("-: expected number".to_string()),
                }
            } else {
                // Binary minus and beyond
                let first = get_integer(&args[0])?;
                let rest_sum = args[1..]
                    .iter()
                    .try_fold(BigInt::zero(), |acc, x| {
                        get_integer(x).map(|n| acc + n)
                    })?;
                Ok(Sexp::Number(Number::Integer(first - rest_sum)))
            }
        }
        "*" => numeric_fold(args, BigInt::from(1), |a, b| a * b),
        "/" => {
            if args.len() < 2 {
                return Err("/: requires at least 2 arguments".to_string());
            }
            let mut result = get_integer(&args[0])?;
            for arg in &args[1..] {
                let n = get_integer(arg)?;
                if n.is_zero() {
                    return Err("Division by zero".to_string());
                }
                result = result / n;
            }
            Ok(Sexp::Number(Number::Integer(result)))
        }

        "=" => {
            if args.len() < 2 {
                return Ok(Sexp::Bool(true));
            }
            let all_eq = args.windows(2).all(|w| sexp_eq(&w[0], &w[1]));
            Ok(Sexp::Bool(all_eq))
        }

        "<" => compare_numbers(args, |a, b| a < b),
        ">" => compare_numbers(args, |a, b| a > b),
        "<=" => compare_numbers(args, |a, b| a <= b),
        ">=" => compare_numbers(args, |a, b| a >= b),

        "inc" => {
            if args.len() != 1 {
                return Err("inc: requires exactly 1 argument".to_string());
            }
            let n = get_integer(&args[0])?;
            Ok(Sexp::Number(Number::Integer(n + 1)))
        }

        "dec" => {
            if args.len() != 1 {
                return Err("dec: requires exactly 1 argument".to_string());
            }
            let n = get_integer(&args[0])?;
            Ok(Sexp::Number(Number::Integer(n - 1)))
        }

        "zero?" => {
            if args.len() != 1 {
                return Err("zero?: requires exactly 1 argument".to_string());
            }
            match &args[0] {
                Sexp::Number(n) => Ok(Sexp::Bool(n.is_zero())),
                _ => Err("zero?: expected number".to_string()),
            }
        }

        "nil?" => {
            if args.len() != 1 {
                return Err("nil?: requires exactly 1 argument".to_string());
            }
            Ok(Sexp::Bool(args[0].is_nil()))
        }

        "not" => {
            if args.len() != 1 {
                return Err("not: requires exactly 1 argument".to_string());
            }
            Ok(Sexp::Bool(!args[0].is_truthy()))
        }

        "list" => Ok(Sexp::List(args.to_vec())),
        "vector" => Ok(Sexp::Vector(args.to_vec())),

        "first" => {
            if args.len() != 1 {
                return Err("first: requires exactly 1 argument".to_string());
            }
            match &args[0] {
                Sexp::List(items) | Sexp::Vector(items) => {
                    Ok(items.first().cloned().unwrap_or(Sexp::Nil))
                }
                Sexp::Nil => Ok(Sexp::Nil),
                _ => Err("first: expected sequence".to_string()),
            }
        }

        "rest" => {
            if args.len() != 1 {
                return Err("rest: requires exactly 1 argument".to_string());
            }
            match &args[0] {
                Sexp::List(items) => {
                    Ok(Sexp::List(items.get(1..).unwrap_or(&[]).to_vec()))
                }
                Sexp::Vector(items) => {
                    Ok(Sexp::List(items.get(1..).unwrap_or(&[]).to_vec()))
                }
                Sexp::Nil => Ok(Sexp::List(vec![])),
                _ => Err("rest: expected sequence".to_string()),
            }
        }

        "cons" => {
            if args.len() != 2 {
                return Err("cons: requires exactly 2 arguments".to_string());
            }
            match &args[1] {
                Sexp::List(items) => {
                    let mut result = vec![args[0].clone()];
                    result.extend(items.clone());
                    Ok(Sexp::List(result))
                }
                Sexp::Nil => Ok(Sexp::List(vec![args[0].clone()])),
                _ => Err("cons: second argument must be a list or nil".to_string()),
            }
        }

        "count" => {
            if args.len() != 1 {
                return Err("count: requires exactly 1 argument".to_string());
            }
            match &args[0] {
                Sexp::List(items) | Sexp::Vector(items) | Sexp::Set(items) => {
                    Ok(Sexp::Number(Number::Integer(BigInt::from(items.len()))))
                }
                Sexp::Map(pairs) => {
                    Ok(Sexp::Number(Number::Integer(BigInt::from(pairs.len()))))
                }
                Sexp::String(s) => {
                    Ok(Sexp::Number(Number::Integer(BigInt::from(s.len()))))
                }
                Sexp::Nil => Ok(Sexp::Number(Number::Integer(BigInt::zero()))),
                _ => Err("count: expected collection".to_string()),
            }
        }

        "empty?" => {
            if args.len() != 1 {
                return Err("empty?: requires exactly 1 argument".to_string());
            }
            match &args[0] {
                Sexp::List(items) | Sexp::Vector(items) | Sexp::Set(items) => {
                    Ok(Sexp::Bool(items.is_empty()))
                }
                Sexp::Map(pairs) => Ok(Sexp::Bool(pairs.is_empty())),
                Sexp::String(s) => Ok(Sexp::Bool(s.is_empty())),
                Sexp::Nil => Ok(Sexp::Bool(true)),
                _ => Err("empty?: expected collection".to_string()),
            }
        }

        "str" => {
            let result: String = args
                .iter()
                .map(|arg| sexp_to_string(arg))
                .collect();
            Ok(Sexp::String(result))
        }

        _ => Err(format!("Unknown primitive: {}", name)),
    }
}

fn get_integer(sexp: &Sexp) -> Result<BigInt, String> {
    match sexp {
        Sexp::Number(Number::Integer(n)) => Ok(n.clone()),
        _ => Err("Expected integer".to_string()),
    }
}

fn numeric_fold<F>(args: &[Sexp], init: BigInt, f: F) -> Result<Sexp, String>
where
    F: Fn(BigInt, BigInt) -> BigInt,
{
    let result = args.iter().try_fold(init, |acc, x| {
        get_integer(x).map(|n| f(acc, n))
    })?;
    Ok(Sexp::Number(Number::Integer(result)))
}

fn compare_numbers<F>(args: &[Sexp], cmp: F) -> Result<Sexp, String>
where
    F: Fn(&BigInt, &BigInt) -> bool,
{
    if args.len() < 2 {
        return Ok(Sexp::Bool(true));
    }

    let nums: Result<Vec<BigInt>, _> = args.iter().map(get_integer).collect();
    let nums = nums?;

    let result = nums.windows(2).all(|w| cmp(&w[0], &w[1]));
    Ok(Sexp::Bool(result))
}

fn sexp_eq(a: &Sexp, b: &Sexp) -> bool {
    match (a, b) {
        (Sexp::Nil, Sexp::Nil) => true,
        (Sexp::Bool(a), Sexp::Bool(b)) => a == b,
        (Sexp::Number(a), Sexp::Number(b)) => a == b,
        (Sexp::Char(a), Sexp::Char(b)) => a == b,
        (Sexp::String(a), Sexp::String(b)) => a == b,
        (Sexp::Symbol(a), Sexp::Symbol(b)) => a == b,
        (Sexp::Keyword(a), Sexp::Keyword(b)) => a == b,
        (Sexp::List(a), Sexp::List(b)) | (Sexp::Vector(a), Sexp::Vector(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| sexp_eq(x, y))
        }
        _ => false,
    }
}

fn sexp_to_string(sexp: &Sexp) -> String {
    match sexp {
        Sexp::Nil => "nil".to_string(),
        Sexp::Bool(true) => "true".to_string(),
        Sexp::Bool(false) => "false".to_string(),
        Sexp::Number(n) => n.to_string(),
        Sexp::Char(c) => c.to_string(),
        Sexp::String(s) => s.clone(),
        Sexp::Symbol(_) => "<symbol>".to_string(),
        Sexp::Keyword(_) => "<keyword>".to_string(),
        Sexp::List(_) => "<list>".to_string(),
        Sexp::Vector(_) => "<vector>".to_string(),
        Sexp::Set(_) => "<set>".to_string(),
        Sexp::Map(_) => "<map>".to_string(),
        Sexp::ReaderConditional(_) => "<reader-conditional>".to_string(),
        Sexp::Primitive(name) => format!("#<primitive:{}>", name),
        Sexp::Function { .. } => "#<function>".to_string(),
    }
}
