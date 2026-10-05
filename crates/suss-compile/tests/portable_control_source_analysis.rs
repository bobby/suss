//! Genuine control operands retain analysis facts without re-expansion.
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
fn bootstrap_bindings_are_expanded_before_source_analysis() {
    for phase in [Phase::Runtime, Phase::Macro] {
        for (input, expected) in [("let", "let*"), ("loop", "loop*")] {
            let source = format!("(let [copy ({input} [x 1] x)] (inspect copy))");
            let mut host = Observe::default();
            portable::prepare_fragment_forms_with_expander(
                read_forms(&source).unwrap(),
                0..source.len(),
                &Environment::default(),
                phase,
                &mut host,
            )
            .unwrap();
            let initializer = host.initializer.unwrap();
            let Kind::List(items) = &initializer.form.kind else {
                panic!("expanded binding form required")
            };
            assert!(
                matches!(&items[0].kind, Kind::Symbol(head) if head.namespace.is_none() && head.name == expected),
                "{input}: {:?}",
                initializer.form
            );
        }
    }
}

#[test]
fn binding_children_share_real_declarations_and_analyzed_body_scope() {
    for phase in [Phase::Runtime, Phase::Macro] {
        for input in ["let", "loop"] {
            let source = format!("(let [copy ({input} [x (mark 0) y (mark 1)] y)] (inspect copy))");
            let mut host = Observe::default();
            portable::prepare_fragment_forms_with_expander(
                read_forms(&source).unwrap(),
                0..source.len(),
                &Environment::default(),
                phase,
                &mut host,
            )
            .unwrap();
            assert_eq!(host.entries, vec![0.0, 1.0]);
            let initializer = host.initializer.unwrap();
            let hir::SourceNode::Bindings {
                is_loop,
                bindings,
                body,
            } = initializer.node.as_ref().unwrap().as_ref()
            else {
                panic!("genuine source bindings required")
            };
            assert_eq!(*is_loop, input == "loop");
            assert_eq!(bindings.len(), 2);
            for (binding, expected) in bindings.iter().zip([0.0, 1.0]) {
                let facts = binding
                    .initializer
                    .as_ref()
                    .unwrap()
                    .source
                    .as_ref()
                    .unwrap();
                assert!(matches!(facts.form.kind, Kind::Number(value) if value == expected));
            }
            let facts = body.source.as_ref().unwrap();
            assert!(facts.is_body);
            assert!(!initializer.is_body);
            assert_eq!(facts.context, portable::AnalysisContext::Return);
            let hir::SourceNode::Do { statements, result } = facts.node.as_ref().unwrap().as_ref()
            else {
                panic!("analyzed synthetic do body required")
            };
            assert!(statements.is_empty());
            let hir::SourceBinding::Local(local) =
                result.source.as_ref().unwrap().resolved.as_ref().unwrap()
            else {
                panic!("source local return required")
            };
            assert!(Arc::ptr_eq(&local.identity, &bindings[1].identity));
            assert!(Arc::ptr_eq(
                &facts.locals["y"].identity,
                &bindings[1].identity
            ));
        }
    }
}

#[test]
fn generated_body_capture_keeps_source_and_macro_recursion_bounded() {
    struct Generate(Option<Form>);
    impl portable::ExpansionHost for Generate {
        fn expand(
            &mut self,
            form: &Form,
            _: portable::ExpansionContext<'_>,
        ) -> Result<Option<Form>, portable::Diagnostic> {
            if let Kind::List(items) = &form.kind
                && matches!(items.first().map(|item| &item.kind), Some(Kind::Symbol(name)) if name.name == "deep")
            {
                Ok(self.0.take())
            } else {
                Ok(None)
            }
        }
    }
    struct Repeat;
    impl portable::ExpansionHost for Repeat {
        fn expand(
            &mut self,
            form: &Form,
            _: portable::ExpansionContext<'_>,
        ) -> Result<Option<Form>, portable::Diagnostic> {
            Ok(Some(form.clone()))
        }
    }
    for phase in [Phase::Runtime, Phase::Macro] {
        // Generated owned forms may exceed the separate reader nesting bound.
        // Preserve a valid call-site span while testing the compiler boundary.
        let source = "(deep)";
        let mut template = read_forms("(let* [] 0)").unwrap().remove(0);
        template.span = 0..source.len();
        let Kind::List(items) = &mut template.kind else {
            unreachable!()
        };
        for item in items {
            item.span = 0..source.len();
        }
        let mut generated = read_forms("0").unwrap().remove(0);
        generated.span = 0..source.len();
        for _ in 0..80 {
            let mut outer = template.clone();
            let Kind::List(items) = &mut outer.kind else {
                unreachable!()
            };
            items[2] = generated;
            generated = outer;
        }
        let error = portable::prepare_fragment_forms_with_expander(
            read_forms(source).unwrap(),
            0..source.len(),
            &Environment::default(),
            phase,
            &mut Generate(Some(generated)),
        )
        .err()
        .expect("deep source must be bounded");
        assert!(
            error.message.contains("Bootstrap analysis expansion limit"),
            "{error:?}"
        );
        assert!(error.span.start < error.span.end && error.span.end <= source.len());
        let error = portable::prepare_fragment_forms_with_expander(
            read_forms("(again)").unwrap(),
            0..7,
            &Environment::default(),
            phase,
            &mut Repeat,
        )
        .err()
        .expect("recursive macro expansion must be bounded");
        assert!(
            error.message.contains("Bootstrap analysis expansion limit"),
            "{error:?}"
        );
        assert_eq!(error.span, 0..7);
    }
}

