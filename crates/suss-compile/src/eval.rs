//! Compile-time macro evaluator
//!
//! A minimal tree-walking interpreter for evaluating macro bodies at compile time.
//! This evaluator operates on EDN values and produces EDN values.

use std::collections::HashMap;
use suss_core::{Edn, Env};
use crate::error::{CompileError, CompileResult};

/// Compile-time macro evaluator
pub struct MacroEvaluator {
    /// Built-in primitive functions
    primitives: HashMap<String, Primitive>,
}

/// A primitive function that operates on EDN values
type Primitive = fn(&[Edn]) -> CompileResult<Edn>;

impl MacroEvaluator {
    /// Create a new macro evaluator with built-in primitives
    pub fn new() -> Self {
        let mut primitives: HashMap<String, Primitive> = HashMap::new();

        // Collection constructors
        primitives.insert("list".into(), prim_list);
        primitives.insert("vector".into(), prim_vector);

        // Sequence operations
        primitives.insert("cons".into(), prim_cons);
        primitives.insert("conj".into(), prim_conj);
        primitives.insert("first".into(), prim_first);
        primitives.insert("rest".into(), prim_rest);
        primitives.insert("next".into(), prim_next);
        primitives.insert("seq".into(), prim_seq);
        primitives.insert("second".into(), prim_second);
        primitives.insert("last".into(), prim_last);
        primitives.insert("nnext".into(), prim_nnext);
        primitives.insert("concat".into(), prim_concat);
        primitives.insert("count".into(), prim_count);
        primitives.insert("nth".into(), prim_nth);
        primitives.insert("get".into(), prim_get);

        // Predicates
        primitives.insert("nil?".into(), prim_nil_q);
        primitives.insert("empty?".into(), prim_empty_q);
        primitives.insert("seq?".into(), prim_seq_q);
        primitives.insert("list?".into(), prim_list_q);
        primitives.insert("vector?".into(), prim_vector_q);
        primitives.insert("map?".into(), prim_map_q);
        primitives.insert("symbol?".into(), prim_symbol_q);
        primitives.insert("keyword?".into(), prim_keyword_q);
        primitives.insert("string?".into(), prim_string_q);
        primitives.insert("number?".into(), prim_number_q);

        // Equality and comparison
        primitives.insert("=".into(), prim_eq);
        primitives.insert("not=".into(), prim_not_eq);
        primitives.insert("not".into(), prim_not);

        // Arithmetic (for macro-time computation)
        primitives.insert("+".into(), prim_add);
        primitives.insert("-".into(), prim_sub);
        primitives.insert("*".into(), prim_mul);
        primitives.insert("<".into(), prim_lt);
        primitives.insert(">".into(), prim_gt);
        primitives.insert("<=".into(), prim_le);
        primitives.insert(">=".into(), prim_ge);
        primitives.insert("inc".into(), prim_inc);
        primitives.insert("dec".into(), prim_dec);

        // String operations
        primitives.insert("str".into(), prim_str);
        primitives.insert("name".into(), prim_name);
        primitives.insert("symbol".into(), prim_symbol);
        primitives.insert("keyword".into(), prim_keyword);

        // Apply
        primitives.insert("apply".into(), prim_apply);

        // Macro expansion helpers (produce code, not values)
        primitives.insert("_and_impl".into(), prim_and_impl);
        primitives.insert("_or_impl".into(), prim_or_impl);
        primitives.insert("_cond_impl".into(), prim_cond_impl);
        primitives.insert("_case_impl".into(), prim_case_impl);
        primitives.insert("_thread_first_impl".into(), prim_thread_first_impl);
        primitives.insert("_thread_last_impl".into(), prim_thread_last_impl);
        primitives.insert("_when_let_impl".into(), prim_when_let_impl);
        primitives.insert("_if_let_impl".into(), prim_if_let_impl);

        Self { primitives }
    }

    /// Evaluate an EDN expression in the given environment
    pub fn eval(&self, expr: &Edn, env: &mut Env) -> CompileResult<Edn> {
        match expr {
            // Self-evaluating forms
            Edn::Nil | Edn::Bool(_) | Edn::Number(_) |
            Edn::String(_) | Edn::Keyword(_) => Ok(expr.clone()),

            // Symbol lookup
            Edn::Symbol(sym) => {
                let name = if let Some(ns) = &sym.namespace {
                    format!("{}/{}", ns, sym.name)
                } else {
                    sym.name.clone()
                };

                // Check for primitive
                if self.primitives.contains_key(&name) {
                    // Return a marker for the primitive
                    Ok(Edn::Primitive(name))
                } else if let Some(value) = env.lookup(&name) {
                    Ok(value.clone())
                } else {
                    Err(CompileError::MacroEval(format!(
                        "Undefined symbol in macro: {}", name
                    )))
                }
            }

            // Empty list evaluates to empty list
            Edn::List(items) if items.is_empty() => Ok(Edn::List(vec![])),

            // List - check for special forms or function call
            Edn::List(items) => {
                let first = &items[0];

                // Check for special forms
                if let Edn::Symbol(sym) = first {
                    match sym.name.as_str() {
                        "quote" => return self.eval_quote(&items[1..]),
                        "if" => return self.eval_if(&items[1..], env),
                        "let" => return self.eval_let(&items[1..], env),
                        "do" => return self.eval_do(&items[1..], env),
                        "fn" => return self.make_closure(&items[1..], env),
                        "loop" => return self.eval_loop(&items[1..], env),
                        _ => {}
                    }
                }

                // Function/closure call
                self.eval_call(first, &items[1..], env)
            }

            // Vector - evaluate elements
            Edn::Vector(items) => {
                let evaled: Vec<Edn> = items.iter()
                    .map(|e| self.eval(e, env))
                    .collect::<CompileResult<_>>()?;
                Ok(Edn::Vector(evaled))
            }

            // Map - evaluate keys and values
            Edn::Map(pairs) => {
                let evaled: Vec<(Edn, Edn)> = pairs.iter()
                    .map(|(k, v)| Ok((self.eval(k, env)?, self.eval(v, env)?)))
                    .collect::<CompileResult<_>>()?;
                Ok(Edn::Map(evaled))
            }

            // Set - evaluate elements
            Edn::Set(items) => {
                let evaled: Vec<Edn> = items.iter()
                    .map(|e| self.eval(e, env))
                    .collect::<CompileResult<_>>()?;
                Ok(Edn::Set(evaled))
            }

            // Function value (from fn form)
            Edn::Function { .. } => Ok(expr.clone()),

            // Primitive reference
            Edn::Primitive(_) => Ok(expr.clone()),

            _ => Err(CompileError::MacroEval(format!(
                "Cannot evaluate in macro context: {:?}", expr
            ))),
        }
    }

