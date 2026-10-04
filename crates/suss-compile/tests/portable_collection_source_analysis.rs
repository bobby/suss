//! Compiler-only expansion observations complement the executing macro AST probe.
use std::sync::Arc;
use suss_compile::portable::{
    self, hir,
    resolve::{Environment, Phase},
};
use suss_reader::forms::{Form, Kind, read_forms};

#[derive(Default)]
struct Observe {
    entries: Vec<f64>,
    initializer: Option<Arc<hir::SourceAnalysis>>,
}
impl portable::ExpansionHost for Observe {
    fn expand(
        &mut self,
        form: &Form,
        context: portable::ExpansionContext<'_>,
    ) -> Result<Option<Form>, portable::Diagnostic> {
        let Kind::List(items) = &form.kind else {
            return Ok(None);
        };
        let Some(Form {
            kind: Kind::Symbol(head),
            ..
        }) = items.first()
        else {
            return Ok(None);
        };
        if head.namespace.is_some() {
            return Ok(None);
        }
        match head.name.as_str() {
            "mark" => {
                assert_eq!(items.len(), 2);
                let Kind::Number(value) = &items[1].kind else {
                    panic!("numeric mark required")
                };
                self.entries.push(*value);
                Ok(Some(items[1].clone()))
            }
            "inspect" => {
                assert_eq!(items.len(), 2);
                let Kind::Symbol(binding) = &items[1].kind else {
                    panic!("binding required")
                };
                let initializer = context.locals[&binding.name].initializer.as_ref().unwrap();
                assert!(self.initializer.is_none(), "inspection must occur once");
                self.initializer = initializer.source.clone();
                Ok(Some(read_forms("nil").unwrap().remove(0)))
            }
            _ => Ok(None),
        }
    }
}

#[test]
fn source_collection_analysis_preserves_forms_and_expands_entries_once_in_textual_order() {
    // Exercise empty and small/large collection literals in each caller phase.
    // These callbacks observe compile-time expansion, not runtime initialization.
    for phase in [Phase::Runtime, Phase::Macro] {
        for (open, close, count, tag) in [
            ("[", "]", 0, "IVector"),
            ("[", "]", 5, "IVector"),
            ("[", "]", 33, "IVector"),
            ("{", "}", 0, "IMap"),
            ("{", "}", 4, "IMap"),
            ("{", "}", 18, "IMap"),
            ("#{", "}", 0, "ISet"),
            ("#{", "}", 3, "ISet"),
            ("#{", "}", 9, "ISet"),
        ] {
            let mut environment = Environment::default();
            for class in [
                "PersistentVector",
                "PersistentArrayMap",
                "PersistentHashMap",
                "PersistentHashSet",
            ] {
                environment.declare_cell(phase, "suss.core", class).unwrap();
            }
            let entries = (0..count)
                .map(|index| format!("(mark {index})"))
                .collect::<Vec<_>>()
                .join(" ");
            let collection = format!("{open}{entries}{close}");
            let source = format!("(let [copy {collection}] (inspect copy))");
            let mut host = Observe::default();
            portable::prepare_fragment_forms_with_expander(
                read_forms(&source).unwrap(),
                0..source.len(),
                &environment,
                phase,
                &mut host,
            )
            .unwrap();
            assert_eq!(
                host.entries,
                (0..count).map(|index| index as f64).collect::<Vec<_>>(),
                "{source}"
            );
            let initializer = host.initializer.unwrap();
            assert_eq!(initializer.phase, phase);
            assert_eq!(&source[initializer.form.span.clone()], collection);
            let forms = read_forms(&source).unwrap();
            let Kind::List(let_items) = &forms[0].kind else {
                panic!("let form required")
            };
            let Kind::Vector(bindings) = &let_items[1].kind else {
                panic!("bindings required")
            };
            assert_eq!(initializer.form, bindings[1]);
            let children = match (open, initializer.node.as_deref().unwrap()) {
                ("[", hir::SourceNode::Vector(children))
                | ("{", hir::SourceNode::Map(children))
                | ("#{", hir::SourceNode::Set(children)) => children,
                _ => panic!("wrong original source collection kind"),
            };
            assert_eq!(children.len(), count);
            for (index, child) in children.iter().enumerate() {
                let analyzed = child.source.as_ref().unwrap();
                assert_eq!(analyzed.phase, phase);
                assert_eq!(analyzed.context, portable::AnalysisContext::Expression);
                assert_eq!(analyzed.form.kind, Kind::Number(index as f64));
            }
            let Kind::Symbol(actual_tag) = &initializer.tags.tag.as_ref().unwrap().kind else {
                panic!("collection source tag required")
            };
            assert_eq!(actual_tag.namespace.as_deref(), Some("cljs.core"));
            assert_eq!(actual_tag.name, tag);
        }
    }
}

