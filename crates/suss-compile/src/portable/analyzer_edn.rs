//! Analyzer output as EDN data (#299 step 2), in tools.analyzer's shape.
//!
//! Each node is a map with `:op`, `:form`, `:env` and `:children`, the
//! ordered keys of its child nodes, so one generic walker serves every op.
//! Nodes are projected from the analyzer's source facts (`SourceAnalysis`),
//! which keep each special form's operands before lowering. Locals and loops
//! are numbered in walk order, so the output does not depend on the
//! analyzer's internal counters. [`emit_form`] turns a node back into a form.
//!
//! Suss analyzes some macros directly, such as `when`, `cond`, `case`,
//! `deftype` and `reify`. They have no lower special form, so they are
//! `:suss/intrinsic` nodes: their children are listed under `:args`, and
//! [`emit_form`] emits their `:form`.
//!
//! The shape is unstable (`docs/runtime/analyzer-edn.md`).
use super::{
    AnalysisContext, SourceOrigin,
    hir::{self, Expression, Hir, LocalBinding, SourceNode, SourceRole},
    resolve::{self, Global, Phase},
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};
use suss_reader::{
    Keyword, Symbol,
    edn::{
        Value,
        source::{LineIndex, form_to_value, position, value_to_form},
    },
    forms::{Form, Kind},
};

/// The version of the analyzer output's shape.
pub const VERSION: u32 = 1;

fn kw(name: &str) -> Value {
    Value::keyword(name)
}
fn sym(namespace: Option<&str>, name: &str) -> Value {
    Value::Symbol(Symbol {
        namespace: namespace.map(str::to_owned),
        name: name.to_owned(),
    })
}
fn num(n: usize) -> Value {
    Value::Number(n as f64)
}
fn global_symbol(global: &Global) -> Value {
    sym(Some(global.namespace()), global.name())
}
fn phase_keyword(phase: Phase) -> Value {
    kw(match phase {
        Phase::Runtime => "runtime",
        Phase::Macro => "macro",
    })
}
fn context_keyword(context: AnalysisContext) -> Value {
    kw(match context {
        AnalysisContext::Statement => "ctx/statement",
        AnalysisContext::Expression => "ctx/expr",
        AnalysisContext::Return => "ctx/return",
    })
}

type Fields = BTreeMap<Value, Value>;

/// A node under construction.
struct Node(Fields, Vec<&'static str>);
impl Node {
    fn new(op: &str) -> Self {
        Node(BTreeMap::from([(kw("op"), kw(op))]), Vec::new())
    }
    fn set(&mut self, key: &str, value: Value) {
        self.0.insert(kw(key), value);
    }
    fn child(&mut self, key: &'static str, value: Value) {
        self.set(key, value);
        self.1.push(key);
    }
    fn finish(mut self) -> Value {
        let children = self.1.iter().map(|k| kw(k)).collect();
        self.set("children", Value::Vector(children));
        Value::Map(self.0)
    }
}

/// Projects analyzed forms to nodes, numbering locals and loops per document.
pub struct Projector<'o> {
    lines: Option<LineIndex<'o>>,
    locals: HashMap<usize, usize>,
    loops: usize,
    loop_stack: Vec<usize>,
}

impl<'o> Projector<'o> {
    /// A projector for forms analyzed from `origin`, whose positions it reports.
    pub fn new(origin: Option<&'o SourceOrigin>) -> Self {
        Projector {
            lines: origin.map(|origin| LineIndex::new(origin.text())),
            locals: HashMap::new(),
            loops: 0,
            loop_stack: Vec::new(),
        }
    }

    fn local_id(&mut self, identity: &Arc<()>) -> usize {
        let next = self.locals.len();
        *self.locals.entry(Arc::as_ptr(identity) as usize).or_insert(next)
    }

    fn form(&self, form: &Form) -> Result<Value, String> {
        form_to_value(form).map_err(|error| error.message)
    }

    fn located(&self, node: &mut Node, span: &std::ops::Range<usize>) {
        if span.end > 0 {
            node.set("suss/source", Value::Map(position(span, self.lines.as_ref())));
        }
    }

    /// A form's position. Macro expansions take their call's span but keep
    /// the reader's position metadata, as Clojure's forms do; when a form has
    /// that metadata, it is the position, without byte offsets.
    fn form_located(&self, node: &mut Node, form: &Form) {
        if let Ok(meta) = suss_reader::edn::source::metadata(form)
            && let Some(line) = meta.get(&kw("line"))
        {
            let mut position = BTreeMap::from([(kw("line"), line.clone())]);
            for key in ["column", "end-line", "end-column"] {
                if let Some(value) = meta.get(&kw(key)) {
                    position.insert(kw(key), value.clone());
                }
            }
            node.set("suss/source", Value::Map(position));
        } else {
            self.located(node, &form.span);
        }
    }