    /// Evaluate quote - return argument unevaluated
    fn eval_quote(&self, args: &[Edn]) -> CompileResult<Edn> {
        if args.is_empty() {
            Ok(Edn::Nil)
        } else {
            Ok(args[0].clone())
        }
    }

    /// Evaluate if expression
    fn eval_if(&self, args: &[Edn], env: &mut Env) -> CompileResult<Edn> {
        if args.len() < 2 {
            return Err(CompileError::MacroEval("if requires at least 2 arguments".into()));
        }

        let cond = self.eval(&args[0], env)?;

        if is_truthy(&cond) {
            self.eval(&args[1], env)
        } else if args.len() > 2 {
            self.eval(&args[2], env)
        } else {
            Ok(Edn::Nil)
        }
    }

    /// Evaluate let expression
    fn eval_let(&self, args: &[Edn], env: &mut Env) -> CompileResult<Edn> {
        if args.is_empty() {
            return Err(CompileError::MacroEval("let requires bindings".into()));
        }

        let bindings = match &args[0] {
            Edn::Vector(v) => v,
            _ => return Err(CompileError::MacroEval("let bindings must be a vector".into())),
        };

        if bindings.len() % 2 != 0 {
            return Err(CompileError::MacroEval("let bindings must have even count".into()));
        }

        let mut inner_env = env.clone();
        inner_env.push_scope();

        for pair in bindings.chunks(2) {
            let name = match &pair[0] {
                Edn::Symbol(sym) => sym.name.clone(),
                _ => return Err(CompileError::MacroEval("let binding name must be symbol".into())),
            };
            let value = self.eval(&pair[1], &mut inner_env)?;
            inner_env.define(&name, value);
        }

        // Evaluate body
        if args.len() == 1 {
            Ok(Edn::Nil)
        } else {
            self.eval_do(&args[1..], &mut inner_env)
        }
    }

    /// Evaluate do expression (sequence)
    fn eval_do(&self, args: &[Edn], env: &mut Env) -> CompileResult<Edn> {
        let mut result = Edn::Nil;
        for arg in args {
            result = self.eval(arg, env)?;
        }
        Ok(result)
    }

    /// Create a closure (fn form)
    fn make_closure(&self, args: &[Edn], env: &Env) -> CompileResult<Edn> {
        if args.is_empty() {
            return Err(CompileError::MacroEval("fn requires parameters".into()));
        }

        let params = match &args[0] {
            Edn::Vector(v) => {
                v.iter().map(|p| match p {
                    Edn::Symbol(sym) => Ok(sym.clone()),
                    _ => Err(CompileError::MacroEval("fn parameter must be symbol".into())),
                }).collect::<CompileResult<Vec<_>>>()?
            }
            _ => return Err(CompileError::MacroEval("fn parameters must be a vector".into())),
        };

        let body = if args.len() == 2 {
            Box::new(args[1].clone())
        } else {
            // Wrap multiple body forms in do
            let mut do_forms = vec![Edn::Symbol(suss_core::Symbol::new("do"))];
            do_forms.extend(args[1..].iter().cloned());
            Box::new(Edn::List(do_forms))
        };

        Ok(Edn::Function {
            params,
            body,
            env: Box::new(env.clone()),
        })
    }

    /// Evaluate loop/recur
    fn eval_loop(&self, args: &[Edn], env: &mut Env) -> CompileResult<Edn> {
        if args.is_empty() {
            return Err(CompileError::MacroEval("loop requires bindings".into()));
        }

        let bindings = match &args[0] {
            Edn::Vector(v) => v,
            _ => return Err(CompileError::MacroEval("loop bindings must be a vector".into())),
        };

        if bindings.len() % 2 != 0 {
            return Err(CompileError::MacroEval("loop bindings must have even count".into()));
        }

        // Extract binding names and initial values
        let mut binding_names = Vec::new();
        let mut current_values = Vec::new();

        for pair in bindings.chunks(2) {
            let name = match &pair[0] {
                Edn::Symbol(sym) => sym.name.clone(),
                _ => return Err(CompileError::MacroEval("loop binding name must be symbol".into())),
            };
            binding_names.push(name);
            current_values.push(self.eval(&pair[1], env)?);
        }

        let body = &args[1..];

        loop {
            let mut loop_env = env.clone();
            loop_env.push_scope();
            for (name, value) in binding_names.iter().zip(current_values.iter()) {
                loop_env.define(name, value.clone());
            }

            match self.eval_loop_body(body, &mut loop_env)? {
                LoopResult::Return(val) => return Ok(val),
                LoopResult::Recur(new_values) => {
                    if new_values.len() != binding_names.len() {
                        return Err(CompileError::MacroEval(format!(
                            "recur expected {} values, got {}",
                            binding_names.len(),
                            new_values.len()
                        )));
                    }
                    current_values = new_values;
                }
            }
        }
    }

    /// Evaluate loop body, detecting recur
    fn eval_loop_body(&self, body: &[Edn], env: &mut Env) -> CompileResult<LoopResult> {
        for (i, expr) in body.iter().enumerate() {
            let is_last = i == body.len() - 1;

            if is_last {
                // Check for recur in tail position
                if let Edn::List(items) = expr {
                    if !items.is_empty() {
                        if let Edn::Symbol(sym) = &items[0] {
                            if sym.name == "recur" {
                                let new_values: Vec<Edn> = items[1..].iter()
                                    .map(|e| self.eval(e, env))
                                    .collect::<CompileResult<_>>()?;
                                return Ok(LoopResult::Recur(new_values));
                            }
                        }
                    }
                }
            }

            let result = self.eval(expr, env)?;

            if is_last {
                return Ok(LoopResult::Return(result));
            }
        }

        Ok(LoopResult::Return(Edn::Nil))
    }

    /// Evaluate a function call
    fn eval_call(&self, func_expr: &Edn, args: &[Edn], env: &mut Env) -> CompileResult<Edn> {
        let func = self.eval(func_expr, env)?;

        match func {
            Edn::Primitive(name) => {
                // Evaluate arguments for primitives
                let evaled_args: Vec<Edn> = args.iter()
                    .map(|a| self.eval(a, env))
                    .collect::<CompileResult<_>>()?;

                if name == "apply" {
                    // apply is special - needs access to evaluator for the function
                    self.call_apply(&evaled_args, env)
                } else if let Some(prim) = self.primitives.get(&name) {
                    prim(&evaled_args)
                } else {
                    Err(CompileError::MacroEval(format!("Unknown primitive: {}", name)))
                }
            }

            Edn::Function { params, body, env: closure_env } => {
                // Evaluate arguments
                let evaled_args: Vec<Edn> = args.iter()
                    .map(|a| self.eval(a, env))
                    .collect::<CompileResult<_>>()?;

                self.call_function(params, *body, *closure_env, evaled_args)
            }

            _ => Err(CompileError::MacroEval(format!(
                "Cannot call as function: {:?}", func
            ))),
        }
    }

