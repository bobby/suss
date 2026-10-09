// Copyright (c) Rich Hickey. All rights reserved.
// The use and distribution terms for this software are covered by the
// Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
// which can be found in the file epl-v10.html at the root of this distribution.
// By using this software in any fashion, you are agreeing to be bound by
// the terms of this license.
// You must not remove this notice, or any other, from this software.
// SPDX-License-Identifier: EPL-1.0
//
// Binding expansion adapted from pinned cljs/core.cljc destructure and
// maybe-destructured at c4295f303100bbf5afac449242d30bca1126f1a1.
// Runtime operations remain ordinary live source calls. No values are extracted
// by the host; the inner let* executes again on each method-loop recurrence.
use super::*;
use suss_reader::{Keyword, Symbol};

struct Expansion {
    next: usize,
    occupied: BTreeSet<String>,
}
fn make(parent: &Form, kind: Kind) -> Form {
    Form {
        span: parent.span.clone(),
        metadata: Vec::new(),
        kind,
    }
}
fn symbol(parent: &Form, name: &str) -> Form {
    make(parent, Kind::Symbol(Symbol::parse(name)))
}
fn call(parent: &Form, name: &str, arguments: Vec<Form>) -> Form {
    let mut parts = vec![symbol(parent, name)];
    parts.extend(arguments);
    make(parent, Kind::List(parts))
}
fn keyword(form: &Form, name: &str) -> bool {
    matches!(&form.kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == name)
}
fn ampersand(form: &Form) -> bool {
    matches!(&form.kind, Kind::Symbol(name) if name.namespace.is_none() && name.name == "&")
}
fn collect_names(form: &Form, names: &mut BTreeSet<String>) {
    if let Kind::Symbol(name) = &form.kind {
        names.insert(name.name.clone());
    }
    match &form.kind {
        Kind::List(items) | Kind::Vector(items) | Kind::Map(items) | Kind::Set(items) => {
            for item in items {
                collect_names(item, names);
            }
        }
        Kind::Prefix { operator, target } => {
            collect_names(operator, names);
            collect_names(target, names);
        }
        _ => {}
    }
    for metadata in &form.metadata {
        collect_names(metadata, names);
    }
}