    fn env(
        &mut self,
        namespace: &str,
        phase: Phase,
        context: AnalysisContext,
        locals: &HashMap<String, LocalBinding>,
        fields: &HashMap<String, hir::FieldBinding>,
    ) -> Value {
        let mut names = BTreeMap::new();
        let mut sorted: Vec<_> = locals.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(b.0));
        for (name, binding) in sorted {
            if matches!(binding.source_role, SourceRole::PrivateCatch { .. }) {
                continue;
            }
            names.insert(sym(None, name), num(self.local_id(&binding.identity)));
        }
        let mut sorted: Vec<_> = fields.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(b.0));
        for (name, field) in sorted {
            names.entry(sym(None, name)).or_insert_with(|| num(self.local_id(&field.identity)));
        }
        Value::Map(BTreeMap::from([
            (kw("ns"), sym(None, namespace)),
            (kw("phase"), phase_keyword(phase)),
            (kw("context"), context_keyword(context)),
            (kw("locals"), Value::Map(names)),
        ]))
    }

    /// The node of one analyzed form.
    pub fn node(&mut self, hir: &Hir) -> Result<Value, String> {
        stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || self.node_inner(hir))
    }

    fn nodes<'h>(&mut self, hirs: impl IntoIterator<Item = &'h Hir>) -> Result<Value, String> {
        Ok(Value::Vector(hirs.into_iter().map(|h| self.node(h)).collect::<Result<_, _>>()?))
    }

    fn node_inner(&mut self, hir: &Hir) -> Result<Value, String> {
        let Some(source) = &hir.source else {
            return self.lowered(hir);
        };
        let form = &source.form;
        let mut node = self.source_node(hir, source)?;
        node.set("form", self.form(form)?);
        let env = self.env(&source.namespace, source.phase, source.context, &source.locals, &source.fields);
        node.set("env", env);
        if source.is_body {
            node.set("body?", Value::Bool(true));
        }
        if let Some(tag) = &source.tags.tag {
            node.set("tag", self.form(tag)?);
        }
        if reader_sequence(hir) {
            node.set("suss/lowering", kw("reader-sequence"));
        }
        self.form_located(&mut node, form);
        Ok(node.finish())
    }

    fn source_node(&mut self, hir: &Hir, source: &hir::SourceAnalysis) -> Result<Node, String> {
        let form = &source.form;
        // (quote x): a quote of a constant, as tools.analyzer's.
        if form.metadata.is_empty()
            && let Kind::List(items) = &form.kind
            && items.len() == 2
            && matches!(&items[0].kind, Kind::Symbol(head) if head.namespace.is_none() && head.name == "quote")
        {
            let mut constant = Node::new("const");
            let datum = self.form(&items[1])?;
            constant.set("val", datum.clone());
            constant.set("form", datum);
            constant.set("literal?", Value::Bool(true));
            let mut node = Node::new("quote");
            node.set("literal?", Value::Bool(true));
            node.child("expr", constant.finish());
            return Ok(node);
        }
        if let Some(node) = &source.node {
            return self.structural(node);
        }
        if form.metadata.is_empty()
            && let Some(kind) = match &form.kind {
                Kind::Nil => Some("nil"),
                Kind::Bool(_) => Some("bool"),
                Kind::Number(_) => Some("number"),
                Kind::String(_) => Some("string"),
                Kind::Keyword(_) => Some("keyword"),
                _ => None,
            }
        {
            let mut node = Node::new("const");
            node.set("val", self.form(form)?);
            node.set("type", kw(kind));
            return Ok(node);
        }
        if let Kind::Symbol(symbol) = &form.kind {
            match &source.resolved {
                Some(hir::SourceBinding::Local(binding)) => return Ok(self.local(binding)),
                Some(hir::SourceBinding::Field(field)) => {
                    let mut node = Node::new("local");
                    node.set("name", sym(None, &symbol.name));
                    node.set("id", num(self.local_id(&field.identity)));
                    node.set("local", kw("field"));
                    return Ok(node);
                }
                Some(hir::SourceBinding::JavaScript { global }) => {
                    let mut node = Node::new("js-var");
                    node.set("var", sym(Some("js"), global.name()));
                    return Ok(node);
                }
                Some(hir::SourceBinding::Global { global, .. }) => {
                    let mut node = Node::new("var");
                    node.set("var", global_symbol(global));
                    return Ok(node);
                }
                None => {}
            }
        }
        if let Some(callable) = &source.callable {
            return self.function(callable, source.phase);
        }
        if let Expression::Definition { global, name_metadata, docstring, initializer, once, .. } = &*hir.kind {
            let mut node = Node::new("def");
            node.set("var", global_symbol(global));
            node.set("name", sym(None, global.name()));
            let mut name = Form { span: 0..0, metadata: name_metadata.clone(), kind: Kind::Nil };
            name.kind = Kind::Symbol(Symbol::new(global.name()));
            if let Value::Meta(_, meta) = self.form(&name)? {
                node.set("meta", Value::Map(meta));
            }
            if let Some(doc) = docstring {
                node.set("doc", suss_reader::edn::source::string_value(doc));
            }
            if *once {
                node.set("suss/once", Value::Bool(true));
            }
            if let Some(init) = initializer {
                node.child("init", self.node(init)?);
            }
            return Ok(node);
        }
        self.intrinsic(hir, form)
    }

    fn local(&mut self, binding: &LocalBinding) -> Node {
        let mut node = Node::new("local");
        let name = match &binding.declaration.kind {
            Kind::Symbol(symbol) => symbol.name.clone(),
            _ => String::new(),
        };
        node.set("name", sym(None, &name));
        node.set("id", num(self.local_id(&binding.identity)));
        let kind = match (&binding.source_role, binding.source_kind()) {
            (SourceRole::CatchBinding { .. }, _) => "catch",
            (_, hir::LocalKind::Let) => "let",
            (_, hir::LocalKind::Loop) => "loop",
            (_, hir::LocalKind::Argument { .. }) => "arg",
            (_, hir::LocalKind::FunctionName) => "fn",
            (_, hir::LocalKind::Catch) => "catch",
        };
        node.set("local", kw(kind));
        if let hir::LocalKind::Argument { index, .. } = binding.source_kind() {
            node.set("arg-id", num(index));
        }
        node
    }

    /// A binding declaration: a `let`/`loop` binding, a parameter, a catch
    /// local or a function's name.
    fn binding(&mut self, binding: &LocalBinding) -> Result<Value, String> {
        let mut node = self.local(binding);
        node.set("op", kw("binding"));
        node.set("form", self.form(&binding.declaration)?);
        self.form_located(&mut node, &binding.declaration);
        if let Some(init) = &binding.initializer
            && !matches!(binding.source_role, SourceRole::CatchBinding { .. })
            && !(init.source.is_none()
                && matches!(*init.kind, Expression::Nominal { operation: hir::Nominal::CurrentThis, .. }))
        {
            node.child("init", self.node(init)?);
        }
        Ok(node.finish())
    }

    fn function(&mut self, callable: &hir::SourceCallable, phase: Phase) -> Result<Node, String> {
        let mut node = Node::new("fn");
        if let Some(scope) = &callable.name
            && let Some(binding) = &scope.self_binding
        {
            node.child("local", self.binding(binding)?);
        }
        let mut methods = Vec::new();
        for method in &callable.methods {
            let mut entry = Node::new("fn-method");
            let id = self.loops;
            self.loops += 1;
            self.loop_stack.push(id);
            entry.set("loop-id", num(id));
            let params = method.declarations.iter().map(|b| self.binding(b)).collect::<Result<_, _>>();
            let body = params.and_then(|params| Ok((params, self.node(&method.body)?)));
            self.loop_stack.pop();
            let (params, body) = body?;
            entry.child("params", Value::Vector(params));
            entry.child("body", body);
            entry.set("variadic?", Value::Bool(method.variadic));
            entry.set("fixed-arity", num(method.parameters.len() - usize::from(method.variadic)));
            entry.set("form", self.form(&method.form)?);
            let environment = &method.environment;
            let namespace = environment.scope.namespace.clone();
            let env = self.env(&namespace, phase, environment.context, &environment.locals, &environment.fields);
            entry.set("env", env);
            self.form_located(&mut entry, &method.form);
            methods.push(entry.finish());
        }
        node.child("methods", Value::Vector(methods));
        node.set("variadic?", Value::Bool(callable.variadic()));
        if let Some(arity) = callable.max_fixed_arity() {
            node.set("max-fixed-arity", num(arity));
        }
        Ok(node)
    }

    fn structural(&mut self, node: &SourceNode) -> Result<Node, String> {
        Ok(match node {
            SourceNode::Vector(items) => {
                let mut n = Node::new("vector");
                n.child("items", self.nodes(items.iter())?);
                n
            }
            SourceNode::Set(items) => {
                let mut n = Node::new("set");
                n.child("items", self.nodes(items.iter())?);
                n
            }
            SourceNode::Map(items) => {
                let mut n = Node::new("map");
                n.child("keys", self.nodes(items.iter().step_by(2))?);
                n.child("vals", self.nodes(items.iter().skip(1).step_by(2))?);
                n
            }
            SourceNode::Do { statements, result } => {
                let mut n = Node::new("do");
                n.child("statements", self.nodes(statements.iter())?);
                n.child("ret", self.node(result)?);
                n
            }
            SourceNode::If { condition, consequent, alternative } => {
                let mut n = Node::new("if");
                n.child("test", self.node(condition)?);
                n.child("then", self.node(consequent)?);
                n.child("else", self.node(alternative)?);
                n
            }
            SourceNode::Bindings { is_loop: false, bindings, body }
                if bindings.len() == 1
                    && bindings[0].initializer.as_ref().is_some_and(|init| {
                        init.source.is_none()
                            && matches!(*init.kind, Expression::Nominal { operation: hir::Nominal::CurrentThis, .. })
                    }) =>
            {
                // (this-as name body...) binds the current this.
                let mut n = Node::new("suss/this-as");
                n.child("local", self.binding(&bindings[0])?);
                n.child("body", self.node(body)?);
                n
            }
            SourceNode::Bindings { is_loop, bindings, body } => {
                let mut n = Node::new(if *is_loop { "loop" } else { "let" });
                let bindings = bindings.iter().map(|b| self.binding(b)).collect::<Result<_, _>>()?;
                n.child("bindings", Value::Vector(bindings));
                if *is_loop {
                    let id = self.loops;
                    self.loops += 1;
                    n.set("loop-id", num(id));
                    self.loop_stack.push(id);
                    let body = self.node(body);
                    self.loop_stack.pop();
                    n.child("body", body?);
                } else {
                    n.child("body", self.node(body)?);
                }
                n
            }
            SourceNode::Recur(arguments) => {
                let mut n = Node::new("recur");
                if let Some(id) = self.loop_stack.last() {
                    n.set("loop-id", num(*id));
                }
                n.child("exprs", self.nodes(arguments.iter())?);
                n
            }
            SourceNode::Throw(exception) => {
                let mut n = Node::new("throw");
                n.child("exception", self.node(exception)?);
                n
            }
            SourceNode::Try { body, handler, cleanup, payload } => {
                let mut n = Node::new("try");
                n.child("body", self.node(body)?);
                let catches = if payload.is_some() { self.catches(handler)? } else { Vec::new() };
                n.child("catches", Value::Vector(catches));
                if let Some(cleanup) = cleanup {
                    n.child("finally", self.node(cleanup)?);
                }
                n
            }
            SourceNode::Assign { target, value } => {
                let mut n = Node::new("set!");
                n.child("target", self.node(target)?);
                n.child("val", self.node(value)?);
                n
            }
            SourceNode::Invoke { callee, arguments } => {
                let mut n = Node::new("invoke");
                n.child("fn", self.node(callee)?);
                n.child("args", self.nodes(arguments.iter())?);
                n
            }
            SourceNode::Construct { class, arguments } => {
                let mut n = Node::new("new");
                n.child("class", self.node(class)?);
                n.child("args", self.nodes(arguments.iter())?);
                n
            }
            SourceNode::WithMeta { expression, metadata, .. } => {
                let mut n = Node::new("with-meta");
                n.child("meta", self.node(metadata)?);
                n.child("expr", self.node(expression)?);
                n
            }
            SourceNode::Future(body) => {
                let mut n = Node::new("suss/future");
                n.child("body", self.node(body)?);
                n
            }
            SourceNode::Await(value) => {
                let mut n = Node::new("suss/await");
                n.child("value", self.node(value)?);
                n
            }
        })
    }

    /// The catch clauses of a try, from the handler the analyzer builds for
    /// them: `(if (instance? T e) (let* [local e] body...) next)` for each
    /// class, `(let* [local e] body...)` for `:default`, then `(throw e)`.
    fn catches(&mut self, handler: &Hir) -> Result<Vec<Value>, String> {
        let mut catches = Vec::new();
        let mut current = handler;
        loop {
            let Some(source) = &current.source else { break };
            match source.node.as_deref() {
                Some(SourceNode::If { condition, consequent, alternative }) => {
                    let class = match condition.source.as_ref().map(|s| &s.form.kind) {
                        Some(Kind::List(items)) if items.len() == 3 => self.form(&items[1])?,
                        _ => return Err("Unrecognized catch test".into()),
                    };
                    catches.push(self.catch(class, consequent)?);
                    current = alternative;
                }
                Some(SourceNode::Bindings { .. }) => {
                    catches.push(self.catch(kw("default"), current)?);
                    break;
                }
                _ => break,
            }
        }
        Ok(catches)
    }

    fn catch(&mut self, class: Value, clause: &Hir) -> Result<Value, String> {
        let Some(source) = &clause.source else { return Err("Catch clause without source".into()) };
        let Some(SourceNode::Bindings { bindings, body, .. }) = source.node.as_deref() else {
            return Err("Unrecognized catch clause".into());
        };
        let mut node = Node::new("catch");
        let mut constant = Node::new("const");
        constant.set("val", class.clone());
        constant.set("form", class);
        constant.set("type", kw("class"));
        node.child("class", constant.finish());
        node.child("local", self.binding(&bindings[0])?);
        node.child("body", self.node(body)?);
        node.set("form", self.form(&source.form)?);
        self.form_located(&mut node, &source.form);
        Ok(node.finish())
    }

    /// A form the analyzer handles itself: its operands are the analyzed
    /// forms found below it, in order.
    fn intrinsic(&mut self, hir: &Hir, form: &Form) -> Result<Node, String> {
        let mut node = Node::new("suss/intrinsic");
        if let Kind::List(items) = &form.kind
            && let Some(Form { kind: Kind::Symbol(head), .. }) = items.first()
        {
            node.set("intrinsic", Value::Symbol(head.clone()));
        }
        let mut found = Vec::new();
        let mut seen = HashSet::new();
        collect_sources(hir, true, &mut found, &mut seen);
        node.child("args", self.nodes(found)?);
        Ok(node)
    }

    /// A node the analyzer built without source facts. A syntax-quote
    /// expansion calls `seq`, `concat` and `list` this way.
    fn lowered(&mut self, hir: &Hir) -> Result<Value, String> {
        if reader_sequence(hir)
            && let Expression::Call { callee, arguments } = &*hir.kind
            && let Expression::GlobalOrFallback { global, .. } = &*callee.kind
        {
            let mut var = Node::new("var");
            var.set("var", global_symbol(global));
            var.set("form", global_symbol(global));
            let mut node = Node::new("invoke");
            node.child("fn", var.finish());
            node.child("args", self.nodes(arguments.iter())?);
            node.set("suss/lowering", kw("reader-sequence"));
            return Ok(node.finish());
        }
        let mut node = Node::new("suss/lowered");
        node.set("lowering", kw(lowering_name(&hir.kind)));
        let mut found = Vec::new();
        let mut seen = HashSet::new();
        collect_sources(hir, true, &mut found, &mut seen);
        node.child("args", self.nodes(found)?);
        Ok(node.finish())
    }
}