    /// Call a user-defined function
    fn call_function(
        &self,
        params: Vec<suss_core::Symbol>,
        body: Edn,
        closure_env: Env,
        args: Vec<Edn>,
    ) -> CompileResult<Edn> {
        let mut fn_env = closure_env.clone();
        fn_env.push_scope();

        // Check for rest parameter (&)
        let mut rest_idx = None;
        for (i, param) in params.iter().enumerate() {
            if param.name == "&" {
                rest_idx = Some(i);
                break;
            }
        }

        if let Some(idx) = rest_idx {
            // Bind regular params
            for (i, param) in params[..idx].iter().enumerate() {
                let value = args.get(i).cloned().unwrap_or(Edn::Nil);
                fn_env.define(&param.name, value);
            }

            // Bind rest param (the param after &)
            if idx + 1 < params.len() {
                let rest_name = &params[idx + 1].name;
                let rest_args: Vec<Edn> = args.into_iter().skip(idx).collect();
                fn_env.define(rest_name, Edn::List(rest_args));
            }
        } else {
            // No rest parameter - bind all params
            for (i, param) in params.iter().enumerate() {
                let value = args.get(i).cloned().unwrap_or(Edn::Nil);
                fn_env.define(&param.name, value);
            }
        }

        self.eval(&body, &mut fn_env)
    }

    /// Call apply - (apply f args) or (apply f a b [more])
    fn call_apply(&self, args: &[Edn], env: &mut Env) -> CompileResult<Edn> {
        if args.is_empty() {
            return Err(CompileError::MacroEval("apply requires a function".into()));
        }

        let func = &args[0];

        // Collect all arguments, spreading the last one if it's a sequence
        let mut call_args = Vec::new();

        for (i, arg) in args[1..].iter().enumerate() {
            if i == args.len() - 2 {
                // Last argument - spread if it's a sequence
                match arg {
                    Edn::List(items) | Edn::Vector(items) => {
                        call_args.extend(items.iter().cloned());
                    }
                    Edn::Nil => {}
                    _ => call_args.push(arg.clone()),
                }
            } else {
                call_args.push(arg.clone());
            }
        }

        // Now call the function with the collected args
        match func {
            Edn::Primitive(name) => {
                if let Some(prim) = self.primitives.get(name) {
                    prim(&call_args)
                } else {
                    Err(CompileError::MacroEval(format!("Unknown primitive: {}", name)))
                }
            }

            Edn::Function { params, body, env: closure_env } => {
                self.call_function(params.clone(), *body.clone(), *closure_env.clone(), call_args)
            }

            _ => Err(CompileError::MacroEval(format!(
                "apply: first arg must be a function, got {:?}", func
            ))),
        }
    }
}

impl Default for MacroEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of evaluating a loop body
enum LoopResult {
    Return(Edn),
    Recur(Vec<Edn>),
}

/// Check if a value is truthy (not nil and not false)
fn is_truthy(val: &Edn) -> bool {
    !matches!(val, Edn::Nil | Edn::Bool(false))
}

// ============================================================================
// Primitive Functions
// ============================================================================

fn prim_list(args: &[Edn]) -> CompileResult<Edn> {
    Ok(Edn::List(args.to_vec()))
}

fn prim_vector(args: &[Edn]) -> CompileResult<Edn> {
    Ok(Edn::Vector(args.to_vec()))
}

fn prim_cons(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() != 2 {
        return Err(CompileError::MacroEval("cons requires 2 arguments".into()));
    }

    let first = args[0].clone();
    let rest = match &args[1] {
        Edn::List(items) => items.clone(),
        Edn::Vector(items) => items.clone(),
        Edn::Nil => vec![],
        _ => return Err(CompileError::MacroEval("cons: second arg must be a sequence".into())),
    };

    let mut result = vec![first];
    result.extend(rest);
    Ok(Edn::List(result))
}

fn prim_conj(args: &[Edn]) -> CompileResult<Edn> {
    if args.is_empty() {
        return Err(CompileError::MacroEval("conj requires at least 1 argument".into()));
    }

    match &args[0] {
        Edn::List(items) => {
            // For lists, conj adds to front
            let mut result = args[1..].to_vec();
            result.reverse();
            result.extend(items.iter().cloned());

            // Actually, Clojure conj on list adds to FRONT
            let mut new_items = items.clone();
            for item in args[1..].iter().rev() {
                new_items.insert(0, item.clone());
            }
            Ok(Edn::List(new_items))
        }
        Edn::Vector(items) => {
            // For vectors, conj adds to end
            let mut result = items.clone();
            result.extend(args[1..].iter().cloned());
            Ok(Edn::Vector(result))
        }
        Edn::Nil => {
            // Treat nil as empty list
            Ok(Edn::List(args[1..].to_vec()))
        }
        _ => Err(CompileError::MacroEval("conj: first arg must be a collection".into())),
    }
}

fn prim_first(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() != 1 {
        return Err(CompileError::MacroEval("first requires 1 argument".into()));
    }

    match &args[0] {
        Edn::List(items) | Edn::Vector(items) => {
            Ok(items.first().cloned().unwrap_or(Edn::Nil))
        }
        Edn::Nil => Ok(Edn::Nil),
        _ => Err(CompileError::MacroEval("first: arg must be a sequence".into())),
    }
}

fn prim_rest(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() != 1 {
        return Err(CompileError::MacroEval("rest requires 1 argument".into()));
    }

    match &args[0] {
        Edn::List(items) | Edn::Vector(items) => {
            if items.len() <= 1 {
                Ok(Edn::List(vec![]))
            } else {
                Ok(Edn::List(items[1..].to_vec()))
            }
        }
        Edn::Nil => Ok(Edn::List(vec![])),
        _ => Err(CompileError::MacroEval("rest: arg must be a sequence".into())),
    }
}

fn prim_next(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() != 1 {
        return Err(CompileError::MacroEval("next requires 1 argument".into()));
    }

    match &args[0] {
        Edn::List(items) | Edn::Vector(items) => {
            if items.len() <= 1 {
                Ok(Edn::Nil)
            } else {
                Ok(Edn::List(items[1..].to_vec()))
            }
        }
        Edn::Nil => Ok(Edn::Nil),
        _ => Err(CompileError::MacroEval("next: arg must be a sequence".into())),
    }
}