#[test]
fn metadata_wrapper_retains_separate_expression_and_metadata_analysis() {
    for phase in [Phase::Runtime, Phase::Macro] {
        for literal in [
            "^{:custom (mark 1)} [(mark 0)]",
            "^{:custom (mark 1)} (fn* [] (mark 0))",
        ] {
            let mut environment = Environment::default();
            for name in [
                "PersistentVector",
                "PersistentArrayMap",
                "Keyword",
                "with-meta",
            ] {
                environment.declare_cell(phase, "suss.core", name).unwrap();
            }
            let source = format!("(let [outer 7 copy {literal}] (inspect copy))");
            let forms = read_forms(&source).unwrap();
            let Kind::List(parts) = &forms[0].kind else {
                panic!("let required")
            };
            let Kind::Vector(bindings) = &parts[1].kind else {
                panic!("bindings required")
            };
            let original = bindings[3].clone();
            let mut host = Observe::default();
            portable::prepare_fragment_forms_with_expander(
                forms,
                0..source.len(),
                &environment,
                phase,
                &mut host,
            )
            .unwrap();
            assert_eq!(
                host.entries,
                [0.0, 1.0],
                "expression and metadata analyzed once"
            );
            let wrapper = host.initializer.unwrap();
            assert_eq!(wrapper.form, original);
            assert_eq!(wrapper.phase, phase);
            assert!(
                wrapper.tags.tag.is_none(),
                "wrapper has no raw collection/function tag"
            );
            let hir::SourceNode::WithMeta {
                expression,
                metadata,
                ..
            } = wrapper.node.as_deref().unwrap()
            else {
                panic!("metadata wrapper required")
            };
            let expression = expression.source.as_ref().unwrap();
            let metadata = metadata.source.as_ref().unwrap();
            assert_eq!(expression.form, original);
            assert_eq!(expression.phase, phase);
            assert_eq!(metadata.phase, phase);
            assert_eq!(expression.context, portable::AnalysisContext::Expression);
            assert_eq!(metadata.context, portable::AnalysisContext::Expression);
            assert!(Arc::ptr_eq(&expression.scope, &wrapper.scope));
            assert!(Arc::ptr_eq(&metadata.scope, &wrapper.scope));
            assert_eq!(expression.locals["outer"].id, wrapper.locals["outer"].id);
            assert_eq!(metadata.locals["outer"].id, wrapper.locals["outer"].id);
            assert!(
                matches!(metadata.node.as_deref(), Some(hir::SourceNode::Map(entries)) if entries.len() == 2)
            );
            assert!(metadata.form.metadata.is_empty());
            if literal.contains("fn*") {
                assert!(expression.callable.is_some());
                assert!(expression.node.is_none());
            } else {
                assert!(
                    matches!(expression.node.as_deref(), Some(hir::SourceNode::Vector(entries)) if entries.len() == 1)
                );
            }
        }
    }
}