/// A syntax quote's call of `sequence`, which reads it through a private
/// fallback cell, so that core's own syntax quotes work before `sequence` is
/// defined. An emitted form calls the public `sequence`.
fn reader_sequence(hir: &Hir) -> bool {
    matches!(&*hir.kind, Expression::Call { callee, .. }
        if matches!(*callee.kind, Expression::GlobalOrFallback { .. }))
}

fn lowering_name(kind: &Expression) -> &'static str {
    match kind {
        Expression::Future { .. } => "future",
        Expression::Await { .. } => "await",
        Expression::AsyncTry { .. } => "async-try",
        Expression::AsyncDynamicScope { .. } => "async-dynamic-scope",
        Expression::Bitwise { .. } => "bitwise",
        Expression::Comparison { .. } => "comparison",
        Expression::NilTest(_) => "nil-test",
        Expression::Array { .. } => "array",
        Expression::DynamicScope { .. } => "dynamic-scope",
        Expression::Assign { .. } => "assign",
        Expression::Throw(_) => "throw",
        Expression::Try { .. } => "try",
        Expression::Nominal { .. } => "nominal",
        Expression::Literal(_) => "literal",
        Expression::Local(_) => "local",
        Expression::Global(_) => "global",
        Expression::GlobalOrFallback { .. } => "global-or-fallback",
        Expression::GlobalCell(_) => "global-cell",
        Expression::Definition { .. } => "definition",
        Expression::Let { .. } => "let",
        Expression::Loop { .. } => "loop",
        Expression::Recur { .. } => "recur",
        Expression::If { .. } => "if",
        Expression::Do(_) => "do",
        Expression::Function { .. } => "function",
        Expression::GeneralFunction { .. } => "function",
        Expression::Call { .. } => "call",
        Expression::Arithmetic { .. } => "arithmetic",
    }
}