fn prim_seq(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() != 1 {
        return Err(CompileError::MacroEval("seq requires 1 argument".into()));
    }

    match &args[0] {
        Edn::List(items) | Edn::Vector(items) => {
            if items.is_empty() {
                Ok(Edn::Nil)
            } else {
                Ok(Edn::List(items.clone()))
            }
        }
        Edn::Nil => Ok(Edn::Nil),
        Edn::String(s) => {
            if s.is_empty() {
                Ok(Edn::Nil)
            } else {
                let chars: Vec<Edn> = s.chars()
                    .map(|c| Edn::String(c.to_string()))
                    .collect();
                Ok(Edn::List(chars))
            }
        }
        _ => Err(CompileError::MacroEval("seq: arg must be a seqable".into())),
    }
}

fn prim_second(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() != 1 {
        return Err(CompileError::MacroEval("second requires 1 argument".into()));
    }

    match &args[0] {
        Edn::List(items) | Edn::Vector(items) => {
            Ok(items.get(1).cloned().unwrap_or(Edn::Nil))
        }
        Edn::Nil => Ok(Edn::Nil),
        _ => Err(CompileError::MacroEval("second: arg must be a sequence".into())),
    }
}

fn prim_last(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() != 1 {
        return Err(CompileError::MacroEval("last requires 1 argument".into()));
    }

    match &args[0] {
        Edn::List(items) | Edn::Vector(items) => {
            Ok(items.last().cloned().unwrap_or(Edn::Nil))
        }
        Edn::Nil => Ok(Edn::Nil),
        _ => Err(CompileError::MacroEval("last: arg must be a sequence".into())),
    }
}

fn prim_nnext(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() != 1 {
        return Err(CompileError::MacroEval("nnext requires 1 argument".into()));
    }

    match &args[0] {
        Edn::List(items) | Edn::Vector(items) => {
            if items.len() <= 2 {
                Ok(Edn::Nil)
            } else {
                Ok(Edn::List(items[2..].to_vec()))
            }
        }
        Edn::Nil => Ok(Edn::Nil),
        _ => Err(CompileError::MacroEval("nnext: arg must be a sequence".into())),
    }
}

fn prim_concat(args: &[Edn]) -> CompileResult<Edn> {
    let mut result = Vec::new();

    for arg in args {
        match arg {
            Edn::List(items) | Edn::Vector(items) => {
                result.extend(items.iter().cloned());
            }
            Edn::Nil => {}
            _ => return Err(CompileError::MacroEval("concat: args must be sequences".into())),
        }
    }

    Ok(Edn::List(result))
}

fn prim_count(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() != 1 {
        return Err(CompileError::MacroEval("count requires 1 argument".into()));
    }

    let count = match &args[0] {
        Edn::List(items) | Edn::Vector(items) | Edn::Set(items) => items.len(),
        Edn::Map(pairs) => pairs.len(),
        Edn::String(s) => s.len(),
        Edn::Nil => 0,
        _ => return Err(CompileError::MacroEval("count: arg must be countable".into())),
    };

    Ok(Edn::Number(suss_core::Number::from_i64(count as i64)))
}

fn prim_nth(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() < 2 {
        return Err(CompileError::MacroEval("nth requires at least 2 arguments".into()));
    }

    let idx = match &args[1] {
        Edn::Number(n) => n.to_i64().unwrap_or(0) as usize,
        _ => return Err(CompileError::MacroEval("nth: index must be a number".into())),
    };

    let not_found = args.get(2).cloned();

    match &args[0] {
        Edn::List(items) | Edn::Vector(items) => {
            Ok(items.get(idx).cloned().or(not_found).unwrap_or(Edn::Nil))
        }
        _ => Err(CompileError::MacroEval("nth: first arg must be a sequence".into())),
    }
}

fn prim_get(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() < 2 {
        return Err(CompileError::MacroEval("get requires at least 2 arguments".into()));
    }

    let not_found = args.get(2).cloned().unwrap_or(Edn::Nil);

    match &args[0] {
        Edn::Map(pairs) => {
            for (k, v) in pairs {
                if edn_eq(k, &args[1]) {
                    return Ok(v.clone());
                }
            }
            Ok(not_found)
        }
        Edn::Vector(items) => {
            if let Edn::Number(n) = &args[1] {
                let idx = n.to_i64().unwrap_or(-1);
                if idx >= 0 && (idx as usize) < items.len() {
                    return Ok(items[idx as usize].clone());
                }
            }
            Ok(not_found)
        }
        Edn::Nil => Ok(not_found),
        _ => Ok(not_found),
    }
}

// Predicates

fn prim_nil_q(args: &[Edn]) -> CompileResult<Edn> {
    Ok(Edn::Bool(matches!(args.get(0), Some(Edn::Nil) | None)))
}

fn prim_empty_q(args: &[Edn]) -> CompileResult<Edn> {
    let is_empty = match args.get(0) {
        Some(Edn::List(items)) | Some(Edn::Vector(items)) | Some(Edn::Set(items)) => items.is_empty(),
        Some(Edn::Map(pairs)) => pairs.is_empty(),
        Some(Edn::String(s)) => s.is_empty(),
        Some(Edn::Nil) => true,
        _ => false,
    };
    Ok(Edn::Bool(is_empty))
}

fn prim_seq_q(args: &[Edn]) -> CompileResult<Edn> {
    Ok(Edn::Bool(matches!(args.get(0), Some(Edn::List(_)))))
}

fn prim_list_q(args: &[Edn]) -> CompileResult<Edn> {
    Ok(Edn::Bool(matches!(args.get(0), Some(Edn::List(_)))))
}

fn prim_vector_q(args: &[Edn]) -> CompileResult<Edn> {
    Ok(Edn::Bool(matches!(args.get(0), Some(Edn::Vector(_)))))
}

fn prim_map_q(args: &[Edn]) -> CompileResult<Edn> {
    Ok(Edn::Bool(matches!(args.get(0), Some(Edn::Map(_)))))
}

fn prim_symbol_q(args: &[Edn]) -> CompileResult<Edn> {
    Ok(Edn::Bool(matches!(args.get(0), Some(Edn::Symbol(_)))))
}

fn prim_keyword_q(args: &[Edn]) -> CompileResult<Edn> {
    Ok(Edn::Bool(matches!(args.get(0), Some(Edn::Keyword(_)))))
}

fn prim_string_q(args: &[Edn]) -> CompileResult<Edn> {
    Ok(Edn::Bool(matches!(args.get(0), Some(Edn::String(_)))))
}

fn prim_number_q(args: &[Edn]) -> CompileResult<Edn> {
    Ok(Edn::Bool(matches!(args.get(0), Some(Edn::Number(_)))))
}

// Equality

