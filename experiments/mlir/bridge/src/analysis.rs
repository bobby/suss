//! Bounded source-analysis transport experiment; no runtime/SSA reconstruction.
//! Requires the isolated checkout's public source-aware HIR and suss-reader.
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc};
use suss_compile::portable::{
    self,
    hir::{self, Hir, LocalBinding, SourceBinding, SourceNode},
};
use suss_reader::forms::{Form, Kind};

type Result<T> = std::result::Result<T, String>;
const SCHEMA: &str = "suss.source-analysis.draft.v1";
const MAX_SOURCE: usize = 1 << 20;

struct Encoder {
    remaining: usize,
    identities: HashMap<usize, String>,
}
impl Encoder {
    fn new() -> Self {
        Self {
            remaining: 32768,
            identities: HashMap::new(),
        }
    }
    fn enter(&mut self, depth: usize) -> Result<()> {
        if depth > 64 {
            return Err("analysis transport depth exceeded".into());
        }
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or("analysis transport budget exceeded")?;
        Ok(())
    }
    fn identity(&mut self, b: &LocalBinding) -> String {
        let key = Arc::as_ptr(&b.identity) as usize;
        let next = format!("binding:{}", self.identities.len());
        self.identities.entry(key).or_insert(next).clone()
    }
    fn forms(&mut self, forms: &[Form], depth: usize) -> Result<Vec<Value>> {
        forms.iter().map(|f| self.form(f, depth)).collect()
    }
    fn form(&mut self, f: &Form, depth: usize) -> Result<Value> {
        self.enter(depth)?;
        let data = match &f.kind {
            Kind::Nil => json!({"tag":"nil"}),
            Kind::Bool(v) => json!({"tag":"bool","value":v}),
            Kind::Number(v) => json!({"tag":"f64","bits":format!("{:016x}",v.to_bits())}),
            Kind::String(v) => json!({"tag":"utf16","units":v}),
            Kind::Symbol(v) => json!({"tag":"symbol","namespace":v.namespace,"name":v.name}),
            Kind::Keyword(v) => json!({"tag":"keyword","namespace":v.namespace,"name":v.name}),
            Kind::List(v)
            | Kind::Vector(v)
            | Kind::Set(v)
            | Kind::Map(v)
            | Kind::Conditional(v) => {
                let tag = match &f.kind {
                    Kind::List(_) => "list",
                    Kind::Vector(_) => "vector",
                    Kind::Set(_) => "set",
                    Kind::Map(_) => "map",
                    _ => "conditional",
                };
                json!({"tag":tag,"items":self.forms(v,depth+1)?})
            }
            Kind::Discard(v) => json!({"tag":"discard","target":self.form(v,depth+1)?}),
            Kind::Prefix { operator, target } => {
                json!({"tag":"prefix","operator":self.form(operator,depth+1)?,"target":self.form(target,depth+1)?})
            }
        };
        Ok(
            json!({"span":[f.span.start,f.span.end],"metadata":self.forms(&f.metadata,depth+1)?,"data":data}),
        )
    }
    fn optional_form(&mut self, f: Option<&Form>, depth: usize) -> Result<Value> {
        Ok(match f {
            None => json!({"present":false}),
            Some(f) => json!({"present":true,"value":self.form(f,depth)?}),
        })
    }
    fn nested_form(&mut self, f: Option<&Option<Form>>, depth: usize) -> Result<Value> {
        Ok(match f {
            None => json!({"present":false}),
            Some(None) => json!({"present":true,"value":null}),
            Some(Some(f)) => json!({"present":true,"value":self.form(f,depth)?}),
        })
    }
    fn binding(&mut self, b: &LocalBinding, depth: usize) -> Result<Value> {
        self.enter(depth)?;
        if b.shadow_field.is_some() || !matches!(b.source_role, hir::SourceRole::Plain) {
            return Err("field/adapted binding roles unsupported".into());
        }
        let kind = match b.kind {
            hir::LocalKind::Let => json!({"tag":"let"}),
            hir::LocalKind::Argument { index, rest: false } => {
                json!({"tag":"argument","index":index,"rest":false})
            }
            _ => return Err("loop/rest/self/catch declarations unsupported".into()),
        };
        let id = self.identity(b);
        Ok(
            json!({"bindingId":id,"hirBindingId":b.id.0,"physicalType":ty(b.ty),
            "declaration":self.form(&b.declaration,depth+1)?,"kind":kind,
            "context":context(b.declaration_context),
            "initializer":match &b.initializer { Some(v)=>self.node(v,depth+1)?,None=>Value::Null },
            "shadow":match &b.shadow { Some(v)=>self.binding(v,depth+1)?,None=>Value::Null }}),
        )
    }
    // Selected declaration syntax/metadata, not recursive initializer HIR graphs.
    // Global identity is the actual phase/namespace/name tuple, never a local/SSA ID.
    fn global_binding(
        &mut self,
        global: &portable::resolve::Global,
        declaration: Option<&portable::resolve::DefinitionInfo>,
        depth: usize,
    ) -> Result<Value> {
        self.enter(depth)?;
        let declaration = match declaration {
            None => json!({"present":false}),
            Some(info) => {
                let origin = match &info.origin {
                    None => json!({"present":false}),
                    Some(origin) => {
                        let path = match origin.path() {
                            None => json!({"present":false}),
                            Some(path) => json!({"present":true,"value":path.to_str()
                                .ok_or("non-Unicode declaration origin path unsupported")?
                                .encode_utf16().collect::<Vec<_>>()}),
                        };
                        json!({"present":true,"value":{"sourceBytes":origin.text().len(),"path":path}})
                    }
                };
                json!({"present":true,"value":{
                    "definitionForm":self.form(&info.definition_form,depth+1)?,
                    "analysisCompleted":info.analysis_completed,
                    "declaration":self.form(&info.declaration,depth+1)?,
                    "docstring":match &info.docstring {None=>json!({"present":false}),Some(v)=>json!({"present":true,"value":v})},
                    "origin":origin,
                    "initializerForm":self.optional_form(info.initializer_form.as_ref(),depth+1)?,
                    "initializerPresent":info.initializer.is_some(),
                    "typeFields":match info.type_fields {None=>json!({"present":false}),Some(v)=>json!({"present":true,"value":v})},
                    "once":info.once,
                }})
            }
        };
        Ok(
            json!({"tag":"global","global":{"phase":phase(global.phase()),
            "namespace":global.namespace(),"name":global.name()},"declaration":declaration}),
        )
    }
    fn node(&mut self, h: &Hir, depth: usize) -> Result<Value> {
        self.enter(depth)?;
        let s = h
            .source
            .as_ref()
            .ok_or("compiler-only HIR has no source facts")?;
        if !s.fields.is_empty() || !s.function_scopes.is_empty() || s.name_hint.is_some() {
            return Err("fields/named function scopes/name hints unsupported".into());
        }
        let mut locals = s.locals.iter().collect::<Vec<_>>();
        locals.sort_by(|a, b| a.0.cmp(b.0));
        // Allocate visible identities deterministically before traversing children.
        for (_, b) in &locals {
            self.identity(b);
        }
        let locals = locals
            .into_iter()
            .map(|(name, b)| Ok(json!({"name":name,"binding":self.binding(b,depth+1)?})))
            .collect::<Result<Vec<_>>>()?;
        let resolved = match &s.resolved {
            None => Value::Null,
            Some(SourceBinding::Local(b)) => {
                json!({"tag":"local","binding":self.binding(b,depth+1)?})
            }
            Some(SourceBinding::Global {
                global,
                declaration,
            }) => self.global_binding(global, declaration.as_deref(), depth + 1)?,
            Some(SourceBinding::Field(_)) => return Err("field resolution unsupported".into()),
        };
        let mut children = Vec::new();
        let mut declarations = Vec::new();
        let op = match s.node.as_deref() {
            Some(SourceNode::Bindings {
                is_loop: false,
                bindings,
                body,
            }) => {
                for b in bindings.iter() {
                    declarations.push(self.binding(b, depth + 1)?);
                }
                for b in bindings.iter() {
                    children.push(json!({"role":"init","bindingId":self.identity(b),"node":self.node(b.initializer.as_ref().ok_or("let initializer absent")?,depth+1)?}));
                }
                children.push(json!({"role":"body","node":self.node(body,depth+1)?}));
                "let"
            }
            Some(SourceNode::Vector(items)) => {
                for item in items.iter() {
                    children.push(json!({"role":"item","node":self.node(item,depth+1)?}));
                }
                "vector"
            }
            Some(SourceNode::Invoke { callee, arguments }) => {
                children.push(json!({"role":"callee","node":self.node(callee,depth+1)?}));
                for arg in arguments.iter() {
                    children.push(json!({"role":"argument","node":self.node(arg,depth+1)?}));
                }
                "invoke"
            }
            Some(SourceNode::Do { statements, result }) => {
                for item in statements.iter() {
                    children.push(json!({"role":"statement","node":self.node(item,depth+1)?}));
                }
                children.push(json!({"role":"result","node":self.node(result,depth+1)?}));
                "do"
            }
            Some(_) => return Err("unsupported SourceNode variant".into()),
            None if s.callable.is_some() => "closure",
            None if matches!(
                s.form.kind,
                Kind::Nil | Kind::Bool(_) | Kind::Number(_) | Kind::String(_)
            ) =>
            {
                "literal"
            }
            None if matches!(s.resolved, Some(SourceBinding::Local(_))) => "local",
            None if matches!(s.form.kind, Kind::Symbol(_))
                && matches!(s.resolved, Some(SourceBinding::Global { .. })) =>
            {
                "global"
            }
            None => {
                return Err(
                    "unsupported source leaf; cannot infer facts from lowered constructor".into(),
                );
            }
        };
        let captures = if s.callable.is_some() {
            let ids = match &h.kind {
                hir::Expression::Function { captures, .. }
                | hir::Expression::GeneralFunction { captures, .. } => captures,
                _ => return Err("callable physical wrapper unsupported".into()),
            };
            ids.iter()
                .map(|id| {
                    let binding = s
                        .locals
                        .values()
                        .find(|b| b.id == *id)
                        .ok_or("capture not mapped to source declaration")?;
                    Ok(json!({"bindingId":self.identity(binding),"hirBindingId":id.0}))
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            vec![]
        };
        let callable = match &s.callable {
            None => Value::Null,
            Some(c) => {
                if c.name.is_some() || c.methods.len() != 1 || c.methods[0].variadic {
                    return Err("only anonymous single fixed method supported".into());
                }
                let m = &c.methods[0];
                if !m.environment.fields.is_empty() || !m.environment.function_scopes.is_empty() {
                    return Err("method field/name scopes unsupported".into());
                }
                let decls = m
                    .declarations
                    .iter()
                    .map(|b| self.binding(b, depth + 1))
                    .collect::<Result<Vec<_>>>()?;
                let params=m.parameters.iter().map(|p| Ok(json!({"hirBindingId":p.id.0,"name":p.name,"span":[p.span.start,p.span.end],"metadata":self.forms(&p.metadata,depth+1)?}))).collect::<Result<Vec<_>>>()?;
                let mut entry = m.environment.locals.iter().collect::<Vec<_>>();
                entry.sort_by(|a, b| a.0.cmp(b.0));
                let entry = entry
                    .into_iter()
                    .map(|(name, b)| Ok(json!({"name":name,"binding":self.binding(b,depth+1)?})))
                    .collect::<Result<Vec<_>>>()?;
                json!({"form":self.form(&m.form,depth+1)?,"declarations":decls,"parameters":params,"variadic":false,
                    "recurs":match m.recurs {None=>json!({"present":false}),Some(v)=>json!({"present":true,"value":v})},
                    "entryLocals":entry,"entryContext":context(m.environment.context),"body":self.node(&m.body,depth+1)?})
            }
        };
        Ok(
            json!({"op":op,"originalForm":self.form(&s.form,depth+1)?,"span":[h.span.start,h.span.end],
            "metadata":self.forms(&h.metadata,depth+1)?,"physicalType":ty(h.ty),"isBody":s.is_body,
            "context":context(s.context),"phase":phase(s.phase),
            "namespace":s.namespace,"scopeNamespace":s.scope.namespace,"snapshotNamespace":s.namespace_snapshot.namespace,
            "tag":self.optional_form(s.tags.tag.as_ref(),depth+1)?,"inferred":self.optional_form(s.tags.inferred.as_ref(),depth+1)?,
            "quotedConstTag":self.nested_form(s.tags.quoted_const_tag.as_ref(),depth+1)?,
            "inferredReturn":self.nested_form(s.tags.inferred_return.as_ref(),depth+1)?,
            "captures":captures,"resolved":resolved,"locals":locals,"declarations":declarations,"children":children,"callable":callable}),
        )
    }
}
fn phase(p: portable::resolve::Phase) -> &'static str {
    match p {
        portable::resolve::Phase::Runtime => "runtime",
        portable::resolve::Phase::Macro => "macro",
    }
}
fn context(c: portable::AnalysisContext) -> &'static str {
    match c {
        portable::AnalysisContext::Statement => "statement",
        portable::AnalysisContext::Expression => "expression",
        portable::AnalysisContext::Return => "return",
    }
}
fn ty(t: hir::Type) -> Value {
    match t {
        hir::Type::Nil => json!({"tag":"nil"}),
        hir::Type::Bool => json!({"tag":"bool"}),
        hir::Type::Number => json!({"tag":"number"}),
        hir::Type::String => json!({"tag":"string"}),
        hir::Type::Value => json!({"tag":"value"}),
        hir::Type::Closure(n) => json!({"tag":"closure","arity":n}),
    }
}

