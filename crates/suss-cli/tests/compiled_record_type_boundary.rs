//! Intrinsic record/type boundary regressions; not public defrecord acceptance.
//! These tests are authored pending bootstrap regeneration and execution.
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;

#[path = "support/compilation_fuel.rs"]
mod compilation_fuel;
use portable_decode::{Decoder, Observation};
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};
use suss_compile::portable::{self, hir::SourceFields};
use suss_reader::forms::{Form, Kind};

#[derive(Default)]
struct Fields {
    observed: Vec<(String, usize, bool, SourceFields)>,
}

impl portable::ExpansionHost for Fields {
    fn expand(
        &mut self,
        form: &Form,
        context: portable::ExpansionContext<'_>,
    ) -> Result<Option<Form>, portable::Diagnostic> {
        let Kind::List(items) = &form.kind else {
            return Ok(None);
        };
        if !matches!(items.first().map(|value| &value.kind), Some(Kind::Symbol(name)) if name.namespace.is_none() && name.name == "record-checkpoint")
        {
            return Ok(None);
        }
        let Kind::String(label) = &items[1].kind else {
            panic!("checkpoint label")
        };
        let binding = context.environment.resolve(
            context.phase,
            &suss_reader::Symbol::new("BoundaryRecord"),
            form.span.clone(),
        )?;
        let info = context
            .environment
            .definition_info(binding.global())
            .unwrap();
        assert!(
            !context.locals.contains_key("outer"),
            "type methods cannot capture enclosing locals"
        );
        self.observed.push((
            String::from_utf16(label).unwrap(),
            info.type_fields.unwrap(),
            info.type_is_record,
            context.fields.clone(),
        ));
        Ok(Some(items[2].clone()))
    }
}

fn number(value: f64) -> Observation {
    Observation::Number(value.to_bits())
}

fn observe(session: &mut Session, decoder: &mut Decoder, source: &str) -> Observation {
    decoder.refill(4096);
    let value = session.eval(source).unwrap();
    session.collect().unwrap();
    decoder.decode_session(session, &value).unwrap()
}

#[test]
fn intrinsic_record_keeps_hidden_storage_fields_method_scope_and_body_order() {
    const SOURCE: &str = r#"
      (defprotocol BoundaryProbe
        (boundary-read [this]) (boundary-cache [this value])
        (boundary-overload [this] [this left]))
      (def boundary-body-effects 0)
      (let [outer 999]
        (defrecord* BoundaryRecord [left right] nil
          (do
            (record-checkpoint "declaration" nil)
            (set! boundary-body-effects (+ boundary-body-effects 1))
            (extend-type BoundaryRecord BoundaryProbe
              (boundary-read ([this] (record-checkpoint "method" [left right __meta __extmap __hash])))
              (boundary-cache ([this value] (set! __hash value) __hash))
              (boundary-overload
                ([this] left)
                ([this left] (+ (.-left this) left)))))))
      (def boundary-value (BoundaryRecord. 7 9 nil nil nil))
    "#;
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        let mut fields = Fields::default();
        session.eval_with_macros(SOURCE, &mut fields).unwrap();
        for (_, count, record, _) in &fields.observed {
            assert_eq!(*count, 2);
            assert!(*record);
        }
        let (_, _, _, methods) = fields
            .observed
            .iter()
            .find(|(label, _, _, _)| label == "method")
            .unwrap();
        assert_eq!(methods.len(), 5);
        for (index, name) in ["left", "right", "__meta", "__extmap", "__hash"]
            .into_iter()
            .enumerate()
        {
            assert_eq!(methods[name].index, index);
            assert_eq!(methods[name].mutable, name == "__hash");
        }
        let value = session.eval("boundary-value").unwrap();
        session
            .inspect(&value, |mut store, value| {
                let owner = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let data = owner.field(&mut store, 1)?;
                assert_eq!(
                    data.unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap()
                        .len(&store)?,
                    5
                );
                Ok(())
            })
            .unwrap();
        assert_eq!(
            observe(&mut session, &mut decoder, "boundary-body-effects"),
            number(1.0)
        );
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "[(boundary-overload boundary-value) (boundary-overload boundary-value 5)]"
            ),
            Observation::Vector(vec![number(7.0), number(12.0)])
        );
        assert_eq!(
            observe(&mut session, &mut decoder, "(boundary-read boundary-value)"),
            Observation::Vector(vec![
                number(7.0),
                number(9.0),
                Observation::Nil,
                Observation::Nil,
                Observation::Nil
            ])
        );
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "(boundary-cache boundary-value 17)"
            ),
            number(17.0)
        );
        assert_eq!(
            observe(&mut session, &mut decoder, "(boundary-read boundary-value)"),
            Observation::Vector(vec![
                number(7.0),
                number(9.0),
                Observation::Nil,
                Observation::Nil,
                number(17.0)
            ])
        );
        assert!(matches!(session.eval("(let [outer 99] (defrecord* InvalidCapture [left] nil (extend-type InvalidCapture BoundaryProbe (boundary-read ([this] outer)))))"), Err(suss_cli::portable_session::SessionError::Compile(_))));
        assert_eq!(
            observe(
                &mut session,
                &mut decoder,
                "(boundary-cache boundary-value 23)"
            ),
            number(23.0)
        );
    }
}

#[test]
fn compiled_macro_catalog_reports_declared_field_count_and_intrinsic_record_kind() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
        macros.define("(defmacro intrinsic-type-facts [name] (let [info (get (get (get &env :suss/catalog) :defs) name)] (list 'quote [(get info :type) (get info :num-fields) (get info :record)])))").unwrap();
        let value = session.eval_with_macros(
            "(defrecord* CatalogRecord [left right] nil (def catalog-body-facts (intrinsic-type-facts CatalogRecord))) [(intrinsic-type-facts CatalogRecord) catalog-body-facts]",
            &mut macros,
        ).unwrap();
        session.collect().unwrap();
        let expected = || {
            Observation::Vector(vec![
                Observation::Bool(true),
                number(2.0),
                Observation::Bool(true),
            ])
        };
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Vector(vec![expected(), expected()])
        );
        let value = session
            .eval_with_macros(
                "(deftype* CatalogRecord [value] nil nil) (intrinsic-type-facts CatalogRecord)",
                &mut macros,
            )
            .unwrap();
        assert_eq!(
            decoder.decode_session(&mut session, &value).unwrap(),
            Observation::Vector(vec![
                Observation::Bool(true),
                number(1.0),
                Observation::Bool(false)
            ])
        );
        assert_eq!(
            observe(&mut session, &mut decoder, "(.-value (CatalogRecord. 42))"),
            number(42.0)
        );
    }
}