#[test]
fn recur_children_are_original_operands_analyzed_once() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let source =
            "(let [copy (loop [x 0 y 0] (if false (recur (mark 0) (mark 1)) x))] (inspect copy))";
        let mut host = Observe::default();
        portable::prepare_fragment_forms_with_expander(
            read_forms(source).unwrap(),
            0..source.len(),
            &Environment::default(),
            phase,
            &mut host,
        )
        .unwrap();
        assert_eq!(host.entries, vec![0.0, 1.0]);
        let initializer = host.initializer.unwrap();
        let hir::SourceNode::Bindings { body, .. } = initializer.node.as_ref().unwrap().as_ref()
        else {
            panic!("loop source body required")
        };
        let hir::SourceNode::Do { result, .. } = body
            .source
            .as_ref()
            .unwrap()
            .node
            .as_ref()
            .unwrap()
            .as_ref()
        else {
            panic!("analyzed loop do required")
        };
        let hir::SourceNode::If { consequent, .. } = result
            .source
            .as_ref()
            .unwrap()
            .node
            .as_ref()
            .unwrap()
            .as_ref()
        else {
            panic!("source conditional required")
        };
        let hir::SourceNode::Recur(arguments) = consequent
            .source
            .as_ref()
            .unwrap()
            .node
            .as_ref()
            .unwrap()
            .as_ref()
        else {
            panic!("original recur children required")
        };
        assert_eq!(arguments.len(), 2);
        for (argument, number) in arguments.iter().zip([0.0, 1.0]) {
            assert!(
                matches!(argument.source.as_ref().unwrap().form.kind, Kind::Number(value) if value == number)
            );
        }
    }
}

#[test]
fn source_if_retains_each_analyzed_operand_once_in_both_phases() {
    for phase in [Phase::Runtime, Phase::Macro] {
        for alternative in [false, true] {
            let suffix = if alternative { " (mark 2)" } else { "" };
            let source = format!("(let [copy (if (mark 0) (mark 1){suffix})] (inspect copy))");
            let mut host = Observe::default();
            portable::prepare_fragment_forms_with_expander(
                read_forms(&source).unwrap(),
                0..source.len(),
                &Environment::default(),
                phase,
                &mut host,
            )
            .unwrap();
            assert_eq!(
                host.entries,
                if alternative {
                    vec![0.0, 1.0, 2.0]
                } else {
                    vec![0.0, 1.0]
                }
            );
            let initializer = host.initializer.unwrap();
            let hir::SourceNode::If {
                condition,
                consequent,
                alternative: otherwise,
            } = initializer.node.as_ref().unwrap().as_ref()
            else {
                panic!("original conditional source children required")
            };
            for (child, number) in [(condition, 0.0), (consequent, 1.0)] {
                let facts = child.source.as_ref().unwrap();
                assert_eq!(facts.phase, phase);
                assert!(matches!(facts.form.kind, Kind::Number(value) if value == number));
            }
            let facts = otherwise.source.as_ref().unwrap();
            assert_eq!(facts.phase, phase);
            if alternative {
                assert!(matches!(facts.form.kind, Kind::Number(value) if value == 2.0));
            } else {
                assert!(matches!(facts.form.kind, Kind::Nil));
                assert!(
                    matches!(&facts.tags.tag.as_ref().unwrap().kind, Kind::Symbol(name) if name.namespace.is_none() && name.name == "clj-nil")
                );
            }
        }
    }
}