pub fn selected_facts(hir: &Hir) -> Result<Value> {
    Ok(json!({"schema":SCHEMA,"selectedFacts":Encoder::new().node(hir,0)?}))
}
pub fn analyze(source: &str) -> Result<Value> {
    if source.len() > MAX_SOURCE {
        return Err("source exceeds 1 MiB".into());
    }
    // Use the validated real bootstrap catalog so collection constructors resolve.
    // This installs no stub declarations and executes no guest code.
    let bootstrap = portable::bootstrap::shipped(portable::resolve::Phase::Runtime)
        .map_err(|e| e.to_string())?;
    let mut environment = bootstrap.environment.clone();
    environment
        .enter_namespace(portable::resolve::Phase::Runtime, "user")
        .map_err(|e| e.to_string())?;
    let fragment = portable::analyze_in(source, &environment, portable::resolve::Phase::Runtime)
        .map_err(|e| e.to_string())?;
    if fragment.source.is_some() {
        return selected_facts(&fragment);
    }
    // Public analyze returns a compiler-only fragment body. Select only a
    // single genuine analyzed expression; never fabricate a SourceAnalysis.
    match &fragment.kind {
        hir::Expression::Do(items) if items.len() == 1 => selected_facts(&items[0]),
        _ => Err(
            "analysis requires one genuine source expression; fragment wrapper ambiguous".into(),
        ),
    }
}
/// Public-API analysis-only probe: a synthetic outer let stages the selected
/// expression, then an expansion sentinel captures its actual initializer HIR.
/// The sentinel returns a diagnostic BEFORE IR lowering/emission can occur.
/// Source expression spans/metadata are reused without printing or rereading.
/// Context is intentionally initializer/expression, not top-level statement.
pub fn analyze_with(
    source: &str,
    environment: &portable::resolve::Environment,
    phase: portable::resolve::Phase,
    expander: &mut dyn portable::ExpansionHost,
) -> Result<Value> {
    use suss_reader::forms::{read_forms, resolve_conditionals};
    if source.len() > MAX_SOURCE {
        return Err("source exceeds 1 MiB".into());
    }
    if source.contains("__transport_probe_") {
        return Err("reserved transport probe spelling in source".into());
    }
    let mut forms = resolve_conditionals(read_forms(source).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if forms.len() != 1 {
        return Err("expander probe requires one expression, no namespace directive".into());
    }
    // Probe symbols are syntax only; the initializer is analyzed before the
    // transport local exists, so its own lexical environment is unchanged.
    let mut wrapper = read_forms("(let* [__transport_probe_value nil] (__transport_probe_stop))")
        .map_err(|e| e.to_string())?
        .remove(0);
    let Kind::List(items) = &mut wrapper.kind else {
        unreachable!()
    };
    let Kind::Vector(bindings) = &mut items[1].kind else {
        unreachable!()
    };
    bindings[1] = forms.remove(0);
    struct Probe<'a> {
        host: &'a mut dyn portable::ExpansionHost,
        captured: Option<Hir>,
    }
    impl portable::ExpansionHost for Probe<'_> {
        fn emit_fragment(
            &mut self,
            f: &portable::ir::Function,
            _: portable::resolve::Phase,
            _: &[Form],
            _: Option<&portable::SourceOrigin>,
        ) -> std::result::Result<Vec<u8>, portable::Diagnostic> {
            Err(portable::Diagnostic {
                span: f.span.clone(),
                message: "probe emission forbidden".into(),
            })
        }
        fn expand(
            &mut self,
            form: &Form,
            context: portable::ExpansionContext<'_>,
        ) -> std::result::Result<Option<Form>, portable::Diagnostic> {
            if let Kind::List(items) = &form.kind
                && items.len() == 1
                && matches!(&items[0].kind,Kind::Symbol(s) if s.namespace.is_none() && s.name=="__transport_probe_stop")
                && let Some(binding) = context.locals.get("__transport_probe_value")
            {
                self.captured = binding.initializer.as_deref().cloned();
                return Err(portable::Diagnostic {
                    span: form.span.clone(),
                    message: "analysis transport probe complete".into(),
                });
            }
            self.host.expand(form, context)
        }
    }
    let mut probe = Probe {
        host: expander,
        captured: None,
    };
    let result = portable::prepare_fragment_forms_with_expander(
        vec![wrapper],
        0..source.len(),
        environment,
        phase,
        &mut probe,
    );
    match (probe.captured, result) {
        (Some(hir), Err(e)) if e.message == "analysis transport probe complete" => {
            selected_facts(&hir)
        }
        (_, Err(e)) => Err(e.to_string()),
        _ => Err("probe did not stop analysis; unexpected compilation path".into()),
    }
}
// Deserialize the complete tree without coalescing duplicate object keys.
// MapAccess decodes escapes before returning keys, so "a" and "\\u0061"
// collide within the same object. Arrays recurse through the same visitor.
struct UniqueJson(Value);
impl<'de> serde::Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct UniqueVisitor;
        impl<'de> serde::de::Visitor<'de> for UniqueVisitor {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON with unique decoded object keys")
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(Value::Null))
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                v: bool,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(Value::Bool(v)))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(Value::Number(v.into())))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(Value::Number(v.into())))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|v| UniqueJson(Value::Number(v)))
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }
            fn visit_str<E: serde::de::Error>(
                self,
                v: &str,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(Value::String(v.to_owned())))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                v: String,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(Value::String(v)))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(UniqueJson(value)) = seq.next_element::<UniqueJson>()? {
                    values.push(value);
                }
                Ok(UniqueJson(Value::Array(values)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                use serde::de::Error as _;
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(A::Error::custom(format!("duplicate JSON key: {key}")));
                    }
                    let UniqueJson(value) = map.next_value::<UniqueJson>()?;
                    values.insert(key, value);
                }
                Ok(UniqueJson(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(UniqueVisitor)
    }
}

