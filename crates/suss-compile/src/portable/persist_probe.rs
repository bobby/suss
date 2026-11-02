//! Throwaway size probe for #299 step 3. Not for commit.
use super::hir::{Expression, Hir, LocalBinding, FieldBinding, FunctionScope, SourceAnalysis, SourceBinding, SourceCallable, SourceNamespace, SourceNode};
use super::resolve::{DefinitionInfo, Phase};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use suss_reader::forms::{Form, Kind};

#[derive(Default)]
struct Probe {
    seen: HashMap<&'static str, HashSet<usize>>,
    counts: HashMap<&'static str, usize>,
    forms: HashSet<u64>,
    hirs: HashSet<u64>,
}
impl Probe {
    fn first<T: ?Sized>(&mut self, kind: &'static str, ptr: *const T) -> bool {
        self.seen.entry(kind).or_default().insert(ptr as *const () as usize)
    }
    fn add(&mut self, key: &'static str, n: usize) {
        *self.counts.entry(key).or_default() += n;
    }
    fn form(&mut self, form: &Form) -> u64 {
        self.add("form-nodes-total", 1);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        form.span.hash(&mut h);
        for m in &form.metadata {
            self.form(m).hash(&mut h);
        }
        std::mem::discriminant(&form.kind).hash(&mut h);
        match &form.kind {
            Kind::Bool(b) => b.hash(&mut h),
            Kind::Number(n) => n.to_bits().hash(&mut h),
            Kind::String(s) => s.hash(&mut h),
            Kind::Symbol(s) => (&s.namespace, &s.name).hash(&mut h),
            Kind::Keyword(k) => format!("{k:?}").hash(&mut h),
            Kind::List(items) | Kind::Vector(items) | Kind::Set(items) | Kind::Map(items) => {
                for item in items {
                    self.form(item).hash(&mut h);
                }
            }
            other => format!("{other:?}").hash(&mut h),
        }
        let v = h.finish();
        self.forms.insert(v);
        v
    }
    fn forms(&mut self, forms: &[Form]) {
        for f in forms {
            self.form(f);
        }
    }
    fn hir(&mut self, hir: &Hir) -> u64 {
        self.add("hir-nodes-total", 1);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        hir.span.hash(&mut h);
        format!("{:?}", hir.ty).hash(&mut h);
        self.forms(&hir.metadata);
        if let Some(source) = &hir.source {
            (std::sync::Arc::as_ptr(source) as usize).hash(&mut h);
            self.source(source);
        }
        std::mem::discriminant(&hir.kind).hash(&mut h);
        let mut child = |p: &mut Self, c: &Hir| p.hir(c).hash(&mut h);
        use Expression as E;
        match &hir.kind {
            E::Future { body, .. } => child(self, body),
            E::Await { value } => child(self, value),
            E::AsyncTry { body, handler, cleanup, .. } => {
                child(self, body);
                handler.iter().for_each(|x| child(self, x));
                cleanup.iter().for_each(|x| child(self, x));
            }
            E::AsyncDynamicScope { bindings, body } | E::DynamicScope { bindings, body } => {
                bindings.iter().for_each(|(_, x)| child(self, x));
                child(self, body);
            }
            E::Bitwise { arguments, .. } | E::Comparison { arguments, .. } | E::Array { arguments, .. }
            | E::Nominal { arguments, .. } | E::Recur { arguments, .. } | E::Arithmetic { arguments, .. } => {
                arguments.iter().for_each(|x| child(self, x))
            }
            E::NilTest(x) | E::Throw(x) => child(self, x),
            E::Assign { value, .. } => child(self, value),
            E::Try { regions } => regions.iter().for_each(|x| child(self, x)),
            E::Definition { name_metadata, initializer, .. } => {
                self.forms(name_metadata);
                initializer.iter().for_each(|x| child(self, x));
            }
            E::Let { bindings, body } | E::Loop { bindings, body, .. } => {
                for b in bindings {
                    self.forms(&b.metadata);
                    child(self, &b.value);
                }
                child(self, body);
            }
            E::If { condition, consequent, alternative } => {
                child(self, condition);
                child(self, consequent);
                child(self, alternative);
            }
            E::Do(items) => items.iter().for_each(|x| child(self, x)),
            E::Function { parameters, body, .. } => {
                parameters.iter().for_each(|p| self.forms(&p.metadata));
                child(self, body);
            }
            E::GeneralFunction { methods, .. } => {
                for m in methods {
                    m.parameters.iter().for_each(|p| self.forms(&p.metadata));
                    child(self, &m.body);
                }
            }
            E::Call { callee, arguments } => {
                child(self, callee);
                arguments.iter().for_each(|x| child(self, x));
            }
            E::Literal(_) | E::Local(_) | E::Global(_) | E::GlobalOrFallback { .. } | E::GlobalCell(_) => {}
        }
        let v = h.finish();
        self.hirs.insert(v);
        v
    }
    fn arc_hir(&mut self, hir: &std::sync::Arc<Hir>) {
        if self.first("arc-hir", std::sync::Arc::as_ptr(hir)) {
            self.hir(hir);
        }
    }
    fn hirs_slice(&mut self, hirs: &std::sync::Arc<[Hir]>) {
        if self.first("arc-hir-slice", std::sync::Arc::as_ptr(hirs)) {
            hirs.iter().for_each(|x| { self.hir(x); });
        }
    }
    fn source(&mut self, s: &std::sync::Arc<SourceAnalysis>) {
        if !self.first("source-analysis", std::sync::Arc::as_ptr(s)) {
            return;
        }
        self.form(&s.form);
        if let Some(n) = &s.name_hint { self.form(n); }
        for t in [&s.tags.tag, &s.tags.inferred].into_iter().flatten() { self.form(t); }
        match &s.resolved {
            Some(SourceBinding::Local(l)) => self.arc_local(l),
            Some(SourceBinding::Field(f)) => self.field(f),
            Some(SourceBinding::Global { declaration: Some(d), .. }) => self.definition(d),
            _ => {}
        }
        self.namespace(&s.scope);
        self.namespace(&s.namespace_snapshot);
        if let Some(c) = &s.callable { self.callable(c); }
        if let Some(n) = &s.node { self.node(n); }
        self.locals(&s.locals);
        self.fields(&s.fields);
        self.scopes(&s.function_scopes);
    }
    fn scopes(&mut self, scopes: &std::sync::Arc<[std::sync::Arc<FunctionScope>]>) {
        if self.first("function-scope-slice", std::sync::Arc::as_ptr(scopes)) {
            self.add("function-scope-slice-entries", scopes.len());
            for s in scopes.iter() { self.function_scope(s); }
        }
    }
    fn function_scope(&mut self, s: &std::sync::Arc<FunctionScope>) {
        if !self.first("function-scope", std::sync::Arc::as_ptr(s)) { return; }
        self.form(&s.function_form);
        self.form(&s.declaration);
        self.namespace(&s.scope);
        self.namespace(&s.namespace_snapshot);
        self.locals(&s.locals);
        self.fields(&s.fields);
        self.scopes(&s.parents);
        if let Some(b) = &s.self_binding { self.local(b); }
    }
    fn callable(&mut self, c: &std::sync::Arc<SourceCallable>) {
        if !self.first("callable", std::sync::Arc::as_ptr(c)) { return; }
        if let Some(n) = &c.name { self.function_scope(n); }
        for m in &c.methods {
            self.form(&m.form);
            self.namespace(&m.environment.scope);
            self.namespace(&m.environment.namespace_snapshot);
            self.locals(&m.environment.locals);
            self.fields(&m.environment.fields);
            self.scopes(&m.environment.function_scopes);
            m.declarations.iter().for_each(|d| self.local(d));
            m.parameters.iter().for_each(|p| self.forms(&p.metadata));
            self.arc_hir(&m.body);
        }
    }
    fn node(&mut self, n: &std::sync::Arc<SourceNode>) {
        if !self.first("source-node", std::sync::Arc::as_ptr(n)) { return; }
        use SourceNode as N;
        match &**n {
            N::Future(x) | N::Await(x) | N::Throw(x) => self.arc_hir(x),
            N::Vector(xs) | N::Map(xs) | N::Set(xs) | N::Recur(xs) => self.hirs_slice(xs),
            N::Do { statements, result } => { self.hirs_slice(statements); self.arc_hir(result); }
            N::If { condition, consequent, alternative } => { self.arc_hir(condition); self.arc_hir(consequent); self.arc_hir(alternative); }
            N::Bindings { bindings, body, .. } => {
                if self.first("local-slice", std::sync::Arc::as_ptr(bindings)) { bindings.iter().for_each(|b| self.local(b)); }
                self.arc_hir(body);
            }
            N::Try { body, handler, cleanup, payload } => {
                self.arc_hir(body); self.arc_hir(handler);
                if let Some(c) = cleanup { self.arc_hir(c); }
                if let Some(p) = payload { self.arc_local(p); }
            }
            N::Assign { target, value } => { self.arc_hir(target); self.arc_hir(value); }
            N::Invoke { callee: a, arguments } | N::Construct { class: a, arguments } => { self.arc_hir(a); self.hirs_slice(arguments); }
            N::WithMeta { expression, metadata, inner } => {
                self.arc_hir(expression); self.arc_hir(metadata);
                if let Some(i) = inner { self.node(i); }
            }
        }
    }
    fn arc_local(&mut self, l: &std::sync::Arc<LocalBinding>) {
        if self.first("arc-local", std::sync::Arc::as_ptr(l)) { self.local(l); }
    }
    fn local(&mut self, l: &LocalBinding) {
        self.add("local-binding-values", 1);
        self.first("local-identity", std::sync::Arc::as_ptr(&l.identity));
        self.form(&l.declaration);
        if let Some(i) = &l.initializer { self.arc_hir(i); }
        if let Some(s) = &l.shadow { self.arc_local(s); }
        if let Some(f) = &l.shadow_field { self.field(f); }
    }
    fn field(&mut self, f: &FieldBinding) {
        self.add("field-binding-values", 1);
        if self.first("field-identity", std::sync::Arc::as_ptr(&f.identity)) {
            self.form(&f.declaration);
            self.hir(&f.access);
        }
    }
    fn locals(&mut self, m: &std::sync::Arc<HashMap<String, LocalBinding>>) {
        if self.first("locals-map", std::sync::Arc::as_ptr(m)) {
            self.add("locals-map-entries", m.len());
            m.values().for_each(|l| self.local(l));
        }
    }
    fn fields(&mut self, m: &std::sync::Arc<HashMap<String, FieldBinding>>) {
        if self.first("fields-map", std::sync::Arc::as_ptr(m)) {
            self.add("fields-map-entries", m.len());
            m.values().for_each(|f| self.field(f));
        }
    }
    fn namespace(&mut self, n: &std::sync::Arc<SourceNamespace>) {
        if !self.first("source-namespace", std::sync::Arc::as_ptr(n)) { return; }
        self.add("namespace-declarations", n.declarations.len());
        self.add("namespace-identities", n.identities.len());
        self.add("namespace-refers", n.refers.len() + n.aliases.len() + n.excluded_core.len());
        for d in n.declarations.values() { self.definition(d); }
    }
    fn definition(&mut self, d: &std::sync::Arc<DefinitionInfo>) {
        if !self.first("definition", std::sync::Arc::as_ptr(d)) { return; }
        if let Some(o) = &d.origin { if o.metadata_initialized() { self.add("origin-metadata-initialized", 1); } if o.reader_forms().is_some() { self.add("origin-with-reader-forms", 1); } }
        self.form(&d.definition_form);
        self.form(&d.declaration);
        if let Some(i) = &d.initializer_form { self.form(i); }
        if let Some(i) = &d.initializer { self.arc_hir(i); }
    }
}