#[test]
fn function_methods_retain_original_declarations_and_once_analyzed_bodies() {
    for phase in [Phase::Runtime, Phase::Macro] {
        for input in [
            "(fn* [x] (mark 0) x)",
            "(fn* [x & rest] (mark 0) rest)",
            "(fn* ^{:doc \"method\"} ([x x] (mark 0) x) ([x] (mark 1) x))",
        ] {
            let source = format!("(let [copy {input}] (inspect copy))");
            let mut host = Observe::default();
            let mut environment = Environment::default();
            for name in ["IndexedSeq", "first", "rest", "next", "seq"] {
                environment.declare_cell(phase, "suss.core", name).unwrap();
            }
            portable::prepare_fragment_forms_with_expander(
                read_forms(&source).unwrap(),
                0..source.len(),
                &environment,
                phase,
                &mut host,
            )
            .unwrap();
            let facts = host.initializer.unwrap();
            let callable = facts.callable.as_ref().unwrap();
            assert_eq!(
                host.entries,
                if callable.methods.len() == 2 {
                    vec![0.0, 1.0]
                } else {
                    vec![0.0]
                }
            );
            for (index, method) in callable.methods.iter().enumerate() {
                assert_eq!(method.parameters.len(), method.declarations.len());
                if callable.methods.len() == 2 {
                    let original = read_forms(&source).unwrap().remove(0);
                    let Kind::List(outer) = original.kind else {
                        unreachable!()
                    };
                    let Kind::Vector(bindings) = &outer[1].kind else {
                        unreachable!()
                    };
                    let Kind::List(parts) = &bindings[1].kind else {
                        unreachable!()
                    };
                    assert_eq!(method.form, parts[index + 1]);
                }
                let Kind::List(parts) = &method.form.kind else {
                    panic!("original method form required")
                };
                assert!(matches!(parts[0].kind, Kind::Vector(_)));
                let body = method
                    .body
                    .source
                    .as_ref()
                    .expect("genuine analyzed do body");
                assert_eq!(body.context, portable::AnalysisContext::Return);
                let hir::SourceNode::Do { statements, result } =
                    body.node.as_ref().unwrap().as_ref()
                else {
                    panic!("source do required")
                };
                assert_eq!(statements.len(), 1);
                let hir::SourceBinding::Local(local) =
                    result.source.as_ref().unwrap().resolved.as_ref().unwrap()
                else {
                    panic!("source local required")
                };
                let declaration = method.declarations.last().unwrap();
                assert!(Arc::ptr_eq(&declaration.identity, &local.identity));
                assert!(Arc::ptr_eq(
                    &declaration.identity,
                    &body.locals[&method.parameters.last().unwrap().name].identity
                ));
                if method.declarations.len() == 2 && !method.variadic {
                    assert!(!Arc::ptr_eq(
                        &method.declarations[0].identity,
                        &declaration.identity
                    ));
                    assert!(Arc::ptr_eq(
                        &declaration.shadow.as_ref().unwrap().identity,
                        &method.declarations[0].identity
                    ));
                }
            }
        }
    }
}

#[test]
fn throw_operand_retains_original_analysis_without_reexpansion() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let source = "(let [copy (throw (mark 42))] (inspect copy))";
        let mut host = Observe::default();
        portable::prepare_fragment_forms_with_expander(
            read_forms(source).unwrap(),
            0..source.len(),
            &Environment::default(),
            phase,
            &mut host,
        )
        .unwrap();
        assert_eq!(host.entries, vec![42.0]);
        let source = host.initializer.unwrap();
        let hir::SourceNode::Throw(exception) =
            source.node.as_ref().expect("throw source node").as_ref()
        else {
            panic!("throw must retain source exception operand")
        };
        let facts = exception.source.as_ref().unwrap();
        assert!(matches!(facts.form.kind, Kind::Number(value) if value == 42.0));
        assert_eq!(facts.context, portable::AnalysisContext::Expression);
    }
}