/// The lowered children of an expression.
fn lowered_children(kind: &Expression) -> Vec<&Hir> {
    match kind {
        Expression::Future { body, .. } => vec![body],
        Expression::Await { value } => vec![value],
        Expression::AsyncTry { body, handler, cleanup, .. } => std::iter::once(body.as_ref())
            .chain(handler.iter().map(|h| h.as_ref()))
            .chain(cleanup.iter().map(|h| h.as_ref()))
            .collect(),
        Expression::Literal(_)
        | Expression::Local(_)
        | Expression::Global(_)
        | Expression::GlobalCell(_)
        | Expression::GlobalOrFallback { .. } => vec![],
        Expression::Bitwise { arguments, .. }
        | Expression::Comparison { arguments, .. }
        | Expression::Array { arguments, .. }
        | Expression::Nominal { arguments, .. }
        | Expression::Arithmetic { arguments, .. }
        | Expression::Recur { arguments, .. } => arguments.iter().collect(),
        Expression::NilTest(value) | Expression::Throw(value) => vec![value],
        Expression::Assign { value, .. } => vec![value],
        Expression::Definition { initializer, .. } => initializer.iter().map(|v| v.as_ref()).collect(),
        Expression::Let { bindings, body } | Expression::Loop { bindings, body, .. } => bindings
            .iter()
            .map(|b| &b.value)
            .chain(std::iter::once(body.as_ref()))
            .collect(),
        Expression::If { condition, consequent, alternative } => vec![condition, consequent, alternative],
        Expression::Do(values) => values.iter().collect(),
        Expression::Call { callee, arguments } => std::iter::once(callee.as_ref()).chain(arguments.iter()).collect(),
        Expression::Function { body, .. } => vec![body],
        Expression::GeneralFunction { methods, .. } => methods.iter().map(|m| m.body.as_ref()).collect(),
        Expression::AsyncDynamicScope { bindings, body } | Expression::DynamicScope { bindings, body } => bindings
            .iter()
            .map(|(_, v)| v)
            .chain(std::iter::once(body.as_ref()))
            .collect(),
        Expression::Try { regions } => regions.iter().map(|r| r.as_ref()).collect(),
    }
}