// Compiler syntax map operations mirror the pinned small-map -> HAMT transition.
// Stable hash collisions retain insertion order; metadata/spans are not key data.
#[derive(Clone)]
struct BindingMap {
    entries: Vec<(Form, Form)>,
    hashed: bool,
}
fn equal(a: &Form, b: &Form) -> bool {
    match (&a.kind, &b.kind) {
        (Kind::List(a) | Kind::Vector(a), Kind::List(b) | Kind::Vector(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| equal(a, b))
        }
        (Kind::Map(a), Kind::Map(b)) => {
            a.len() == b.len()
                && a.chunks_exact(2).all(|entry| {
                    b.chunks_exact(2)
                        .any(|other| equal(&entry[0], &other[0]) && equal(&entry[1], &other[1]))
                })
        }
        (Kind::Set(a), Kind::Set(b)) => {
            a.len() == b.len() && a.iter().all(|a| b.iter().any(|b| equal(a, b)))
        }
        _ => a.kind == b.kind,
    }
}
fn mix_word(word: u32) -> u32 {
    word.wrapping_mul(0xcc9e2d51)
        .rotate_left(15)
        .wrapping_mul(0x1b873593)
}
fn collection_hash(basis: u32, count: usize) -> u32 {
    let mut hash = mix_word(basis)
        .rotate_left(13)
        .wrapping_mul(5)
        .wrapping_add(0xe6546b64)
        ^ count as u32;
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x85ebca6b);
    hash ^= hash >> 13;
    hash = hash.wrapping_mul(0xc2b2ae35);
    hash ^ (hash >> 16)
}
fn syntax_hash(form: &Form) -> u32 {
    match &form.kind {
        Kind::Symbol(name) => identifier_hash(name.namespace.as_deref(), &name.name, false) as u32,
        Kind::Keyword(name) => identifier_hash(name.namespace.as_deref(), &name.name, true) as u32,
        Kind::Nil => 0,
        Kind::Bool(value) => {
            if *value {
                1231
            } else {
                1237
            }
        }
        Kind::String(units) => {
            let hash = units.iter().fold(0u32, |hash, &unit| {
                hash.wrapping_mul(31).wrapping_add(unit as u32)
            });
            if hash == 0 {
                0
            } else {
                collection_hash(hash, 4)
            }
        }
        Kind::Number(value) => {
            if value.is_finite() && value.fract() == 0.0 && value.abs() <= 9007199254740991.0 {
                (*value % 2147483647.0) as i64 as u32
            } else if value.is_nan() {
                2146959360
            } else if *value == f64::INFINITY {
                2146435072
            } else if *value == f64::NEG_INFINITY {
                (-1048576i32) as u32
            } else {
                let bits = value.to_bits();
                ((bits as u32) ^ (bits >> 32) as u32).swap_bytes()
            }
        }
        Kind::Vector(items) | Kind::List(items) => collection_hash(
            items.iter().fold(1u32, |hash, item| {
                hash.wrapping_mul(31).wrapping_add(syntax_hash(item))
            }),
            items.len(),
        ),
        Kind::Set(items) => collection_hash(
            items
                .iter()
                .fold(0u32, |hash, item| hash.wrapping_add(syntax_hash(item))),
            items.len(),
        ),
        Kind::Map(items) => collection_hash(
            items.chunks_exact(2).fold(0u32, |hash, entry| {
                hash.wrapping_add(collection_hash(
                    31u32
                        .wrapping_mul(31u32.wrapping_add(syntax_hash(&entry[0])))
                        .wrapping_add(syntax_hash(&entry[1])),
                    2,
                ))
            }),
            items.len() / 2,
        ),
        Kind::Prefix { operator, target } => collection_hash(
            31u32
                .wrapping_mul(31u32.wrapping_add(syntax_hash(operator)))
                .wrapping_add(syntax_hash(target)),
            2,
        ),
        _ => unreachable!("Reader conditionals/discards must be resolved before analysis"),
    }
}
impl BindingMap {
    fn new(items: &[Form]) -> Self {
        let mut map = Self {
            entries: Vec::new(),
            hashed: false,
        };
        for pair in items.chunks_exact(2) {
            map.assoc(pair[0].clone(), pair[1].clone());
        }
        map
    }
    fn assoc(&mut self, key: Form, value: Form) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|(candidate, _)| equal(candidate, &key))
        {
            entry.1 = value;
        } else {
            self.entries.push((key, value));
            if self.entries.len() > 8 {
                self.hashed = true;
            }
        }
    }
    fn dissoc(&mut self, key: &Form) {
        self.entries.retain(|(candidate, _)| !equal(candidate, key));
    }
    fn ordered(&self) -> Vec<(Form, Form)> {
        let mut entries = self.entries.clone();
        if self.hashed {
            entries.sort_by_key(|(key, _)| {
                let hash = syntax_hash(key);
                std::array::from_fn::<_, 7, _>(|i| (hash >> (i * 5)) & 31)
            });
        }
        entries
    }
}
impl Expansion {
    fn fresh(&mut self, parent: &Form, stem: &str) -> Form {
        loop {
            let name = format!("__suss_destructure_{stem}_{}", self.next);
            self.next += 1;
            if self.occupied.insert(name.clone()) {
                return symbol(parent, &name);
            }
        }
    }
    fn local(pattern: &Form) -> Option<Form> {
        let name = match &pattern.kind {
            Kind::Symbol(name) => &name.name,
            Kind::Keyword(name) => &name.name,
            _ => return None,
        };
        let mut local = symbol(pattern, name);
        if matches!(&pattern.kind, Kind::Symbol(name) if name.namespace.is_none()) {
            local.metadata = pattern.metadata.clone();
        }
        Some(local)
    }
    fn bind(
        &mut self,
        bindings: &mut Vec<Form>,
        pattern: &Form,
        value: Form,
    ) -> Result<(), Diagnostic> {
        if let Some(local) = Self::local(pattern) {
            bindings.extend([local, value]);
            return Ok(());
        }
        match &pattern.kind {
            Kind::Vector(items) => self.vector(bindings, pattern, items, value),
            Kind::Map(items) => self.map(bindings, pattern, items, value),
            _ => Err(fail(
                pattern.span.clone(),
                "Unsupported destructuring binding form",
            )),
        }
    }
    fn vector(
        &mut self,
        bindings: &mut Vec<Form>,
        pattern: &Form,
        items: &[Form],
        value: Form,
    ) -> Result<(), Diagnostic> {
        let vector = self.fresh(pattern, "vec");
        let sequence = self.fresh(pattern, "seq");
        let first = self.fresh(pattern, "first");
        bindings.extend([vector.clone(), value]);
        let has_rest = items.iter().any(ampersand);
        if has_rest {
            bindings.extend([
                sequence.clone(),
                call(pattern, "cljs.core/seq", vec![vector.clone()]),
            ]);
        }
        let mut index = 0;
        let mut cursor = 0;
        let mut seen_rest = false;
        while cursor < items.len() {
            let item = &items[cursor];
            if keyword(item, "as") || ampersand(item) {
                let Some(target) = items.get(cursor + 1) else {
                    return Err(fail(
                        item.span.clone(),
                        "Destructuring marker requires a binding form",
                    ));
                };
                if keyword(item, "as") {
                    self.bind(bindings, target, vector.clone())?;
                    // Pinned :as terminates the binding vector.
                    return Ok(());
                }
                self.bind(bindings, target, sequence.clone())?;
                seen_rest = true;
                cursor += 2;
                continue;
            }
            if seen_rest {
                return Err(fail(
                    item.span.clone(),
                    "Only :as can follow a destructuring rest parameter",
                ));
            }
            let extracted = if has_rest {
                // next runs before nested extraction, exactly as pinned pvec.
                bindings.extend([
                    first.clone(),
                    call(item, "cljs.core/first", vec![sequence.clone()]),
                    sequence.clone(),
                    call(item, "cljs.core/next", vec![sequence.clone()]),
                ]);
                first.clone()
            } else {
                call(
                    item,
                    "cljs.core/nth",
                    vec![
                        vector.clone(),
                        make(item, Kind::Number(index as f64)),
                        make(item, Kind::Nil),
                    ],
                )
            };
            self.bind(bindings, item, extracted)?;
            index += 1;
            cursor += 1;
        }
        Ok(())
    }
    fn map(
        &mut self,
        bindings: &mut Vec<Form>,
        pattern: &Form,
        items: &[Form],
        value: Form,
    ) -> Result<(), Diagnostic> {
        if items.len() % 2 != 0 {
            return Err(fail(
                pattern.span.clone(),
                "Map destructuring requires paired entries",
            ));
        }
        let map = self.fresh(pattern, "map");
        bindings.extend([
            map.clone(),
            value,
            map.clone(),
            call(pattern, "cljs.core/--destructure-map", vec![map.clone()]),
        ]);
        let defaults = items
            .chunks_exact(2)
            .find(|entry| keyword(&entry[0], "or"))
            .map(|entry| &entry[1]);
        if let Some(alias) = items.chunks_exact(2).find(|entry| keyword(&entry[0], "as")) {
            if !matches!(alias[1].kind, Kind::Nil | Kind::Bool(false)) {
                bindings.extend([alias[1].clone(), map.clone()]);
            }
        }
        let mut entries = BindingMap::new(items);
        for key in ["as", "or"] {
            entries.dissoc(&make(pattern, Kind::Keyword(Keyword::new(key))));
        }
        let mut transforms = BindingMap {
            entries: Vec::new(),
            hashed: false,
        };
        for (key, value) in BindingMap::new(items).ordered() {
            if matches!(&key.kind, Kind::Keyword(key) if matches!(key.name.as_str(), "keys" | "syms" | "strs"))
            {
                transforms.assoc(key, value);
            }
        }
        for (transform, targets) in transforms.ordered() {
            entries.dissoc(&transform);
            let Kind::Keyword(transform_key) = &transform.kind else {
                unreachable!()
            };
            let ordered_set;
            let targets = match &targets.kind {
                Kind::Vector(items) | Kind::List(items) => items,
                Kind::Set(items) => {
                    ordered_set = {
                        let mut items = items.clone();
                        items.sort_by_key(|item| {
                            let hash = syntax_hash(item);
                            std::array::from_fn::<_, 7, _>(|i| (hash >> (i * 5)) & 31)
                        });
                        items
                    };
                    &ordered_set
                }
                Kind::Nil => continue,
                _ => {
                    return Err(fail(
                        targets.span.clone(),
                        "Map shorthand requires a sequence of names",
                    ));
                }
            };
            for target in targets {
                let (namespace, name, spelling) = match &target.kind {
                    Kind::Symbol(name) => {
                        (name.namespace.clone(), name.name.clone(), name.to_string())
                    }
                    Kind::Keyword(name) => {
                        (name.namespace.clone(), name.name.clone(), name.to_string())
                    }
                    Kind::String(units) => {
                        let spelling = String::from_utf16(units).map_err(|_| {
                            fail(target.span.clone(), "Shorthand name requires valid UTF-16")
                        })?;
                        let parsed = Symbol::parse(&spelling);
                        (parsed.namespace, parsed.name, spelling)
                    }
                    _ => return Err(fail(target.span.clone(), "Unsupported map shorthand name")),
                };
                let namespace = transform_key.namespace.clone().or(namespace);
                let lookup = match transform_key.name.as_str() {
                    "keys" => make(target, Kind::Keyword(Keyword { namespace, name })),
                    "syms" => call(
                        target,
                        "quote",
                        vec![make(target, Kind::Symbol(Symbol { namespace, name }))],
                    ),
                    "strs" => make(target, Kind::String(spelling.encode_utf16().collect())),
                    _ => unreachable!(),
                };
                entries.assoc(target.clone(), lookup);
            }
        }
        for (target, lookup) in entries.ordered() {
            let mut local = Self::local(&target).unwrap_or_else(|| target.clone());
            // pmap explicitly restores named binding metadata after unqualification.
            if matches!(target.kind, Kind::Symbol(_) | Kind::Keyword(_)) {
                local.metadata = target.metadata.clone();
            }
            let mut arguments = vec![map.clone(), lookup];
            let default = match defaults.map(|form| &form.kind) {
                Some(Kind::Map(defaults)) => defaults
                    .chunks_exact(2)
                    .find(|entry| equal(&entry[0], &local))
                    .map(|entry| entry[1].clone()),
                Some(Kind::Set(defaults)) => {
                    defaults.iter().find(|item| equal(item, &local)).cloned()
                }
                _ => None,
            };
            if let Some(default) = default {
                // get's third argument is eager, even when the key exists.
                arguments.push(default);
            }
            self.bind(bindings, &local, call(&target, "cljs.core/get", arguments))?;
        }
        Ok(())
    }
    fn method(&mut self, parent: &Form, args: &[Form]) -> Result<Vec<Form>, Diagnostic> {
        let Some(Form {
            kind: Kind::Vector(parameters),
            ..
        }) = args.first()
        else {
            return Err(fail(
                parent.span.clone(),
                "Function requires a parameter vector",
            ));
        };
        if parameters
            .iter()
            .all(|parameter| matches!(parameter.kind, Kind::Symbol(_)))
        {
            return Ok(args.to_vec());
        }
        let mut names = Vec::new();
        let mut bindings = Vec::new();
        for parameter in parameters {
            if matches!(parameter.kind, Kind::Symbol(_)) {
                names.push(parameter.clone());
            } else {
                if matches!(parameter.kind, Kind::Keyword(_)) {
                    return Err(fail(
                        parameter.span.clone(),
                        "Unsupported top-level binding key",
                    ));
                }
                let temporary = self.fresh(parameter, "param");
                names.push(temporary.clone());
                self.bind(&mut bindings, parameter, temporary)?;
            }
        }
        let mut params = args[0].clone();
        params.kind = Kind::Vector(names);
        let mut body = vec![symbol(parent, "let*"), make(parent, Kind::Vector(bindings))];
        body.extend_from_slice(&args[1..]);
        Ok(vec![params, make(parent, Kind::List(body))])
    }
}
impl Analyzer<'_> {
    pub(super) fn destructured_function_arguments(
        &mut self,
        form: &Form,
        args: &[Form],
    ) -> Result<Vec<Form>, Diagnostic> {
        let mut expansion = Expansion {
            next: self.next,
            occupied: self.locals.keys().cloned().collect(),
        };
        expansion.occupied.extend(self.fields.keys().cloned());
        collect_names(form, &mut expansion.occupied);
        for arg in args {
            collect_names(arg, &mut expansion.occupied);
        }
        let offset = usize::from(matches!(
            args.first().map(|arg| &arg.kind),
            Some(Kind::Symbol(_))
        ));
        let mut result = args[..offset].to_vec();
        if matches!(args.get(offset).map(|arg| &arg.kind), Some(Kind::Vector(_))) {
            result.extend(expansion.method(form, &args[offset..])?);
        } else {
            for signature in &args[offset..] {
                let Kind::List(parts) = &signature.kind else {
                    return Err(fail(
                        signature.span.clone(),
                        "Function signature must be a list",
                    ));
                };
                let mut signature = signature.clone();
                signature.kind = Kind::List(expansion.method(&signature, parts)?);
                result.push(signature);
            }
        }
        self.next = expansion.next;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suss_reader::forms::{read_forms, resolve_conditionals};
    fn read(source: &str) -> Form {
        resolve_conditionals(read_forms(source).unwrap())
            .unwrap()
            .remove(0)
    }
    fn expand(source: &str) -> Vec<Form> {
        let form = read(source);
        let Kind::List(parts) = &form.kind else {
            panic!("method")
        };
        let mut occupied = BTreeSet::new();
        collect_names(&form, &mut occupied);
        Expansion { next: 0, occupied }
            .method(&form, parts)
            .unwrap()
    }
    fn bindings(method: &[Form]) -> &[Form] {
        let Kind::List(body) = &method[1].kind else {
            panic!("inner let*")
        };
        let Kind::Vector(bindings) = &body[1].kind else {
            panic!("binding vector")
        };
        bindings
    }
    fn call_name(form: &Form) -> Option<&str> {
        let Kind::List(parts) = &form.kind else {
            return None;
        };
        let Kind::Symbol(name) = &parts[0].kind else {
            return None;
        };
        Some(&name.name)
    }
    #[test]
    fn parameter_binding_vector_rest_advances_before_nested_extraction() {
        let expanded = expand("([[[a] & [b] :as all]] [a b all])");
        let calls = bindings(&expanded)
            .chunks_exact(2)
            .filter_map(|pair| call_name(&pair[1]))
            .collect::<Vec<_>>();
        assert_eq!(calls, ["seq", "first", "next", "nth", "nth"]);
        let raw = &bindings(&expanded)[1];
        assert_eq!(
            bindings(&expanded).last().unwrap().kind,
            bindings(&expanded)[0].kind
        );
        let Kind::Vector(parameters) = &expanded[0].kind else {
            panic!("parameters")
        };
        assert_eq!(raw.kind, parameters[0].kind);
    }
    #[test]
    fn parameter_binding_large_map_uses_pinned_hash_order_after_dissoc() {
        let expanded = expand("([{a :a b :b c :c d :d e :e f :f g :g :as all :or {a 0}}] a)");
        let locals = bindings(&expanded)
            .chunks_exact(2)
            .filter(|pair| call_name(&pair[1]) == Some("get"))
            .map(|pair| {
                let Kind::Symbol(name) = &pair[0].kind else {
                    panic!("local")
                };
                name.name.as_str()
            })
            .collect::<Vec<_>>();
        assert_eq!(locals, ["a", "e", "c", "g", "b", "d", "f"]);
        let get_a = bindings(&expanded)
            .chunks_exact(2)
            .find(|pair| matches!(&pair[0].kind, Kind::Symbol(name) if name.name == "a"))
            .unwrap();
        let Kind::List(arguments) = &get_a[1].kind else {
            panic!("get")
        };
        assert_eq!(arguments.len(), 4, "eager default remains a get argument");
    }
    #[test]
    fn parameter_binding_qualified_shorthand_and_hygiene_preserve_local_metadata() {
        let expanded = expand(
            "([__suss_destructure_param_0 {:box/keys [^:tag a other/b] :strs [other/c]}] [a b c])",
        );
        let Kind::Vector(parameters) = &expanded[0].kind else {
            panic!("parameters")
        };
        assert_ne!(parameters[0].kind, parameters[1].kind);
        let local = bindings(&expanded)
            .chunks_exact(2)
            .find(|pair| matches!(&pair[0].kind, Kind::Symbol(name) if name.name == "a"))
            .unwrap();
        assert_eq!(local[0].metadata.len(), 1);
        let Kind::List(get) = &local[1].kind else {
            panic!("get")
        };
        assert_eq!(get[2].kind, Kind::Keyword(Keyword::namespaced("box", "a")));
    }
    #[test]
    fn parameter_binding_patterns_enter_real_hir_and_recur_keeps_raw_arity() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut environment = Environment::default();
            environment.enter_namespace(phase, "cljs.core").unwrap();
            let source = "(declare nth seq first next get --destructure-map Keyword PersistentVector PersistentArrayMap) (fn again [[n total] {:keys [step]}] (if (< n 3) (recur [(inc n) (+ total step)] {:step step}) total))";
            let forms = resolve_conditionals(read_forms(source).unwrap()).unwrap();
            let (hir, _) = prepare(&forms, 0..source.len(), &environment, phase).unwrap();
            let Expression::Do(items) = hir.kind else {
                panic!("top-level")
            };
            let Expression::GeneralFunction { methods, .. } = &items.last().unwrap().kind else {
                panic!("named method")
            };
            assert_eq!(
                methods[0].parameters.len(),
                2,
                "patterns are not extra arguments"
            );
            let Expression::Loop { body, .. } = &methods[0].body.kind else {
                panic!("method recurrence")
            };
            let Expression::Do(statements) = &body.kind else {
                panic!("method body source do")
            };
            assert!(
                matches!(statements.last().unwrap().kind, Expression::Let { .. }),
                "extraction belongs inside recurrence loop"
            );
        }
    }
    #[test]
    fn parameter_binding_primitive_fn_star_stays_strict_and_bad_rest_fails() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let env = Environment::default();
            for source in ["(fn* [[a]] a)", "(fn [[a & b c]] a)", "(fn [:a] 0)"] {
                let forms = resolve_conditionals(read_forms(source).unwrap()).unwrap();
                assert!(
                    prepare(&forms, 0..source.len(), &env, phase).is_err(),
                    "{source}"
                );
            }
        }
    }
}
