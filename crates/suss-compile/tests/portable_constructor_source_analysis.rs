//! Retain constructor syntax and analyzed operands before nominal lowering.
use std::sync::Arc;
use suss_compile::portable::{
    self, hir,
    resolve::{Environment, Phase},
};
use suss_reader::forms::{Form, Kind, read_forms};

#[derive(Default)]
struct Observe {
    marks: Vec<f64>,
    initializer: Option<Arc<hir::Hir>>,
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
                let Kind::Number(value) = items[1].kind else {
                    panic!("number mark required")
                };
                self.marks.push(value);
                Ok(Some(items[1].clone()))
            }
            "inspect" => {
                assert_eq!(items.len(), 2);
                assert!(self.initializer.is_none());
                self.initializer = context.locals["copy"].initializer.clone();
                Ok(Some(read_forms("nil").unwrap().remove(0)))
            }
            _ => Ok(None),
        }
    }
}
fn analyze(phase: Phase, constructor: &str) -> Observe {
    let source =
        format!("(deftype SourceConstructorPair [x y]) (let [copy {constructor}] (inspect copy))");
    let mut host = Observe::default();
    portable::prepare_fragment_forms_with_expander(
        read_forms(&source).unwrap(),
        0..source.len(),
        &Environment::default(),
        phase,
        &mut host,
    )
    .unwrap();
    host
}

#[test]
fn constructor_source_children_share_once_analyzed_callee_and_arguments() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let host = analyze(
            phase,
            "(new (do (mark 0) SourceConstructorPair) (mark 1) (mark 2))",
        );
        assert_eq!(host.marks, [0.0, 1.0, 2.0]);
        let init = host.initializer.unwrap();
        let source = init.source.as_ref().unwrap();
        let hir::SourceNode::Construct { class, arguments } = source.node.as_deref().unwrap()
        else {
            panic!("genuine constructor source record required")
        };
        let hir::Expression::Nominal {
            operation: hir::Nominal::Construct,
            arguments: lowered,
        } = &init.kind
        else {
            panic!("existing nominal lowering must remain intact")
        };
        assert_eq!(arguments.len(), 2);
        assert_eq!(lowered.len(), 3);
        for (source, lowered) in std::iter::once(class.as_ref())
            .chain(arguments.iter())
            .zip(lowered)
        {
            assert!(Arc::ptr_eq(
                source.source.as_ref().unwrap(),
                lowered.source.as_ref().unwrap()
            ));
        }
        // parse-new has a present nil result tag for a non-reference class AST.
        // This checks analysis only: it is not a claim about upstream JS emission
        // of a computed constructor, which failed the exploratory Node probe.
        assert!(matches!(source.tags.tag.as_ref().unwrap().kind, Kind::Nil));
        assert!(source.tags.inferred.is_none());
    }
}

#[test]
fn constructor_shorthand_retains_expanded_new_form_and_real_type_declaration() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let host = analyze(phase, "(SourceConstructorPair. (mark 1) (mark 2))");
        assert_eq!(host.marks, [1.0, 2.0]);
        let init = host.initializer.unwrap();
        let source = init.source.as_ref().unwrap();
        let Kind::List(parts) = &source.form.kind else {
            panic!("expanded source list required")
        };
        assert!(
            matches!(&parts[0].kind, Kind::Symbol(head) if head.name == "new" && head.namespace.is_none())
        );
        assert!(
            matches!(&parts[1].kind, Kind::Symbol(head) if head.name == "SourceConstructorPair")
        );
        let hir::SourceNode::Construct { class, .. } = source.node.as_deref().unwrap() else {
            panic!("constructor source children required")
        };
        let class_source = class.source.as_ref().unwrap();
        let Some(hir::SourceBinding::Global {
            global,
            declaration: Some(info),
        }) = class_source.resolved.as_ref()
        else {
            panic!("captured genuine constructor declaration required")
        };
        assert_eq!(global.namespace(), "user");
        assert_eq!(info.type_fields, Some(2));
        assert!(info.analysis_completed);
        assert!(
            matches!(&class_source.tags.tag.as_ref().unwrap().kind, Kind::Symbol(tag) if tag.name == "function")
        );
        assert!(
            matches!(&source.tags.tag.as_ref().unwrap().kind, Kind::Symbol(tag) if tag.to_string() == "user/SourceConstructorPair")
        );
    }
}