fn edn_eq(a: &Edn, b: &Edn) -> bool {
    match (a, b) {
        (Edn::Nil, Edn::Nil) => true,
        (Edn::Bool(x), Edn::Bool(y)) => x == y,
        (Edn::Number(x), Edn::Number(y)) => {
            // Compare as f64 for simplicity
            x.to_f64() == y.to_f64()
        }
        (Edn::String(x), Edn::String(y)) => x == y,
        (Edn::Symbol(x), Edn::Symbol(y)) => x.name == y.name && x.namespace == y.namespace,
        (Edn::Keyword(x), Edn::Keyword(y)) => x.name == y.name && x.namespace == y.namespace,
        (Edn::List(x), Edn::List(y)) | (Edn::Vector(x), Edn::Vector(y)) |
        (Edn::List(x), Edn::Vector(y)) | (Edn::Vector(x), Edn::List(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| edn_eq(a, b))
        }
        (Edn::Set(x), Edn::Set(y)) => {
            x.len() == y.len() && x.iter().all(|a| y.iter().any(|b| edn_eq(a, b)))
        }
        (Edn::Map(x), Edn::Map(y)) => {
            if x.len() != y.len() {
                return false;
            }
            for (k1, v1) in x {
                let found = y.iter().any(|(k2, v2)| edn_eq(k1, k2) && edn_eq(v1, v2));
                if !found {
                    return false;
                }
            }
            true
        }
        _ => false,
    }
}

fn prim_eq(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() < 2 {
        return Ok(Edn::Bool(true));
    }

    let first = &args[0];
    for arg in &args[1..] {
        if !edn_eq(first, arg) {
            return Ok(Edn::Bool(false));
        }
    }
    Ok(Edn::Bool(true))
}

fn prim_not_eq(args: &[Edn]) -> CompileResult<Edn> {
    let result = prim_eq(args)?;
    match result {
        Edn::Bool(b) => Ok(Edn::Bool(!b)),
        _ => Ok(result),
    }
}

fn prim_not(args: &[Edn]) -> CompileResult<Edn> {
    if args.is_empty() {
        return Err(CompileError::MacroEval("not requires 1 argument".into()));
    }
    Ok(Edn::Bool(!is_truthy(&args[0])))
}

// Arithmetic

fn prim_add(args: &[Edn]) -> CompileResult<Edn> {
    let mut sum = 0i64;
    let mut is_float = false;
    let mut float_sum = 0.0f64;

    for arg in args {
        match arg {
            Edn::Number(n) => {
                if is_float {
                    float_sum += n.to_f64();
                } else if n.is_float() {
                    is_float = true;
                    float_sum = sum as f64 + n.to_f64();
                } else {
                    sum += n.to_i64().unwrap_or(0);
                }
            }
            _ => return Err(CompileError::MacroEval("+ requires numbers".into())),
        }
    }

    if is_float {
        Ok(Edn::Number(suss_core::Number::from_f64(float_sum)))
    } else {
        Ok(Edn::Number(suss_core::Number::from_i64(sum)))
    }
}

fn prim_sub(args: &[Edn]) -> CompileResult<Edn> {
    if args.is_empty() {
        return Ok(Edn::Number(suss_core::Number::from_i64(0)));
    }

    let first = match &args[0] {
        Edn::Number(n) => n,
        _ => return Err(CompileError::MacroEval("- requires numbers".into())),
    };

    if args.len() == 1 {
        // Negation
        if first.is_float() {
            Ok(Edn::Number(suss_core::Number::from_f64(-first.to_f64())))
        } else {
            Ok(Edn::Number(suss_core::Number::from_i64(-first.to_i64().unwrap_or(0))))
        }
    } else {
        let mut result = first.to_f64();
        let mut is_float = first.is_float();

        for arg in &args[1..] {
            match arg {
                Edn::Number(n) => {
                    result -= n.to_f64();
                    if n.is_float() {
                        is_float = true;
                    }
                }
                _ => return Err(CompileError::MacroEval("- requires numbers".into())),
            }
        }

        if is_float {
            Ok(Edn::Number(suss_core::Number::from_f64(result)))
        } else {
            Ok(Edn::Number(suss_core::Number::from_i64(result as i64)))
        }
    }
}

fn prim_mul(args: &[Edn]) -> CompileResult<Edn> {
    let mut product = 1i64;
    let mut is_float = false;
    let mut float_product = 1.0f64;

    for arg in args {
        match arg {
            Edn::Number(n) => {
                if is_float {
                    float_product *= n.to_f64();
                } else if n.is_float() {
                    is_float = true;
                    float_product = product as f64 * n.to_f64();
                } else {
                    product *= n.to_i64().unwrap_or(1);
                }
            }
            _ => return Err(CompileError::MacroEval("* requires numbers".into())),
        }
    }

    if is_float {
        Ok(Edn::Number(suss_core::Number::from_f64(float_product)))
    } else {
        Ok(Edn::Number(suss_core::Number::from_i64(product)))
    }
}

fn prim_lt(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() < 2 {
        return Ok(Edn::Bool(true));
    }

    for pair in args.windows(2) {
        let a = match &pair[0] {
            Edn::Number(n) => n.to_f64(),
            _ => return Err(CompileError::MacroEval("< requires numbers".into())),
        };
        let b = match &pair[1] {
            Edn::Number(n) => n.to_f64(),
            _ => return Err(CompileError::MacroEval("< requires numbers".into())),
        };
        if !(a < b) {
            return Ok(Edn::Bool(false));
        }
    }
    Ok(Edn::Bool(true))
}

fn prim_gt(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() < 2 {
        return Ok(Edn::Bool(true));
    }

    for pair in args.windows(2) {
        let a = match &pair[0] {
            Edn::Number(n) => n.to_f64(),
            _ => return Err(CompileError::MacroEval("> requires numbers".into())),
        };
        let b = match &pair[1] {
            Edn::Number(n) => n.to_f64(),
            _ => return Err(CompileError::MacroEval("> requires numbers".into())),
        };
        if !(a > b) {
            return Ok(Edn::Bool(false));
        }
    }
    Ok(Edn::Bool(true))
}

fn prim_le(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() < 2 {
        return Ok(Edn::Bool(true));
    }

    for pair in args.windows(2) {
        let a = match &pair[0] {
            Edn::Number(n) => n.to_f64(),
            _ => return Err(CompileError::MacroEval("<= requires numbers".into())),
        };
        let b = match &pair[1] {
            Edn::Number(n) => n.to_f64(),
            _ => return Err(CompileError::MacroEval("<= requires numbers".into())),
        };
        if !(a <= b) {
            return Ok(Edn::Bool(false));
        }
    }
    Ok(Edn::Bool(true))
}