#[test]
fn core_analysis_size() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let start = std::time::Instant::now();
        let analyzed = super::bootstrap::catalog(phase).unwrap();
        let analysis = start.elapsed();
        let mut p = Probe::default();
        for d in analyzed.environment.probe_definitions() {
            p.definition(d);
        }
        let mut rows: Vec<_> = p.seen.iter().map(|(k, v)| (k.to_string(), v.len())).collect();
        rows.extend(p.counts.iter().map(|(k, v)| (k.to_string(), *v)));
        rows.push(("form-nodes-distinct".into(), p.forms.len()));
        rows.push(("hir-nodes-distinct".into(), p.hirs.len()));
        rows.sort();
        eprintln!("== {phase:?}: catalog {analysis:?}");
        for (k, v) in rows {
            eprintln!("  {k:32} {v}");
        }
    }
}

#[test]
fn core_analysis_round_trip() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let start = std::time::Instant::now();
        let analyzed = super::bootstrap::catalog(phase).unwrap();
        let analysis = start.elapsed();
        let source = super::bootstrap::SOURCE;
        let start = std::time::Instant::now();
        let text = super::core_analysis::encode(&analyzed, source, phase, 0).unwrap();
        let encode = start.elapsed();
        let base = super::SourceOrigin::privileged(source);
        if phase == Phase::Runtime { std::fs::write("/tmp/claude-1000/-home-bobby-Work-code-bobby-suss/4394e53b-cbb3-43be-89a3-3cb265d9ecaf/scratchpad/core-runtime.edn", &text).unwrap(); }
        let start = std::time::Instant::now();
        let read = suss_reader::edn::read_all(&text, &suss_reader::edn::Options { work_allowance: u64::MAX, max_depth: 64, ..Default::default() }).unwrap();
        let parse = start.elapsed();
        drop(read);
        let start = std::time::Instant::now();
        super::core_analysis::PROBE.with(|p| p.borrow_mut().clear());
        let decoded = super::core_analysis::decode(&text, phase, base).unwrap();
        let decode = start.elapsed();
        let fragment = super::AnalyzedFragment { hir: analyzed.hir.clone(), environment: decoded.environment, cells: decoded.cells, namespace_directive: decoded.namespace_directive };
        let again = super::core_analysis::encode(&fragment, source, phase, 0).unwrap();
        let start = std::time::Instant::now();
        drop(fragment);
        eprintln!("drop decoded graph {:?}", start.elapsed());
        let start = std::time::Instant::now();
        drop(analyzed);
        eprintln!("drop analyzed graph {:?}", start.elapsed());
        let analyzed = ();
        let _ = analyzed;
        eprintln!("== {phase:?}: analysis {analysis:?}, encode {encode:?}, parse {parse:?}, parse+decode {decode:?}, {} bytes, {} lines", text.len(), text.lines().count());
        if text != again {
            let strip = |t: &str| t.lines().map(|l| { let mut parts: Vec<&str> = l.splitn(3, ' ').collect(); if parts.len() == 3 && (parts[0] == "[2" || parts[0] == "[5") { parts.remove(1); } parts.join(" ") }).collect::<Vec<_>>();
            let (a, b) = (strip(&text), strip(&again));
            let first = a.iter().zip(&b).position(|(x, y)| x != y);
            eprintln!("lines {} vs {}; first stripped difference at {first:?}", a.len(), b.len());
            if let Some(i) = first { eprintln!("  {}\n  {}", a[i], b[i]); }
            assert!(first.is_none() && a.len() == b.len(), "re-encoding differs");
        }
        super::core_analysis::PROBE.with(|p| { for (t, (d, n)) in p.borrow_mut().iter().map(|(t,v)| (*t,*v)).collect::<Vec<_>>() { eprintln!("  {t:?} {d:?} {n}"); } });
    }
}