#[test]
fn effectful_collection_child_retains_analyzed_operands_without_reexpansion() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut environment = Environment::default();
        environment
            .declare_cell(phase, "suss.core", "PersistentVector")
            .unwrap();
        environment.declare_cell(phase, "user", "effects").unwrap();
        let source = "(let [copy [(do (set! effects (+ (mark 0) (mark 1))) 42)]] (inspect copy))";
        let mut host = Observe::default();
        portable::prepare_fragment_forms_with_expander(
            read_forms(source).unwrap(),
            0..source.len(),
            &environment,
            phase,
            &mut host,
        )
        .unwrap();
        assert_eq!(host.entries, [0.0, 1.0]);
        let collection = host.initializer.unwrap();
        let hir::SourceNode::Vector(items) = collection.node.as_deref().unwrap() else {
            panic!("vector required")
        };
        let body = items[0].source.as_ref().unwrap();
        let hir::SourceNode::Do { statements, result } = body.node.as_deref().unwrap() else {
            panic!("do required")
        };
        assert_eq!(statements.len(), 1);
        assert_eq!(
            result.source.as_ref().unwrap().form.kind,
            Kind::Number(42.0)
        );
        let assignment = statements[0].source.as_ref().unwrap();
        assert_eq!(assignment.context, portable::AnalysisContext::Statement);
        assert!(assignment.tags.tag.is_none());
        assert!(assignment.tags.inferred.is_none());
        let hir::SourceNode::Assign { target, value } = assignment.node.as_deref().unwrap() else {
            panic!("assignment required")
        };
        assert_eq!(
            target.source.as_ref().unwrap().form.kind,
            read_forms("effects").unwrap()[0].kind
        );
        let call = value.source.as_ref().unwrap();
        let hir::SourceNode::Invoke { callee, arguments } = call.node.as_deref().unwrap() else {
            panic!("source invocation required")
        };
        assert!(
            matches!(&callee.source.as_ref().unwrap().form.kind, Kind::Symbol(symbol) if symbol.name == "+")
        );
        assert_eq!(arguments.len(), 2);
        for (index, argument) in arguments.iter().enumerate() {
            let argument = argument.source.as_ref().unwrap();
            assert_eq!(argument.form.kind, Kind::Number(index as f64));
            assert_eq!(argument.context, portable::AnalysisContext::Expression);
            assert_eq!(argument.phase, phase);
        }
    }
}

#[test]
fn empty_do_return_keeps_the_enclosing_source_context() {
    use portable::AnalysisContext::{Expression, Return, Statement};
    for phase in [Phase::Runtime, Phase::Macro] {
        for (initializer, context) in [
            ("(do)", Expression),
            ("(do (do) 1)", Statement),
            ("(fn* [] (do))", Return),
        ] {
            let source = format!("(let [copy {initializer}] (inspect copy))");
            let mut host = Observe::default();
            portable::prepare_fragment_forms_with_expander(
                read_forms(&source).unwrap(),
                0..source.len(),
                &Environment::default(),
                phase,
                &mut host,
            )
            .unwrap();
            let source = host.initializer.unwrap();
            let empty = match context {
                Expression => source.clone(),
                Statement => {
                    let hir::SourceNode::Do { statements, .. } = source.node.as_deref().unwrap()
                    else {
                        panic!("outer do required")
                    };
                    statements[0].source.clone().unwrap()
                }
                Return => {
                    let callable = source.callable.as_ref().unwrap();
                    let hir::Expression::Do(items) = &callable.methods[0].body.kind else {
                        panic!("method body required")
                    };
                    items[0].source.clone().unwrap()
                }
            };
            assert_eq!(empty.context, context);
            let hir::SourceNode::Do { statements, result } = empty.node.as_deref().unwrap() else {
                panic!("empty do required")
            };
            assert!(statements.is_empty());
            let nil = result.source.as_ref().unwrap();
            assert_eq!(nil.form.kind, Kind::Nil);
            assert_eq!(
                nil.context, context,
                "implicit nil keeps empty do context in {initializer}"
            );
            assert_eq!(nil.phase, phase);
            for tags in [&empty.tags, &nil.tags] {
                let clj_nil = read_forms("clj-nil").unwrap().remove(0);
                assert_eq!(tags.tag.as_ref().unwrap().kind, clj_nil.kind);
                assert_eq!(tags.inferred.as_ref().unwrap().kind, clj_nil.kind);
            }
            assert!(Arc::ptr_eq(&nil.scope, &empty.scope));
        }
    }
}