fn prim_ge(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() < 2 {
        return Ok(Edn::Bool(true));
    }

    for pair in args.windows(2) {
        let a = match &pair[0] {
            Edn::Number(n) => n.to_f64(),
            _ => return Err(CompileError::MacroEval(">= requires numbers".into())),
        };
        let b = match &pair[1] {
            Edn::Number(n) => n.to_f64(),
            _ => return Err(CompileError::MacroEval(">= requires numbers".into())),
        };
        if !(a >= b) {
            return Ok(Edn::Bool(false));
        }
    }
    Ok(Edn::Bool(true))
}

fn prim_inc(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() != 1 {
        return Err(CompileError::MacroEval("inc requires 1 argument".into()));
    }

    match &args[0] {
        Edn::Number(n) => {
            if n.is_float() {
                Ok(Edn::Number(suss_core::Number::from_f64(n.to_f64() + 1.0)))
            } else {
                Ok(Edn::Number(suss_core::Number::from_i64(n.to_i64().unwrap_or(0) + 1)))
            }
        }
        _ => Err(CompileError::MacroEval("inc requires a number".into())),
    }
}

fn prim_dec(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() != 1 {
        return Err(CompileError::MacroEval("dec requires 1 argument".into()));
    }

    match &args[0] {
        Edn::Number(n) => {
            if n.is_float() {
                Ok(Edn::Number(suss_core::Number::from_f64(n.to_f64() - 1.0)))
            } else {
                Ok(Edn::Number(suss_core::Number::from_i64(n.to_i64().unwrap_or(0) - 1)))
            }
        }
        _ => Err(CompileError::MacroEval("dec requires a number".into())),
    }
}

// String operations

fn prim_str(args: &[Edn]) -> CompileResult<Edn> {
    let mut result = String::new();

    for arg in args {
        match arg {
            Edn::Nil => result.push_str("nil"),
            Edn::Bool(b) => result.push_str(if *b { "true" } else { "false" }),
            Edn::Number(n) => result.push_str(&format!("{}", n)),
            Edn::String(s) => result.push_str(s),
            Edn::Keyword(k) => result.push_str(&format!(":{}", k)),
            Edn::Symbol(s) => result.push_str(&format!("{}", s)),
            _ => result.push_str(&format!("{:?}", arg)),
        }
    }

    Ok(Edn::String(result))
}

fn prim_name(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() != 1 {
        return Err(CompileError::MacroEval("name requires 1 argument".into()));
    }

    match &args[0] {
        Edn::Keyword(k) => Ok(Edn::String(k.name.clone())),
        Edn::Symbol(s) => Ok(Edn::String(s.name.clone())),
        Edn::String(s) => Ok(Edn::String(s.clone())),
        _ => Err(CompileError::MacroEval("name requires keyword, symbol, or string".into())),
    }
}

fn prim_symbol(args: &[Edn]) -> CompileResult<Edn> {
    match args.len() {
        1 => {
            match &args[0] {
                Edn::String(s) => Ok(Edn::Symbol(suss_core::Symbol::parse(s))),
                Edn::Symbol(s) => Ok(Edn::Symbol(s.clone())),
                _ => Err(CompileError::MacroEval("symbol requires a string or symbol".into())),
            }
        }
        2 => {
            let ns = match &args[0] {
                Edn::String(s) => s.clone(),
                Edn::Nil => return prim_symbol(&args[1..]),
                _ => return Err(CompileError::MacroEval("symbol namespace must be string".into())),
            };
            let name = match &args[1] {
                Edn::String(s) => s.clone(),
                _ => return Err(CompileError::MacroEval("symbol name must be string".into())),
            };
            Ok(Edn::Symbol(suss_core::Symbol::namespaced(&ns, &name)))
        }
        _ => Err(CompileError::MacroEval("symbol requires 1 or 2 arguments".into())),
    }
}

fn prim_keyword(args: &[Edn]) -> CompileResult<Edn> {
    match args.len() {
        1 => {
            match &args[0] {
                Edn::String(s) => Ok(Edn::Keyword(suss_core::Keyword::parse(s))),
                Edn::Keyword(k) => Ok(Edn::Keyword(k.clone())),
                Edn::Symbol(s) => {
                    if let Some(ns) = &s.namespace {
                        Ok(Edn::Keyword(suss_core::Keyword::namespaced(ns, &s.name)))
                    } else {
                        Ok(Edn::Keyword(suss_core::Keyword::new(&s.name)))
                    }
                }
                _ => Err(CompileError::MacroEval("keyword requires a string, symbol, or keyword".into())),
            }
        }
        2 => {
            let ns = match &args[0] {
                Edn::String(s) => s.clone(),
                Edn::Nil => return prim_keyword(&args[1..]),
                _ => return Err(CompileError::MacroEval("keyword namespace must be string".into())),
            };
            let name = match &args[1] {
                Edn::String(s) => s.clone(),
                _ => return Err(CompileError::MacroEval("keyword name must be string".into())),
            };
            Ok(Edn::Keyword(suss_core::Keyword::namespaced(&ns, &name)))
        }
        _ => Err(CompileError::MacroEval("keyword requires 1 or 2 arguments".into())),
    }
}

fn prim_apply(args: &[Edn]) -> CompileResult<Edn> {
    // This is a stub - actual apply is handled in MacroEvaluator::call_apply
    // because it needs access to the evaluator
    Err(CompileError::MacroEval("apply called directly as primitive".into()))
}

/// Implement (and ...) macro expansion
/// (and) => true
/// (and x) => x
/// (and x y ...) => (let [and__N__ x] (if and__N__ (and y ...) and__N__))
fn prim_and_impl(args: &[Edn]) -> CompileResult<Edn> {
    // args is a list containing the and arguments
    let items = match args.get(0) {
        Some(Edn::List(v)) => v.clone(),
        Some(Edn::Nil) => vec![],
        _ => return Err(CompileError::MacroEval("_and_impl expects a list".into())),
    };

    if items.is_empty() {
        return Ok(Edn::Bool(true));
    }
    if items.len() == 1 {
        return Ok(items[0].clone());
    }

    // Generate (if first-arg (and rest-args...) first-arg)
    // For short-circuit semantics, we'd need let, but lower.rs handles and natively
    // So we just produce the equivalent form that lower_and expects
    let first = items[0].clone();
    let rest: Vec<Edn> = items[1..].to_vec();

    // Produce (if first (and rest...) first)
    let and_rest = {
        let mut and_call = vec![Edn::Symbol(suss_core::Symbol::new("and"))];
        and_call.extend(rest);
        Edn::List(and_call)
    };

    Ok(Edn::List(vec![
        Edn::Symbol(suss_core::Symbol::new("if")),
        first.clone(),
        and_rest,
        first,
    ]))
}

