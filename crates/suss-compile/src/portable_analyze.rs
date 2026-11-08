//! `suss analyze`: a source file's forms or analysis as EDN (#299 step 2).
//!
//! Analysis runs the ordinary compilation pipeline, macros included, with the
//! analyzer recording each fragment of the selected file, and runs no Runtime
//! code. The document shapes are in `docs/runtime/analyzer-edn.md`.
use crate::portable::{
    analysis_record::{self, RecordedFragment},
    analyzer_edn::{self, Projector},
    resolve::Phase,
};
use crate::portable_aot::{SourceInput, prepare_inputs_with_command};
use crate::portable_repl::read_script_forms;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use suss_reader::edn::{
    Value,
    source::{LineIndex, form_to_value, span_tree},
};

/// What `suss analyze` prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emit {
    /// The forms as read, with their source positions alongside.
    Forms,
    /// The analyzer's nodes, in tools.analyzer's shape.
    Hir,
}

fn kw(name: &str) -> Value {
    Value::keyword(name)
}

fn header(kind: &str, path: &Path) -> BTreeMap<Value, Value> {
    BTreeMap::from([
        (kw("suss.analyzer/version"), Value::Long(i64::from(analyzer_edn::VERSION))),
        (kw("suss.analyzer/kind"), kw(kind)),
        (kw("file"), Value::String(path.display().to_string())),
    ])
}

/// The forms of a source text, with their positions.
pub fn forms_document(source: &str, path: &Path) -> Result<Value, String> {
    let forms = read_script_forms(source).map_err(|error| format!("{}: {error}", path.display()))?;
    let lines = LineIndex::new(source);
    let mut document = header("forms", path);
    let values = forms.iter().map(form_to_value).collect::<Result<Vec<_>, _>>();
    let trees = forms.iter().map(|form| span_tree(form, Some(&lines))).collect::<Result<Vec<_>, _>>();
    document.insert(kw("forms"), Value::Vector(values.map_err(|error| error.message)?));
    document.insert(kw("suss/source"), Value::Vector(trees.map_err(|error| error.message)?));
    Ok(Value::Map(document))
}