/// The outermost descendants of `hir` that have source facts, each once.
fn collect_sources<'h>(hir: &'h Hir, root: bool, found: &mut Vec<&'h Hir>, seen: &mut HashSet<usize>) {
    stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
        if !root && let Some(source) = &hir.source {
            if seen.insert(Arc::as_ptr(source) as usize) {
                found.push(hir);
            }
            return;
        }
        for child in lowered_children(&hir.kind) {
            collect_sources(child, false, found, seen);
        }
    })
}

/// The node of a namespace declaration, which the compiler handles before
/// analysis: its form and position.
pub fn namespace_node(form: &Form, phase: Phase, origin: Option<&SourceOrigin>) -> Result<Value, String> {
    let projector = Projector::new(origin);
    let mut node = Node::new("suss/ns");
    node.set("form", projector.form(form)?);
    if let Kind::List(items) = &form.kind
        && let Some(Form { kind: Kind::Symbol(name), .. }) = items.get(1)
    {
        node.set("name", Value::Symbol(name.clone()));
    }
    node.set("env", Value::Map(BTreeMap::from([(kw("phase"), phase_keyword(phase))])));
    projector.form_located(&mut node, form);
    Ok(node.finish())
}

/// A namespace's state after analysis, as tools.analyzer's global
/// environment keeps it: aliases, refers and definitions.
pub fn namespace_env(environment: &resolve::Environment, phase: Phase, namespace: &str) -> Option<Value> {
    let scope = hir::SourceNamespace::capture_namespace(environment, phase, namespace)?;
    let strings = |map: &BTreeMap<String, String>| {
        Value::Map(map.iter().map(|(k, v)| (sym(None, k), sym(None, v))).collect())
    };
    let mut defs = BTreeMap::new();
    for (global, info) in &scope.declarations {
        if global.namespace() != namespace {
            continue;
        }
        let mut def = BTreeMap::from([(kw("var"), global_symbol(global))]);
        if let Ok(Value::Meta(_, meta)) = form_to_value(&info.declaration) {
            def.insert(kw("meta"), Value::Map(meta));
        }
        if let Some(doc) = &info.docstring {
            def.insert(kw("doc"), suss_reader::edn::source::string_value(doc));
        }
        defs.insert(sym(None, global.name()), Value::Map(def));
    }
    Some(Value::Map(BTreeMap::from([
        (kw("ns"), sym(None, namespace)),
        (kw("aliases"), strings(&scope.aliases)),
        (kw("refers"), Value::Map(scope.refers.iter().map(|(k, g)| (sym(None, k), global_symbol(g))).collect())),
        (kw("excludes"), Value::Set(scope.excluded_core.iter().map(|s| sym(None, s)).collect())),
        (kw("macro-aliases"), strings(&scope.macro_aliases)),
        (kw("defs"), Value::Map(defs)),
    ])))
}