#[test]
fn invocation_class_has_no_reference_result_tag() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let source = "(deftype SourceConstructorPair [x y]) (def make-constructor (fn [] SourceConstructorPair)) (let [copy (new (make-constructor) (mark 1) (mark 2))] (inspect copy))";
        let mut host = Observe::default();
        portable::prepare_fragment_forms_with_expander(
            read_forms(source).unwrap(),
            0..source.len(),
            &Environment::default(),
            phase,
            &mut host,
        )
        .unwrap();
        assert_eq!(host.marks, [1.0, 2.0]);
        let init = host.initializer.unwrap();
        let source = init.source.as_ref().unwrap();
        assert!(matches!(source.tags.tag.as_ref().unwrap().kind, Kind::Nil));
        assert!(source.tags.inferred.is_none());
        let hir::SourceNode::Construct { class, .. } = source.node.as_deref().unwrap() else {
            panic!("constructor source record required")
        };
        assert!(matches!(class.kind, hir::Expression::Call { .. }));
    }
}

#[test]
fn current_type_metadata_replaces_an_older_declared_definition() {
    for phase in [Phase::Runtime, Phase::Macro] {
        for declared in ["", ":declared true"] {
            let source = format!(
                "(def ^:private SourceConstructorPair 0) (deftype ^{{{declared} :tag false}} SourceConstructorPair [x y]) (let [copy (new SourceConstructorPair 1 2)] (inspect copy))"
            );
            let mut host = Observe::default();
            portable::prepare_fragment_forms_with_expander(
                read_forms(&source).unwrap(),
                0..source.len(),
                &Environment::default(),
                phase,
                &mut host,
            )
            .unwrap();
            let init = host.initializer.unwrap();
            let hir::SourceNode::Construct { class, .. } =
                init.source.as_ref().unwrap().node.as_deref().unwrap()
            else {
                panic!("constructor record required")
            };
            let class = class.source.as_ref().unwrap();
            assert!(matches!(
                class.tags.tag.as_ref().unwrap().kind,
                Kind::Bool(false)
            ));
            let Some(hir::SourceBinding::Global {
                declaration: Some(info),
                ..
            }) = class.resolved.as_ref()
            else {
                panic!("current type declaration required")
            };
            assert_eq!(info.type_fields, Some(2));
            let metadata = hir::reader_metadata_pairs(&info.declaration).unwrap();
            assert!(metadata.chunks_exact(2).any(|pair| matches!(&pair[0].kind,
            Kind::Keyword(key) if key.namespace.is_none() && key.name == "private")
                && matches!(pair[1].kind, Kind::Bool(true))));
            let Kind::List(form) = &info.definition_form.kind else {
                panic!("definition form required")
            };
            assert!(matches!(&form[0].kind, Kind::Symbol(head) if head.name == "deftype"));
        }
    }
}

#[test]
fn nonprimitive_js_hints_are_source_tag_data_not_fabricated_constructor_names() {
    for phase in [Phase::Runtime, Phase::Macro] {
        for hint in ["js", "js/Foreign", "js/Number"] {
            let source = format!(
                "(deftype ^{hint} SourceConstructorPair [x y]) (let [copy (new SourceConstructorPair 1 2)] (inspect copy))"
            );
            let mut host = Observe::default();
            portable::prepare_fragment_forms_with_expander(
                read_forms(&source).unwrap(),
                0..source.len(),
                &Environment::default(),
                phase,
                &mut host,
            )
            .unwrap();
            let init = host.initializer.unwrap();
            let tag = init.source.as_ref().unwrap().tags.tag.as_ref().unwrap();
            let Kind::Symbol(tag) = &tag.kind else {
                panic!("source result tag required")
            };
            assert_eq!(
                tag.to_string(),
                if hint == "js/Number" {
                    "user/SourceConstructorPair"
                } else {
                    "js"
                }
            );
        }
    }
}