/// The fragments analyzed from a source text, in order. `namespace`, when
/// given, is the namespace the file must declare.
pub fn analyze_source(
    source: &str,
    path: &Path,
    source_paths: &[PathBuf],
    namespace: Option<&str>,
    compile_fuel: Option<u64>,
) -> Result<Vec<RecordedFragment>, String> {
    let forms = read_script_forms(source).map_err(|error| format!("{}: {error}", path.display()))?;
    if let Some(namespace) = namespace {
        crate::portable::modules::validate_namespace_source(namespace, &forms, Phase::Runtime)
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    let inputs = [SourceInput {
        source: source.to_owned(),
        path: Some(path.to_owned()),
        forms,
        namespace: namespace.map(str::to_owned),
    }];
    let selected = {
        let (source, path) = (source.to_owned(), path.to_owned());
        move |origin: Option<&crate::portable::SourceOrigin>| {
            origin.is_some_and(|origin| origin.text() == source && origin.path() == Some(path.as_path()))
        }
    };
    let (result, fragments) = analysis_record::record(Box::new(selected), || {
        prepare_inputs_with_command(&inputs, source_paths, false, compile_fuel)
    });
    result.map_err(|error| error.to_string())?;
    Ok(fragments)
}

/// The analysis document of a file's fragments: `:asts`, one node per
/// top-level form (and a `:suss/ns` node for a namespace declaration), and
/// `:global-env`, the state of each namespace the file entered.
pub fn hir_document(fragments: &[RecordedFragment], path: &Path) -> Result<Value, String> {
    let mut document = header("hir", path);
    let mut asts = Vec::new();
    let mut namespaces = BTreeMap::new();
    let origin = fragments.first().and_then(|fragment| fragment.origin.as_ref());
    let mut projector = Projector::new(origin);
    for fragment in fragments {
        if let Some(directive) = &fragment.namespace_directive {
            asts.push(analyzer_edn::namespace_node(directive, fragment.phase, origin)?);
        }
        for form in &fragment.forms {
            // The compiler handles a namespace declaration itself and analyzes
            // nil in its place.
            let placeholder = fragment.namespace_directive.as_ref().is_some_and(|directive| {
                form.source.as_ref().is_some_and(|source| {
                    source.form.span == directive.span
                        && matches!(source.form.kind, suss_reader::forms::Kind::Nil)
                })
            });
            if !placeholder {
                asts.push(projector.node(form)?);
            }
        }
        let namespace = fragment.environment.current_namespace(fragment.phase).to_owned();
        if let Some(env) = analyzer_edn::namespace_env(&fragment.environment, fragment.phase, &namespace) {
            namespaces.insert((fragment.phase == Phase::Macro, namespace), env);
        }
    }
    document.insert(kw("asts"), Value::Vector(asts));
    let mut runtime = BTreeMap::new();
    let mut macro_phase = BTreeMap::new();
    for ((is_macro, namespace), env) in namespaces {
        let name = Value::Symbol(suss_reader::Symbol::new(&namespace));
        if is_macro { macro_phase.insert(name, env) } else { runtime.insert(name, env) };
    }
    document.insert(
        kw("global-env"),
        Value::Map(BTreeMap::from([
            (kw("namespaces"), Value::Map(runtime)),
            (kw("macro-namespaces"), Value::Map(macro_phase)),
        ])),
    );
    Ok(Value::Map(document))
}

/// `suss analyze`: the document for a file, or for a namespace found on the
/// source paths.
pub fn analyze(
    file: Option<&Path>,
    namespace: Option<&str>,
    source_paths: &[PathBuf],
    emit: Emit,
    compile_fuel: Option<u64>,
) -> Result<Value, String> {
    let path = match (file, namespace) {
        (Some(_), Some(_)) => return Err("analyze takes a file or a namespace, not both".into()),
        (Some(file), None) => file.to_owned(),
        (None, Some(namespace)) => crate::portable::resolve::locate_source(namespace, source_paths, 0..0)
            .map_err(|error| error.to_string())?,
        (None, None) => return Err("analyze needs a file or a namespace".into()),
    };
    let source = std::fs::read_to_string(&path).map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
    match emit {
        Emit::Forms => forms_document(&source, &path),
        Emit::Hir => {
            let fragments = analyze_source(&source, &path, source_paths, namespace, compile_fuel)?;
            hir_document(&fragments, &path)
        }
    }
}

/// The forms that the nodes of a document emit, as source text.
pub fn emitted_source(document: &Value) -> Result<String, String> {
    let Some(Value::Vector(asts)) = (match document {
        Value::Map(map) => map.get(&kw("asts")),
        _ => None,
    }) else {
        return Err("Document has no asts".into());
    };
    let mut text = String::new();
    for ast in asts {
        let form = analyzer_edn::emit_form(ast)?;
        text.push_str(&suss_reader::edn::source::print_form(&form).map_err(|error| error.message)?);
        text.push('\n');
    }
    Ok(text)
}

/// A document's nodes for comparison (see [`analyzer_edn::comparable`]).
pub fn comparable_asts(document: &Value) -> Value {
    match document {
        Value::Map(map) => map.get(&kw("asts")).map(analyzer_edn::comparable).unwrap_or(Value::Nil),
        _ => Value::Nil,
    }
}

/// The analysis document of the shipped core in `phase`.
pub fn core_document(phase: Phase) -> Result<Value, String> {
    let source = crate::portable::bootstrap::core_source();
    let fragments = crate::portable::bootstrap::analyze_core_source(source, phase).map_err(|error| error.to_string())?;
    hir_document(&fragments, Path::new("suss/core.sus"))
}