// emit-form

fn get<'v>(node: &'v Value, key: &str) -> Option<&'v Value> {
    match node.unmeta() {
        Value::Map(map) => map.get(&kw(key)),
        _ => None,
    }
}
fn field<'v>(node: &'v Value, key: &str) -> Result<&'v Value, String> {
    get(node, key).ok_or_else(|| format!("Node has no {key}"))
}
fn items<'v>(node: &'v Value, key: &str) -> Result<&'v [Value], String> {
    match field(node, key)? {
        Value::Vector(items) => Ok(items),
        _ => Err(format!("Node's {key} is not a vector")),
    }
}
fn list(items: Vec<Form>) -> Form {
    Form { span: 0..0, metadata: Vec::new(), kind: Kind::List(items) }
}
fn symbol_form(name: &str) -> Form {
    Form { span: 0..0, metadata: Vec::new(), kind: Kind::Symbol(Symbol::new(name)) }
}
fn op(node: &Value) -> Result<&Keyword, String> {
    match field(node, "op")? {
        Value::Keyword(k) => Ok(k),
        _ => Err("Node's op is not a keyword".into()),
    }
}

/// The form a node was analyzed from, rebuilt from its children, as
/// tools.analyzer's `emit-form`. Analyzing it again in the same environment
/// gives an equal node, apart from source positions and generated names.
pub fn emit_form(node: &Value) -> Result<Form, String> {
    stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || emit_inner(node))
}

/// The forms of a body: a `do` the analyzer made for a body contributes its
/// forms, not itself.
fn body_forms(node: &Value) -> Result<Vec<Form>, String> {
    if get(node, "body?") == Some(&Value::Bool(true)) && op(node)?.name == "do" {
        let mut forms = items(node, "statements")?.iter().map(emit_form).collect::<Result<Vec<_>, _>>()?;
        forms.push(emit_form(field(node, "ret")?)?);
        Ok(forms)
    } else {
        Ok(vec![emit_form(node)?])
    }
}

fn binding_name(binding: &Value) -> Result<Form, String> {
    Ok(value_to_form(field(binding, "form")?))
}

/// A macro-phase definition of a function whose methods all take `&form`
/// and `&env` first, as `defmacro` makes.
fn is_macro_definition(node: &Value) -> bool {
    let phase = get(node, "env").and_then(|env| get(env, "phase"));
    let Some(init) = get(node, "init") else { return false };
    phase == Some(&kw("macro"))
        && get(init, "op") == Some(&kw("fn"))
        && items(init, "methods").is_ok_and(|methods| {
            methods.iter().all(|method| {
                items(method, "params").is_ok_and(|params| {
                    params.len() >= 2
                        && get(&params[0], "name") == Some(&sym(None, "&form"))
                        && get(&params[1], "name") == Some(&sym(None, "&env"))
                })
            })
        })
}