/// Exact comparison of all selected facts, including unknown fields and ordering.
/// The expected value must come from independent native analysis, not received JSON.
pub fn compare_transport(expected: &Value, bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_SOURCE {
        return Err("transport exceeds 1 MiB".into());
    }
    let UniqueJson(received) =
        serde_json::from_slice::<UniqueJson>(bytes).map_err(|e| e.to_string())?;
    if &received != expected {
        return Err("complete selected source facts differ".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transport_rejects_duplicate_keys_recursively() {
        // These are parser-only witnesses, not generated source-analysis facts.
        let cases = [
            (json!({"a":1}), r#"{"a":1,"a":1}"#),
            (json!({"a":1}), r#"{"a":1,"\u0061":1}"#),
            (
                json!({"outer":{"name":false}}),
                r#"{"outer":{"name":false,"na\u006de":false}}"#,
            ),
            (
                json!({"children":[{"role":"body"}]}),
                r#"{"children":[{"role":"body","role":"body"}]}"#,
            ),
            (json!({"é":null}), r#"{"é":null,"\u00e9":null}"#),
        ];
        for (expected, input) in cases {
            // A coalescing Value parser would silently accept all these inputs.
            let error = compare_transport(&expected, input.as_bytes()).unwrap_err();
            assert!(error.contains("duplicate JSON key"), "{error}");
        }
    }
    #[test]
    fn transport_accepts_unique_keys_and_preserves_values() {
        let input = br#"{"same":[{"key":false},{"key":null}],"bits":"8000000000000000","units":[55296,0,56320],"bounds":[-9223372036854775808,18446744073709551615],"real":1.5}"#;
        let expected: Value = serde_json::from_slice(input).unwrap();
        compare_transport(&expected, input).unwrap();
        // Key uniqueness is local to each object; order stays immaterial.
        compare_transport(&json!({"a":null,"b":false}), br#"{"b":false,"a":null}"#).unwrap();
    }
    #[test]
    fn transport_rejects_trailing_input_size_and_recursion_overflow() {
        assert!(compare_transport(&json!({}), b"{} {}").is_err());
        let deep = format!("{}0{}", "[".repeat(129), "]".repeat(129));
        assert!(
            compare_transport(&json!({}), deep.as_bytes())
                .unwrap_err()
                .contains("recursion limit")
        );
        assert!(
            compare_transport(&json!({}), &vec![b' '; MAX_SOURCE + 1])
                .unwrap_err()
                .contains("exceeds 1 MiB")
        );
    }
    #[test]
    fn representative_source_roundtrip_and_mutation() {
        // let/closure capture, same-name lexical shadow, vector and invocation.
        let source = "(let [^number x 7 f (fn [y] [x y false nil])] (let [x 9] (f x)))";
        let expected = analyze(source).unwrap();
        // Check genuine source vector facts, not its lowered constructor wrapper.
        fn find_vector(v: &Value) -> Option<&Value> {
            match v {
                Value::Object(o) => {
                    if o.get("op") == Some(&json!("vector")) {
                        Some(v)
                    } else {
                        o.values().find_map(find_vector)
                    }
                }
                Value::Array(a) => a.iter().find_map(find_vector),
                _ => None,
            }
        }
        let vector = find_vector(&expected).expect("original vector source node retained");
        assert!(vector["resolved"].is_null());
        let items = vector["children"].as_array().unwrap();
        assert_eq!(items.len(), 4);
        assert!(items.iter().all(|item| item["role"] == "item"));
        assert_eq!(items[0]["node"]["originalForm"]["data"]["name"], "x");
        assert_eq!(items[1]["node"]["originalForm"]["data"]["name"], "y");
        assert_eq!(
            items[2]["node"]["originalForm"]["data"],
            json!({"tag":"bool","value":false})
        );
        assert_eq!(
            items[3]["node"]["originalForm"]["data"],
            json!({"tag":"nil"})
        );
        let bytes = serde_json::to_vec(&expected).unwrap();
        compare_transport(&expected, &bytes).unwrap();
        let mut changed = expected.clone();
        changed["selectedFacts"]["physicalType"] = json!({"tag":"number"});
        assert!(compare_transport(&expected, &serde_json::to_vec(&changed).unwrap()).is_err());
        let mut changed = expected.clone();
        changed["extra"] = json!(false);
        assert!(compare_transport(&expected, &serde_json::to_vec(&changed).unwrap()).is_err());
    }
    #[test]
    fn global_source_binding_preserves_selected_declaration_and_revision_facts() {
        fn leaf(
            source: &str,
            env: &portable::resolve::Environment,
            phase: portable::resolve::Phase,
        ) -> Value {
            let h = portable::analyze_in(source, env, phase).unwrap();
            let hir::Expression::Do(items) = &h.kind else {
                panic!("actual fragment wrapper")
            };
            assert_eq!(items.len(), 1);
            selected_facts(&items[0]).unwrap()
        }
        for phase in [
            portable::resolve::Phase::Runtime,
            portable::resolve::Phase::Macro,
        ] {
            let source =
                "(def ^{:marker false :nil-marker nil} transport-global \"Global doc\" \"value\")";
            let prepared = portable::prepare_fragment(
                source,
                &portable::resolve::Environment::default(),
                phase,
            )
            .unwrap();
            let facts = leaf("transport-global", &prepared.environment, phase);
            let node = &facts["selectedFacts"];
            assert_eq!(node["op"], "global");
            let resolved = &node["resolved"];
            assert_eq!(resolved["tag"], "global");
            assert_eq!(
                resolved["global"],
                json!({"phase":super::phase(phase),"namespace":"user","name":"transport-global"})
            );
            assert_eq!(resolved["declaration"]["present"], true);
            let d = &resolved["declaration"]["value"];
            assert_eq!(d["analysisCompleted"], true);
            assert_eq!(
                d["docstring"],
                json!({"present":true,"value":"Global doc".encode_utf16().collect::<Vec<_>>()})
            );
            assert_eq!(
                d["initializerForm"]["value"]["data"],
                json!({"tag":"utf16","units":"value".encode_utf16().collect::<Vec<_>>()})
            );
            assert_eq!(d["initializerPresent"], true);
            assert_eq!(d["typeFields"], json!({"present":false}));
            assert_eq!(d["once"], false);
            assert_eq!(
                d["origin"],
                json!({"present":true,"value":{"sourceBytes":source.len(),"path":{"present":false}}})
            );
            let metadata = d["declaration"]["metadata"][0]["data"]["items"]
                .as_array()
                .unwrap();
            let pairs = metadata.chunks_exact(2).collect::<Vec<_>>();
            assert!(pairs.iter().any(|p| p[0]["data"]["name"] == "marker"
                && p[1]["data"] == json!({"tag":"bool","value":false})));
            assert!(
                pairs.iter().any(|p| p[0]["data"]["name"] == "nil-marker"
                    && p[1]["data"] == json!({"tag":"nil"}))
            );
            compare_transport(&facts, &serde_json::to_vec(&facts).unwrap()).unwrap();
            let mut changed = facts.clone();
            changed["selectedFacts"]["resolved"]["global"]["phase"] =
                json!(if phase == portable::resolve::Phase::Runtime {
                    "macro"
                } else {
                    "runtime"
                });
            assert!(compare_transport(&facts, &serde_json::to_vec(&changed).unwrap()).is_err());
            let revised = portable::prepare_fragment(
                "(def transport-global \"changed\")",
                &prepared.environment,
                phase,
            )
            .unwrap();
            let revised = leaf("transport-global", &revised.environment, phase);
            assert_eq!(
                revised["selectedFacts"]["resolved"]["global"],
                resolved["global"]
            );
            assert_ne!(
                revised["selectedFacts"]["resolved"]["declaration"],
                resolved["declaration"]
            );
            assert_eq!(
                revised["selectedFacts"]["resolved"]["declaration"]["value"]["docstring"],
                json!({"present":false})
            );
        }
    }
    #[test]
    fn bootstrap_global_resolution_retains_absent_declaration() {
        let h = portable::analyze("cljs.core/+ ").unwrap();
        let hir::Expression::Do(items) = &h.kind else {
            panic!("actual fragment wrapper")
        };
        let facts = selected_facts(&items[0]).unwrap();
        let r = &facts["selectedFacts"]["resolved"];
        assert_eq!(facts["selectedFacts"]["op"], "global");
        assert_eq!(
            r["global"],
            json!({"phase":"runtime","namespace":"suss.core","name":"+"})
        );
        assert_eq!(r["declaration"], json!({"present":false}));
    }
    #[test]
    fn source_control_pinned_representatives() {
        struct NoExpansion;
        impl portable::ExpansionHost for NoExpansion {
            fn expand(
                &mut self,
                _: &Form,
                _: portable::ExpansionContext<'_>,
            ) -> std::result::Result<Option<Form>, portable::Diagnostic> {
                Ok(None)
            }
        }
        // Existing SourceControl cases, not constructor-AST observations.
        let corpus: Value = serde_json::from_str(include_str!(
            "../../../../tests/oracle/control-source-ast-observations.json"
        ))
        .unwrap();
        assert_eq!(
            corpus["upstream"],
            "c4295f303100bbf5afac449242d30bca1126f1a1"
        );
        for (label, source, op, count) in [
            ("let-local", "(let [x 1] x)", "let", 2),
            ("let-sequential", "(let [x 1 y x] y)", "let", 3),
            ("fixed-function", "(fn* [x] x)", "closure", 0),
        ] {
            let row = corpus["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r[0] == label)
                .unwrap();
            let upstream_op = &row[1][0][0];
            assert_eq!(
                upstream_op,
                &json!([
                    "op",
                    true,
                    ["keyword", if op == "closure" { ":fn" } else { ":let" }]
                ])
            );
            let facts = analyze_with(
                source,
                &portable::resolve::Environment::default(),
                portable::resolve::Phase::Runtime,
                &mut NoExpansion,
            )
            .unwrap();
            assert_eq!(facts["selectedFacts"]["op"], op);
            assert_eq!(
                facts["selectedFacts"]["children"].as_array().unwrap().len(),
                count
            );
            compare_transport(&facts, &serde_json::to_vec(&facts).unwrap()).unwrap();
        }
    }
    #[test]
    fn lossless_forms_and_presence() {
        let mut e = Encoder::new();
        for bits in [0x8000000000000000, 0x7ff0000000000000, 0x7ff8000000000042] {
            let f = Form {
                span: 3..5,
                metadata: vec![],
                kind: Kind::Number(f64::from_bits(bits)),
            };
            assert_eq!(
                e.form(&f, 0).unwrap()["data"]["bits"],
                format!("{bits:016x}")
            );
        }
        let f = Form {
            span: 0..0,
            metadata: vec![],
            kind: Kind::String(vec![0xd800, 0, 0xdc00]),
        };
        assert_eq!(
            e.form(&f, 0).unwrap()["data"]["units"],
            json!([0xd800, 0, 0xdc00])
        );
        assert_ne!(
            e.nested_form(None, 0).unwrap(),
            e.nested_form(Some(&None), 0).unwrap()
        );
        assert_ne!(
            json!({"present":true,"value":null}),
            json!({"present":true,"value":false})
        );
    }
    #[test]
    fn unsupported_control_is_rejected() {
        assert!(analyze("(if true 1 2)").is_err());
    }
}