/// Implement (or ...) macro expansion
/// (or) => nil
/// (or x) => x
/// (or x y ...) => (let [or__N__ x] (if or__N__ or__N__ (or y ...)))
fn prim_or_impl(args: &[Edn]) -> CompileResult<Edn> {
    let items = match args.get(0) {
        Some(Edn::List(v)) => v.clone(),
        Some(Edn::Nil) => vec![],
        _ => return Err(CompileError::MacroEval("_or_impl expects a list".into())),
    };

    if items.is_empty() {
        return Ok(Edn::Nil);
    }
    if items.len() == 1 {
        return Ok(items[0].clone());
    }

    // Generate (if first first (or rest...))
    let first = items[0].clone();
    let rest: Vec<Edn> = items[1..].to_vec();

    let or_rest = {
        let mut or_call = vec![Edn::Symbol(suss_core::Symbol::new("or"))];
        or_call.extend(rest);
        Edn::List(or_call)
    };

    Ok(Edn::List(vec![
        Edn::Symbol(suss_core::Symbol::new("if")),
        first.clone(),
        first,
        or_rest,
    ]))
}

/// Implement (cond ...) macro expansion
/// (cond) => nil
/// (cond :else expr) => expr
/// (cond test expr & rest) => (if test expr (cond rest...))
fn prim_cond_impl(args: &[Edn]) -> CompileResult<Edn> {
    let clauses = match args.get(0) {
        Some(Edn::List(v)) => v.clone(),
        Some(Edn::Nil) => vec![],
        _ => return Err(CompileError::MacroEval("_cond_impl expects a list".into())),
    };

    if clauses.is_empty() {
        return Ok(Edn::Nil);
    }

    if clauses.len() < 2 {
        return Err(CompileError::MacroEval("cond requires pairs of test/expr".into()));
    }

    let test = clauses[0].clone();
    let expr = clauses[1].clone();
    let rest: Vec<Edn> = clauses[2..].to_vec();

    // Check for :else
    let is_else = matches!(&test, Edn::Keyword(k) if k.name == "else");

    if is_else {
        return Ok(expr);
    }

    // Produce (if test expr (cond rest...))
    let cond_rest = if rest.is_empty() {
        Edn::Nil
    } else {
        let mut cond_call = vec![Edn::Symbol(suss_core::Symbol::new("cond"))];
        cond_call.extend(rest);
        Edn::List(cond_call)
    };

    Ok(Edn::List(vec![
        Edn::Symbol(suss_core::Symbol::new("if")),
        test,
        expr,
        cond_rest,
    ]))
}

/// Implement (case ...) macro expansion
/// (case e v1 r1 v2 r2 ... default) => nested if/= checks
fn prim_case_impl(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() < 2 {
        return Err(CompileError::MacroEval("_case_impl expects e and clauses".into()));
    }

    let e = args[0].clone();
    let clauses = match &args[1] {
        Edn::List(v) => v.clone(),
        Edn::Nil => vec![],
        _ => return Err(CompileError::MacroEval("_case_impl clauses must be a list".into())),
    };

    if clauses.is_empty() {
        return Ok(Edn::Nil);
    }

    // If odd number of clauses, last is default
    let has_default = clauses.len() % 2 == 1;
    let default = if has_default {
        clauses.last().cloned().unwrap_or(Edn::Nil)
    } else {
        Edn::Nil
    };

    let pairs_len = if has_default { clauses.len() - 1 } else { clauses.len() };

    // Build nested if structure from the end
    let mut result = default;

    for i in (0..pairs_len).step_by(2).rev() {
        let val = clauses[i].clone();
        let expr = clauses[i + 1].clone();

        // (if (= e val) expr previous-result)
        result = Edn::List(vec![
            Edn::Symbol(suss_core::Symbol::new("if")),
            Edn::List(vec![
                Edn::Symbol(suss_core::Symbol::new("=")),
                e.clone(),
                val,
            ]),
            expr,
            result,
        ]);
    }

    Ok(result)
}

/// Implement (-> x forms...) macro expansion (thread-first)
/// (-> x) => x
/// (-> x (f a b)) => (f x a b)
/// (-> x f) => (f x)
/// (-> x (f a) (g b)) => (g (f x a) b)
fn prim_thread_first_impl(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() < 2 {
        return Err(CompileError::MacroEval("_thread_first_impl expects x and forms".into()));
    }

    let x = args[0].clone();
    let forms = match &args[1] {
        Edn::List(v) => v.clone(),
        Edn::Nil => vec![],
        _ => return Err(CompileError::MacroEval("_thread_first_impl forms must be a list".into())),
    };

    if forms.is_empty() {
        return Ok(x);
    }

    // Thread x through each form
    let mut result = x;
    for form in forms {
        result = match form {
            Edn::List(items) if !items.is_empty() => {
                // (f a b) with x becomes (f x a b)
                let mut new_items = vec![items[0].clone(), result.clone()];
                new_items.extend(items[1..].iter().cloned());
                Edn::List(new_items)
            }
            Edn::Symbol(_) => {
                // f becomes (f x)
                Edn::List(vec![form, result])
            }
            _ => {
                return Err(CompileError::MacroEval(
                    "-> form must be a symbol or list".into()
                ));
            }
        };
    }

    Ok(result)
}

/// Implement (->> x forms...) macro expansion (thread-last)
/// (->> x) => x
/// (->> x (f a b)) => (f a b x)
/// (->> x f) => (f x)
/// (->> x (f a) (g b)) => (g b (f a x))
fn prim_thread_last_impl(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() < 2 {
        return Err(CompileError::MacroEval("_thread_last_impl expects x and forms".into()));
    }

    let x = args[0].clone();
    let forms = match &args[1] {
        Edn::List(v) => v.clone(),
        Edn::Nil => vec![],
        _ => return Err(CompileError::MacroEval("_thread_last_impl forms must be a list".into())),
    };

    if forms.is_empty() {
        return Ok(x);
    }

    // Thread x through each form
    let mut result = x;
    for form in forms {
        result = match form {
            Edn::List(items) if !items.is_empty() => {
                // (f a b) with x becomes (f a b x)
                let mut new_items = items.clone();
                new_items.push(result);
                Edn::List(new_items)
            }
            Edn::Symbol(_) => {
                // f becomes (f x)
                Edn::List(vec![form, result])
            }
            _ => {
                return Err(CompileError::MacroEval(
                    "->> form must be a symbol or list".into()
                ));
            }
        };
    }

    Ok(result)
}