fn emit_inner(node: &Value) -> Result<Form, String> {
    let op = op(node)?;
    let emitted = |key: &str| emit_form(field(node, key)?);
    let all = |key: &str| items(node, key)?.iter().map(emit_form).collect::<Result<Vec<_>, _>>();
    let head = |name: &str, rest: Vec<Form>| {
        let mut forms = vec![symbol_form(name)];
        forms.extend(rest);
        list(forms)
    };
    let collection = |kind: fn(Vec<Form>) -> Kind, forms: Vec<Form>| Form { span: 0..0, metadata: Vec::new(), kind: kind(forms) };
    Ok(match (op.namespace.as_deref(), op.name.as_str()) {
        (None, "const") => value_to_form(field(node, "val")?),
        (None, "quote") => head("quote", vec![value_to_form(field(field(node, "expr")?, "val")?)]),
        (None, "vector") => collection(Kind::Vector, all("items")?),
        (None, "set") => collection(Kind::Set, all("items")?),
        (None, "map") => {
            let keys = all("keys")?;
            let vals = all("vals")?;
            collection(Kind::Map, keys.into_iter().zip(vals).flat_map(|(k, v)| [k, v]).collect())
        }
        (None, "with-meta") => {
            let mut form = emitted("expr")?;
            form.metadata = vec![emitted("meta")?];
            form
        }
        (None, "local") | (None, "binding") => match field(node, "name")? {
            Value::Symbol(s) => symbol_form(&s.name),
            _ => return Err("Local without a name".into()),
        },
        (None, "var") | (None, "js-var") => value_to_form(field(node, "var")?),
        (None, "do") => {
            let mut forms = all("statements")?;
            forms.push(emitted("ret")?);
            head("do", forms)
        }
        (None, "if") => head("if", vec![emitted("test")?, emitted("then")?, emitted("else")?]),
        (None, "let") | (None, "loop") => {
            let mut bindings = Vec::new();
            for binding in items(node, "bindings")? {
                bindings.push(binding_name(binding)?);
                bindings.push(emit_form(field(binding, "init")?)?);
            }
            let mut forms = vec![collection(Kind::Vector, bindings)];
            forms.extend(body_forms(field(node, "body")?)?);
            head(if op.name == "let" { "let*" } else { "loop*" }, forms)
        }
        (None, "recur") => head("recur", all("exprs")?),
        (None, "fn") => {
            let mut forms = Vec::new();
            if let Some(local) = get(node, "local") {
                forms.push(binding_name(local)?);
            }
            for method in items(node, "methods")? {
                let params = items(method, "params")?;
                let mut names = params.iter().map(binding_name).collect::<Result<Vec<_>, _>>()?;
                if get(method, "variadic?") == Some(&Value::Bool(true)) && !names.is_empty() {
                    names.insert(names.len() - 1, symbol_form("&"));
                }
                let mut clause = vec![collection(Kind::Vector, names)];
                clause.extend(body_forms(field(method, "body")?)?);
                forms.push(list(clause));
            }
            head("fn*", forms)
        }
        (None, "def") if is_macro_definition(node) => {
            // defmacro: the macro session defines a function of &form, &env
            // and the macro's own parameters.
            let Value::Symbol(name) = field(node, "name")? else { return Err("def without a name".into()) };
            let mut name = symbol_form(&name.name);
            if let Some(meta) = get(node, "meta") {
                name.metadata = vec![value_to_form(meta)];
            }
            let mut forms = vec![name];
            if let Some(doc) = get(node, "doc") {
                forms.push(value_to_form(doc));
            }
            for method in items(field(node, "init")?, "methods")? {
                let params = &items(method, "params")?[2..];
                let mut names = params.iter().map(binding_name).collect::<Result<Vec<_>, _>>()?;
                if get(method, "variadic?") == Some(&Value::Bool(true)) && !names.is_empty() {
                    names.insert(names.len() - 1, symbol_form("&"));
                }
                let mut clause = vec![collection(Kind::Vector, names)];
                clause.extend(body_forms(field(method, "body")?)?);
                forms.push(list(clause));
            }
            let mut form = list(forms);
            if let Kind::List(items) = &mut form.kind {
                items.insert(0, value_to_form(&sym(Some("suss.core"), "defmacro")));
            }
            form
        }
        (None, "def") => {
            let Value::Symbol(name) = field(node, "name")? else { return Err("def without a name".into()) };
            let mut name = symbol_form(&name.name);
            if let Some(meta) = get(node, "meta") {
                name.metadata = vec![value_to_form(meta)];
            }
            let mut forms = vec![name];
            if let Some(doc) = get(node, "doc") {
                forms.push(value_to_form(doc));
            }
            if let Some(init) = get(node, "init") {
                forms.push(emit_form(init)?);
            }
            let once = get(node, "suss/once") == Some(&Value::Bool(true));
            let mut form = head("def", forms);
            if once && let Kind::List(items) = &mut form.kind {
                items[0] = value_to_form(&sym(Some("suss.core"), "defonce"));
            }
            form
        }
        (None, "set!") => head("set!", vec![emitted("target")?, emitted("val")?]),
        (None, "invoke") => {
            let mut forms = vec![emitted("fn")?];
            forms.extend(all("args")?);
            list(forms)
        }
        (None, "new") => {
            let mut forms = vec![emitted("class")?];
            forms.extend(all("args")?);
            head("new", forms)
        }
        (None, "throw") => head("throw", vec![emitted("exception")?]),
        (None, "try") => {
            let mut forms = body_forms(field(node, "body")?)?;
            for catch in items(node, "catches")? {
                let mut clause = vec![
                    symbol_form("catch"),
                    value_to_form(field(field(catch, "class")?, "val")?),
                    binding_name(field(catch, "local")?)?,
                ];
                clause.extend(body_forms(field(catch, "body")?)?);
                forms.push(list(clause));
            }
            if let Some(cleanup) = get(node, "finally") {
                let mut clause = vec![symbol_form("finally")];
                clause.extend(body_forms(cleanup)?);
                forms.push(list(clause));
            }
            head("try", forms)
        }
        (Some("suss"), "this-as") => {
            let mut forms = vec![symbol_form("this-as"), binding_name(field(node, "local")?)?];
            forms.extend(body_forms(field(node, "body")?)?);
            list(forms)
        }
        (Some("suss"), "future") => {
            let mut forms = vec![value_to_form(&sym(Some("suss.async"), "future*"))];
            forms.extend(body_forms(field(node, "body")?)?);
            list(forms)
        }
        (Some("suss"), "await") => list(vec![value_to_form(&sym(Some("suss.async"), "await*")), emitted("value")?]),
        (Some("suss"), "intrinsic") => emit_intrinsic(node)?,
        (Some("suss"), "ns") => value_to_form(field(node, "form")?),
        _ => return Err(format!("Cannot emit a form for op {op:?}")),
    })
}

