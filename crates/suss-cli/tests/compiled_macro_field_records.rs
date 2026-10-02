use std::collections::HashMap;
use suss_cli::portable_session::Session;
use suss_compile::portable::{
    self,
    hir::{Expression, FieldBinding, LocalBinding, Nominal},
};
use suss_reader::forms::{Form, Kind};

#[derive(Default)]
struct Observe {
    calls: Vec<(
        String,
        HashMap<String, LocalBinding>,
        HashMap<String, FieldBinding>,
    )>,
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
        if !matches!(&items[0].kind, Kind::Symbol(s) if s.namespace.is_none() && s.name == "checkpoint")
        {
            return Ok(None);
        }
        let Kind::String(label) = &items[1].kind else {
            panic!("label")
        };
        self.calls.push((
            String::from_utf16(label).unwrap(),
            context.locals.clone(),
            context.fields.clone(),
        ));
        Ok(Some(items[2].clone()))
    }
}
#[test]
fn compiler_macro_field_records_preserve_declarations_access_and_parameter_shadows() {
    let source = r#"(defprotocol Probe (probe [this]))
(defprotocol ProbeArg (probe-arg [this x]))
(deftype Holder [^:mutable x]
  Probe
  (probe [this]
    (checkpoint "field" x)
    (let [f (fn [x] (checkpoint "shadow" x))] (f x))
    (checkpoint "restored" x))
  ProbeArg
  (probe-arg [this x] (checkpoint "direct-shadow" x)))
(def result (probe (Holder. 42)))
(def direct-result (probe-arg (Holder. 7) 42))"#;
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut host = Observe::default();
        session.eval_with_macros(source, &mut host).unwrap();
        let call = |label: &str| {
            host.calls
                .iter()
                .find(|(name, _, _)| name == label)
                .unwrap()
        };
        let (_, locals, fields) = call("field");
        assert!(!locals.contains_key("x"));
        let field = &fields["x"];
        assert_eq!(field.index, 0);
        assert!(field.mutable);
        assert_eq!(&source[field.declaration.span.clone()], "^:mutable x");
        assert_eq!(field.declaration.metadata.len(), 1);
        assert!(matches!(
            field.access.kind,
            Expression::Nominal {
                operation: Nominal::Field(0),
                ..
            }
        ));
        assert_eq!(field.origin.as_ref().unwrap().text(), source);
        let (_, locals, _) = call("shadow");
        let parameter = &locals["x"];
        assert!(parameter.shadow.is_none());
        let shadow = parameter
            .shadow_field
            .as_ref()
            .expect("actual field shadow");
        assert_eq!(shadow.declaration, field.declaration);
        assert_eq!(shadow.index, field.index);
        let (_, locals, fields) = call("restored");
        assert!(!locals.contains_key("x"));
        assert_eq!(fields["x"].declaration, field.declaration);
        let (_, locals, fields) = call("direct-shadow");
        let direct = &locals["x"];
        assert!(matches!(
            direct.kind,
            portable::hir::LocalKind::Argument {
                index: 1,
                rest: false
            }
        ));
        assert_eq!(&source[direct.declaration.span.clone()], "x");
        assert!(direct.shadow.is_none());
        let direct_field = direct
            .shadow_field
            .as_ref()
            .expect("method parameter shadows actual field");
        assert_eq!(direct_field.declaration, field.declaration);
        assert_eq!(direct_field.declaration, fields["x"].declaration);
        assert!(direct_field.mutable);
        assert!(matches!(
            direct_field.access.kind,
            Expression::Nominal {
                operation: Nominal::Field(0),
                ..
            }
        ));
        session.collect().unwrap();
        for name in ["result", "direct-result"] {
            let result = session.eval(name).unwrap();
            session
                .inspect(&result, |mut store, value| {
                    let number = value
                        .unwrap_anyref()
                        .unwrap()
                        .as_struct(&store)?
                        .unwrap()
                        .fields(&mut store)?
                        .next()
                        .unwrap()
                        .unwrap_f64();
                    assert_eq!(number.to_bits(), 42f64.to_bits());
                    Ok(())
                })
                .unwrap();
        }
    }
}
