//! #241: macro calls larger than the former 4096-node transport limit, at the
//! default macro allowance. Covers caller &form input, explicit argument input
//! and expanded-result transport, for macros that read &form (directly and
//! through a source macro expanded in their body) and one that does not.
use suss_cli::{
    portable_macro_data::FormBridge, portable_macros::CompiledMacros, portable_session::Session,
};
use suss_reader::forms::{Form, Kind};

const WIDTH: usize = 5000;
const MARKED: usize = 2500;
// Caller execution only: evaluating the quoted 5000-element result with no
// macro involved exhausts 20M and completes at 50M. Macro expansion keeps
// the default allowance, asserted below.
const RESULT_EVALUATION_FUEL: u64 = 100_000_000;

fn keyword_entry<'a>(map: &'a Form, name: &str) -> Option<&'a Form> {
    let Kind::Map(entries) = &map.kind else { return None };
    entries.chunks_exact(2).find_map(|pair| {
        matches!(&pair[0].kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == name)
            .then_some(&pair[1])
    })
}

fn metadata<'a>(form: &'a Form, name: &str) -> Option<&'a Form> {
    form.metadata.iter().find_map(|map| keyword_entry(map, name))
}

fn number(form: &Form) -> f64 {
    let Kind::Number(value) = form.kind else { panic!("number: {form:?}") };
    value
}

fn items(form: &Form) -> &[Form] {
    match &form.kind {
        Kind::Vector(items) | Kind::List(items) => items,
        _ => panic!("sequential form: {form:?}"),
    }
}

/// The complete wide vector, with its marked element and its own metadata.
fn assert_wide(vector: &Form) {
    assert_eq!(number(metadata(vector, "arg-mark").expect("argument metadata")), 23.0);
    let items = items(vector);
    assert_eq!(items.len(), WIDTH);
    for (index, item) in items.iter().enumerate() {
        if index == MARKED {
            assert!(matches!(&item.kind, Kind::Symbol(name) if name.name == "marker"));
            assert_eq!(number(metadata(item, "leaf").expect("leaf metadata")), 91.0);
        } else {
            assert_eq!(item.kind, Kind::Number(index as f64), "index {index}");
        }
    }
}

#[test]
fn large_macro_calls_transport_form_arguments_and_results_at_default_allowance() {
    let wide = (0..WIDTH)
        .map(|index| if index == MARKED { "^{:leaf 91} marker".to_owned() } else { index.to_string() })
        .collect::<Vec<_>>()
        .join(" ");
    let call = |name: &str| format!("^{{:call-mark 17}} ({name} ^{{:arg-mark 23}} [{wide}])");
    for (mut caller, phase) in [
        (Session::new_repl().unwrap(), "Runtime"),
        (Session::new_macro().unwrap(), "Macro"),
    ] {
        caller.set_operation_fuel(RESULT_EVALUATION_FUEL);
        let mut macros = CompiledMacros::new().unwrap();
        let allowance = suss_compile::portable::ExpansionHost::expansion_work_fuel(&macros);
        assert_eq!(allowance, suss_compile::portable::DEFAULT_OPERATION_FUEL);
        macros
            .define(
                "(defmacro form-facts [v]
                   (let [m (meta &form) arg (second &form)]
                     (list 'quote [(get m :call-mark) (get m :line) (get m :column)
                                   (count arg) (get (meta arg) :arg-mark)
                                   (get (meta (nth arg 2500)) :leaf) (identical? arg v)])))",
            )
            .unwrap();
        macros.define("(defmacro form-echo [v] (list 'quote &form))").unwrap();
        macros.define("(defmacro call-meta [] '(meta &form))").unwrap();
        macros
            .define("(defmacro form-indirect [v] (list 'quote [(get (call-meta) :call-mark) (count v)]))")
            .unwrap();
        macros
            .define("(defmacro form-free [v] (list 'quote (with-meta [v (count v)] {:payload 41})))")
            .unwrap();
        let bridge = FormBridge::new(&mut caller).unwrap();
        let mut expand = |caller: &mut Session, name: &str| {
            let source = call(name);
            let value = caller
                .eval_with_macros(&source, &mut macros)
                .unwrap_or_else(|error| panic!("{phase} {name}: {error:?}"));
            caller.collect().unwrap();
            bridge.read(caller, &value, 0..source.len()).unwrap()
        };

        // &form metadata, reader position and children are the caller's.
        let facts = expand(&mut caller, "form-facts");
        let facts = items(&facts);
        assert_eq!(number(&facts[0]), 17.0, "{phase} &form metadata");
        assert_eq!(number(&facts[1]), 1.0, "{phase} &form :line");
        // As in Clojure's MetaReader, the position is that of the leading `^`.
        assert_eq!(number(&facts[2]), 1.0, "{phase} &form :column");
        assert_eq!(number(&facts[3]), WIDTH as f64);
        assert_eq!(number(&facts[4]), 23.0);
        assert_eq!(number(&facts[5]), 91.0);
        // Identity across &form and the explicit argument is unspecified
        // (#276); only record that the comparison itself executed.
        assert!(matches!(facts[6].kind, Kind::Bool(_)));

        // Expanded-result transport of the whole &form, including metadata.
        let echoed = expand(&mut caller, "form-echo");
        assert_eq!(number(metadata(&echoed, "call-mark").expect("call metadata")), 17.0);
        let echoed = items(&echoed);
        assert!(matches!(&echoed[0].kind, Kind::Symbol(name) if name.name == "form-echo"));
        assert_wide(&echoed[1]);

        // &form reached only through a source macro expanded in the body.
        let indirect = expand(&mut caller, "form-indirect");
        let indirect = items(&indirect);
        assert_eq!(number(&indirect[0]), 17.0, "{phase} indirect &form must not be elided");
        assert_eq!(number(&indirect[1]), WIDTH as f64);

        // A macro that cannot read &form still receives its whole argument.
        let free = expand(&mut caller, "form-free");
        assert_eq!(number(metadata(&free, "payload").expect("result metadata")), 41.0);
        let free = items(&free);
        assert_wide(&free[0]);
        assert_eq!(number(&free[1]), WIDTH as f64);

        assert_eq!(suss_compile::portable::ExpansionHost::expansion_work_fuel(&macros), allowance);
    }
}