#[test]
fn try_source_regions_are_analyzed_once_in_pinned_visitation_order() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let source = "(let [copy (try (mark 0) (catch :default error (mark 1)) (finally (mark 2)))] (inspect copy))";
        let mut host = Observe::default();
        portable::prepare_fragment_forms_with_expander(
            read_forms(source).unwrap(),
            0..source.len(),
            &Environment::default(),
            phase,
            &mut host,
        )
        .unwrap();
        assert_eq!(host.entries, vec![2.0, 1.0, 0.0]);
        let facts = host.initializer.unwrap();
        let hir::SourceNode::Try {
            body,
            handler,
            cleanup,
            payload,
        } = facts.node.as_ref().unwrap().as_ref()
        else {
            panic!("genuine try region nodes required")
        };
        assert!(body.source.as_ref().unwrap().is_body);
        assert!(cleanup.as_ref().unwrap().source.as_ref().unwrap().is_body);
        assert!(!handler.source.as_ref().unwrap().is_body);
        assert!(!facts.is_body);
        assert_eq!(
            body.source.as_ref().unwrap().context,
            portable::AnalysisContext::Return
        );
        assert_eq!(
            cleanup.as_ref().unwrap().source.as_ref().unwrap().context,
            portable::AnalysisContext::Statement
        );
        let hir::SourceNode::Bindings {
            is_loop,
            bindings,
            body: catch_body,
        } = handler
            .source
            .as_ref()
            .unwrap()
            .node
            .as_ref()
            .unwrap()
            .as_ref()
        else {
            panic!("actual analyzed catch let required")
        };
        assert!(!is_loop);
        assert!(catch_body.source.as_ref().unwrap().is_body);
        assert_eq!(bindings.len(), 1);
        let hidden = payload.as_ref().unwrap();
        assert_eq!(bindings[0].id, hidden.id);
        assert!(matches!(
            bindings[0].source_role,
            hir::SourceRole::CatchBinding { .. }
        ));
        let initializer = bindings[0]
            .initializer
            .as_ref()
            .unwrap()
            .source
            .as_ref()
            .unwrap();
        let hir::SourceBinding::Local(local) = initializer.resolved.as_ref().unwrap() else {
            panic!("actual payload local required")
        };
        assert!(Arc::ptr_eq(&local.identity, &hidden.identity));
        assert!(Arc::ptr_eq(
            &catch_body.source.as_ref().unwrap().locals["error"].identity,
            &bindings[0].identity
        ));
    }
}

#[test]
fn typed_catch_uses_canonical_type_test_and_bare_try_retains_analyzed_fallback() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let source = "(let [instance? (fn* [x y] false) copy (try 1 (catch Class error error))] (inspect copy))";
        let mut environment = Environment::default();
        environment.declare_cell(phase, "user", "Class").unwrap();
        let mut host = Observe::default();
        portable::prepare_fragment_forms_with_expander(
            read_forms(source).unwrap(),
            0..source.len(),
            &environment,
            phase,
            &mut host,
        )
        .unwrap();
        let facts = host.initializer.unwrap();
        let hir::SourceNode::Try { handler, .. } = facts.node.as_ref().unwrap().as_ref() else {
            panic!("try required")
        };
        let hir::SourceNode::If { condition, .. } = handler
            .source
            .as_ref()
            .unwrap()
            .node
            .as_ref()
            .unwrap()
            .as_ref()
        else {
            panic!("typed catch source if required")
        };
        let Kind::List(test) = &condition.source.as_ref().unwrap().form.kind else {
            panic!("actual type test call required")
        };
        assert!(
            matches!(&test[0].kind, Kind::Symbol(name) if name.namespace.as_deref() == Some("cljs.core") && name.name == "instance?")
        );
        let source = "(let [copy (try 1)] (inspect copy))";
        let mut host = Observe::default();
        portable::prepare_fragment_forms_with_expander(
            read_forms(source).unwrap(),
            0..source.len(),
            &Environment::default(),
            phase,
            &mut host,
        )
        .unwrap();
        let facts = host.initializer.unwrap();
        let hir::SourceNode::Try {
            body,
            handler,
            cleanup,
            payload,
        } = facts.node.as_ref().unwrap().as_ref()
        else {
            panic!("try required")
        };
        assert!(cleanup.is_none() && payload.is_none());
        assert_eq!(
            body.source.as_ref().unwrap().context,
            portable::AnalysisContext::Expression
        );
        let hir::SourceNode::Throw(exception) = handler
            .source
            .as_ref()
            .unwrap()
            .node
            .as_ref()
            .unwrap()
            .as_ref()
        else {
            panic!("analyzed fallback required")
        };
        assert!(matches!(
            exception.source.as_ref().unwrap().form.kind,
            Kind::Nil
        ));
    }
}