/// An intrinsic's form with each of its argument nodes emitted in place of
/// the operand it was analyzed from, so edits to them apply, and map and set
/// literals that are arguments keep their order. An operand matches the first
/// unused argument whose `:form` equals it; operands that match none are
/// emitted as written.
fn emit_intrinsic(node: &Value) -> Result<Form, String> {
    let form = field(node, "form")?;
    let args = items(node, "args")?;
    let mut by_form: BTreeMap<&Value, Vec<usize>> = BTreeMap::new();
    for (index, arg) in args.iter().enumerate().rev() {
        if let Some(form) = get(arg, "form") {
            by_form.entry(form).or_default().push(index);
        }
    }
    fn rebuild(value: &Value, root: bool, args: &[Value], by_form: &mut BTreeMap<&Value, Vec<usize>>) -> Result<Form, String> {
        stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
            let metadata = match value {
                Value::Meta(_, meta) => vec![value_to_form(&Value::Map(meta.clone()))],
                _ => Vec::new(),
            };
            if !root && let Some(index) = by_form.get_mut(value).and_then(Vec::pop) {
                let mut emitted = emit_form(&args[index])?;
                if emitted.metadata.is_empty() {
                    emitted.metadata = metadata;
                }
                return Ok(emitted);
            }
            let children = |items: &[Value], by_form: &mut BTreeMap<&Value, Vec<usize>>| {
                items.iter().map(|item| rebuild(item, false, args, by_form)).collect::<Result<Vec<_>, String>>()
            };
            let kind = match value.unmeta() {
                Value::List(items) => Kind::List(children(items, by_form)?),
                Value::Vector(items) => Kind::Vector(children(items, by_form)?),
                Value::Set(items) => Kind::Set(children(&items.iter().cloned().collect::<Vec<_>>(), by_form)?),
                Value::Map(map) => {
                    let mut entries = Vec::new();
                    for (k, v) in map {
                        entries.push(rebuild(k, false, args, by_form)?);
                        entries.push(rebuild(v, false, args, by_form)?);
                    }
                    Kind::Map(entries)
                }
                _ => return Ok(value_to_form(value)),
            };
            Ok(Form { span: 0..0, metadata, kind })
        })
    }
    rebuild(form, true, args, &mut by_form)
}

/// A node for comparison: without `:form`, source positions or lowering
/// notes, and with generated names (`G__12`, `x__3__auto__`, `$exception4`)
/// renumbered in order of appearance.
pub fn comparable(node: &Value) -> Value {
    let mut names = HashMap::new();
    strip(node, &mut names)
}


fn strip(value: &Value, names: &mut HashMap<String, String>) -> Value {
    stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || match value {
        Value::Map(map) => Value::Map(
            map.iter()
                .filter(|(k, _)| ![kw("form"), kw("suss/source"), kw("suss/lowering")].contains(k))
                .map(|(k, v)| (strip(k, names), strip(v, names)))
                .collect(),
        ),
        Value::Vector(items) => Value::Vector(items.iter().map(|v| strip(v, names)).collect()),
        Value::List(items) => Value::List(items.iter().map(|v| strip(v, names)).collect()),
        Value::Set(items) => Value::Set(items.iter().map(|v| strip(v, names)).collect()),
        Value::Symbol(s) => Value::Symbol(Symbol { namespace: s.namespace.clone(), name: generated(&s.name, names) }),
        Value::Meta(inner, meta) => Value::Meta(
            Box::new(strip(inner, names)),
            meta.iter().map(|(k, v)| (strip(k, names), strip(v, names))).collect(),
        ),
        Value::Tagged(tag, inner) => Value::Tagged(tag.clone(), Box::new(strip(inner, names))),
        other => other.clone(),
    })
}

/// A generated name with its number replaced by its order of appearance.
fn generated(name: &str, names: &mut HashMap<String, String>) -> String {
    let digits_at = |i: usize| name[i..].chars().take_while(char::is_ascii_digit).count();
    let mut out = String::new();
    let mut rest = 0;
    let bytes = name.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let marker = if name[i..].starts_with("__") {
            2
        } else if name[i..].starts_with("$exception") {
            "$exception".len()
        } else {
            0
        };
        if marker > 0 {
            let digits = digits_at(i + marker);
            if digits > 0 {
                let number = &name[i + marker..i + marker + digits];
                let next = names.len();
                let id = names.entry(number.to_owned()).or_insert_with(|| format!("#{next}")).clone();
                out.push_str(&name[rest..i + marker]);
                out.push_str(&id);
                i += marker + digits;
                rest = i;
                continue;
            }
        }
        i += name[i..].chars().next().map_or(1, char::len_utf8);
    }
    out.push_str(&name[rest..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_names_are_renumbered_in_order() {
        let mut names = HashMap::new();
        assert_eq!(generated("G__17", &mut names), "G__#0");
        assert_eq!(generated("x__3__auto__", &mut names), "x__#1__auto__");
        assert_eq!(generated("p__17", &mut names), "p__#0");
        assert_eq!(generated("$exception9", &mut names), "$exception#2");
        assert_eq!(generated("plain", &mut names), "plain");
        assert_eq!(generated("a__b", &mut names), "a__b");
    }
}