/// Implement (when-let [sym expr] body...) macro expansion
/// (when-let [x expr] body...) => (let [temp expr] (when temp (let [x temp] body...)))
fn prim_when_let_impl(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() < 2 {
        return Err(CompileError::MacroEval("_when_let_impl expects binding and body".into()));
    }

    // Parse binding vector [sym expr]
    let binding = match &args[0] {
        Edn::Vector(v) if v.len() >= 2 => v,
        _ => return Err(CompileError::MacroEval("when-let requires [sym expr] binding".into())),
    };

    let sym = binding[0].clone();
    let expr = binding[1].clone();

    let body = match &args[1] {
        Edn::List(v) => v.clone(),
        Edn::Nil => vec![],
        _ => return Err(CompileError::MacroEval("_when_let_impl body must be a list".into())),
    };

    // Generate: (let [temp# expr] (when temp# (let [sym temp#] body...)))
    let temp_sym = suss_core::Symbol::new("temp__when_let__");

    // Build (let [sym temp#] (do body...))
    let inner_let = {
        let mut do_forms = vec![Edn::Symbol(suss_core::Symbol::new("do"))];
        do_forms.extend(body);
        Edn::List(vec![
            Edn::Symbol(suss_core::Symbol::new("let")),
            Edn::Vector(vec![sym, Edn::Symbol(temp_sym.clone())]),
            Edn::List(do_forms),
        ])
    };

    // Build (when temp# inner_let)
    let when_form = Edn::List(vec![
        Edn::Symbol(suss_core::Symbol::new("when")),
        Edn::Symbol(temp_sym.clone()),
        inner_let,
    ]);

    // Build outer (let [temp# expr] when_form)
    Ok(Edn::List(vec![
        Edn::Symbol(suss_core::Symbol::new("let")),
        Edn::Vector(vec![Edn::Symbol(temp_sym), expr]),
        when_form,
    ]))
}

/// Implement (if-let [sym expr] then else?) macro expansion
/// (if-let [x expr] then else) => (let [temp expr] (if temp (let [x temp] then) else))
fn prim_if_let_impl(args: &[Edn]) -> CompileResult<Edn> {
    if args.len() < 2 {
        return Err(CompileError::MacroEval("_if_let_impl expects binding and then".into()));
    }

    // Parse binding vector [sym expr]
    let binding = match &args[0] {
        Edn::Vector(v) if v.len() >= 2 => v,
        _ => return Err(CompileError::MacroEval("if-let requires [sym expr] binding".into())),
    };

    let sym = binding[0].clone();
    let expr = binding[1].clone();
    let then_clause = args[1].clone();

    // else clause is optional (first element of rest list, or nil)
    let else_clause = match &args[2] {
        Edn::List(v) => v.first().cloned().unwrap_or(Edn::Nil),
        _ => Edn::Nil,
    };

    // Generate: (let [temp# expr] (if temp# (let [sym temp#] then) else))
    let temp_sym = suss_core::Symbol::new("temp__if_let__");

    // Build (let [sym temp#] then)
    let inner_let = Edn::List(vec![
        Edn::Symbol(suss_core::Symbol::new("let")),
        Edn::Vector(vec![sym, Edn::Symbol(temp_sym.clone())]),
        then_clause,
    ]);

    // Build (if temp# inner_let else)
    let if_form = Edn::List(vec![
        Edn::Symbol(suss_core::Symbol::new("if")),
        Edn::Symbol(temp_sym.clone()),
        inner_let,
        else_clause,
    ]);

    // Build outer (let [temp# expr] if_form)
    Ok(Edn::List(vec![
        Edn::Symbol(suss_core::Symbol::new("let")),
        Edn::Vector(vec![Edn::Symbol(temp_sym), expr]),
        if_form,
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_literals() {
        let eval = MacroEvaluator::new();
        let mut env = Env::new();

        assert!(matches!(eval.eval(&Edn::Nil, &mut env).unwrap(), Edn::Nil));
        assert!(matches!(eval.eval(&Edn::Bool(true), &mut env).unwrap(), Edn::Bool(true)));
    }

    #[test]
    fn test_eval_if() {
        let eval = MacroEvaluator::new();
        let mut env = Env::new();

        // (if true 1 2) -> 1
        let expr = Edn::List(vec![
            Edn::Symbol(suss_core::Symbol::new("if")),
            Edn::Bool(true),
            Edn::Number(suss_core::Number::from_i64(1)),
            Edn::Number(suss_core::Number::from_i64(2)),
        ]);
        let result = eval.eval(&expr, &mut env).unwrap();
        assert!(matches!(result, Edn::Number(n) if n.to_i64() == Some(1)));

        // (if false 1 2) -> 2
        let expr = Edn::List(vec![
            Edn::Symbol(suss_core::Symbol::new("if")),
            Edn::Bool(false),
            Edn::Number(suss_core::Number::from_i64(1)),
            Edn::Number(suss_core::Number::from_i64(2)),
        ]);
        let result = eval.eval(&expr, &mut env).unwrap();
        assert!(matches!(result, Edn::Number(n) if n.to_i64() == Some(2)));
    }

    #[test]
    fn test_eval_let() {
        let eval = MacroEvaluator::new();
        let mut env = Env::new();

        // (let [x 42] x) -> 42
        let expr = Edn::List(vec![
            Edn::Symbol(suss_core::Symbol::new("let")),
            Edn::Vector(vec![
                Edn::Symbol(suss_core::Symbol::new("x")),
                Edn::Number(suss_core::Number::from_i64(42)),
            ]),
            Edn::Symbol(suss_core::Symbol::new("x")),
        ]);
        let result = eval.eval(&expr, &mut env).unwrap();
        assert!(matches!(result, Edn::Number(n) if n.to_i64() == Some(42)));
    }

    #[test]
    fn test_eval_fn() {
        let eval = MacroEvaluator::new();
        let mut env = Env::new();

        // ((fn [x] x) 42) -> 42
        let fn_expr = Edn::List(vec![
            Edn::Symbol(suss_core::Symbol::new("fn")),
            Edn::Vector(vec![Edn::Symbol(suss_core::Symbol::new("x"))]),
            Edn::Symbol(suss_core::Symbol::new("x")),
        ]);
        let call_expr = Edn::List(vec![
            fn_expr,
            Edn::Number(suss_core::Number::from_i64(42)),
        ]);
        let result = eval.eval(&call_expr, &mut env).unwrap();
        assert!(matches!(result, Edn::Number(n) if n.to_i64() == Some(42)));
    }

    #[test]
    fn test_list_operations() {
        let eval = MacroEvaluator::new();
        let mut env = Env::new();

        // (first [1 2 3]) -> 1
        let expr = Edn::List(vec![
            Edn::Symbol(suss_core::Symbol::new("first")),
            Edn::Vector(vec![
                Edn::Number(suss_core::Number::from_i64(1)),
                Edn::Number(suss_core::Number::from_i64(2)),
                Edn::Number(suss_core::Number::from_i64(3)),
            ]),
        ]);
        let result = eval.eval(&expr, &mut env).unwrap();
        assert!(matches!(result, Edn::Number(n) if n.to_i64() == Some(1)));
    }
}