#[test]
fn method_recurrence_records_only_accepted_edges_to_its_own_target() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut environment = Environment::default();
        for name in ["IndexedSeq", "first", "rest", "next", "seq"] {
            environment.declare_cell(phase, "suss.core", name).unwrap();
        }
        for (function, expected) in [
            ("(fn [x] x)", vec![None]),
            ("(fn [x] (if x (recur nil) 42))", vec![Some(true)]),
            ("(fn [x] (loop [y x] (if y (recur nil) 42)))", vec![None]),
            ("(fn [x] (fn [y] (if y (recur nil) x)))", vec![None]),
            ("(fn ([x] (recur nil)) ([x y] y))", vec![Some(true), None]),
            ("(fn [x & xs] (recur nil xs))", vec![Some(true)]),
            // Recurrence is an analysis fact even in a runtime-unselected arm.
            ("(fn [x] (if false (recur x) 42))", vec![Some(true)]),
        ] {
            let source = format!("(let [copy {function}] (inspect copy))");
            let mut host = Observe::default();
            portable::prepare_fragment_forms_with_expander(
                read_forms(&source).unwrap(),
                0..source.len(),
                &environment,
                phase,
                &mut host,
            )
            .unwrap();
            let facts = host.initializer.unwrap();
            let callable = facts.callable.as_ref().expect("source methods");
            assert_eq!(
                callable
                    .methods
                    .iter()
                    .map(|method| method.recurs)
                    .collect::<Vec<_>>(),
                expected,
                "{function}"
            );
        }
        for function in ["(fn [x] (do (recur x) x))", "(fn [x] (recur))"] {
            let source = format!("(let [copy {function}] (inspect copy))");
            let mut host = Observe::default();
            assert!(
                portable::prepare_fragment_forms_with_expander(
                    read_forms(&source).unwrap(),
                    0..source.len(),
                    &environment,
                    phase,
                    &mut host,
                )
                .is_err(),
                "invalid recurrence must remain a diagnostic: {function}"
            );
            assert!(host.initializer.is_none());
        }
    }
}

#[test]
fn method_entry_environment_precedes_parameters_and_body_markers_are_explicit() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let source = "(let [x 7 copy (fn* [x] x)] (inspect copy))";
        let mut host = Observe::default();
        portable::prepare_fragment_forms_with_expander(
            read_forms(source).unwrap(),
            0..source.len(),
            &Environment::default(),
            phase,
            &mut host,
        )
        .unwrap();
        let source = host.initializer.unwrap();
        assert!(!source.is_body);
        let method = &source.callable.as_ref().unwrap().methods[0];
        let entry = &method.environment;
        assert_eq!(entry.context, portable::AnalysisContext::Expression);
        assert_eq!(entry.locals["x"].kind, hir::LocalKind::Let);
        assert!(!entry.locals.contains_key("copy"));
        let parameter = &method.declarations[0];
        assert!(!Arc::ptr_eq(
            &entry.locals["x"].identity,
            &parameter.identity
        ));
        assert!(Arc::ptr_eq(
            &parameter.shadow.as_ref().unwrap().identity,
            &entry.locals["x"].identity
        ));
        let body = method.body.source.as_ref().unwrap();
        assert!(body.is_body);
        assert_eq!(body.context, portable::AnalysisContext::Return);
        assert!(Arc::ptr_eq(&body.locals["x"].identity, &parameter.identity));
        let hir::SourceNode::Do { result, .. } = body.node.as_deref().unwrap() else {
            panic!("source do body")
        };
        assert!(!result.source.as_ref().unwrap().is_body);

        for (text, expected) in [
            ("(fn* [x] x)", portable::AnalysisContext::Statement),
            (
                "(fn* ([x] x) ([x y] y))",
                portable::AnalysisContext::Expression,
            ),
        ] {
            let analyzed = hir::analyze_in(
                &read_forms(text).unwrap(),
                0..text.len(),
                &Environment::default(),
                phase,
            )
            .unwrap();
            let hir::Expression::Do(items) = &analyzed.kind else {
                panic!("fragment body")
            };
            let callable = items
                .last()
                .unwrap()
                .source
                .as_ref()
                .unwrap()
                .callable
                .as_ref()
                .unwrap();
            for method in &callable.methods {
                assert_eq!(method.environment.context, expected, "{text}");
                assert!(!method.environment.locals.contains_key("x"));
                assert!(method.body.source.as_ref().unwrap().is_body);
            }
        }
    }
}
