//! Value display for the compiled native REPL, without evaluating source again.
use crate::portable_session::{Session, SessionError, SessionValue};
use wasmtime::Val;

enum Output {
    Text(String),
    Number,
    Data,
}

/// Own reusable canonical data roots for one native session; discard on reset.
#[derive(Default)]
pub struct NativeDisplay {
    bridge: Option<crate::portable_macro_data::FormBridge>,
}
impl NativeDisplay {
    pub fn clear(&mut self) {
        self.bridge = None;
    }
    pub fn display(
        &mut self,
        session: &mut Session,
        value: &SessionValue,
    ) -> Result<String, SessionError> {
        match inspect_display(session, value)? {
            Output::Text(text) => Ok(text),
            Output::Number => session.number_text(value),
            Output::Data => {
                if self.bridge.is_none() {
                    self.bridge = Some(crate::portable_macro_data::FormBridge::new(session)?);
                }
                let form = self
                    .bridge
                    .as_ref()
                    .unwrap()
                    .read_for_display(session, value)
                    .map_err(|error| match error {
                        SessionError::Compile(error) => {
                            SessionError::Host(wasmtime::Error::msg(format!(
                                "Value display does not yet support this runtime object: {}",
                                error.message
                            )))
                        }
                        error => error,
                    })?;
                let mut text = String::new();
                render_data(session, &form, &mut text)?;
                Ok(text)
            }
        }
    }
}
pub fn display(session: &mut Session, value: &SessionValue) -> Result<String, SessionError> {
    NativeDisplay::default().display(session, value)
}
fn inspect_display(session: &mut Session, value: &SessionValue) -> Result<Output, SessionError> {
    let output = session.inspect(value, |mut store, value| {
        let reference = value
            .unwrap_anyref()
            .ok_or_else(|| wasmtime::Error::msg("Null language value"))?;
        if let Some(bits) = reference.as_i31(&store)? {
            return Ok(Output::Text(
                match bits.get_u32() {
                    0 => "nil",
                    2 => "false",
                    4 => "true",
                    6 => "#<undefined>",
                    _ => return Err(wasmtime::Error::msg("Unknown language sentinel")),
                }
                .into(),
            ));
        }
        if let Some(array) = reference.as_array(&store)? {
            if !matches!(array.ty(&store)?.element_type(), wasmtime::StorageType::I16) {
                return Err(wasmtime::Error::msg(
                    "Value display does not yet support reference arrays",
                ));
            }
            let mut units = Vec::new();
            for unit in array.elems(&mut store)? {
                let Val::I32(unit) = unit else {
                    return Err(wasmtime::Error::msg(
                        "Value display does not yet support reference arrays",
                    ));
                };
                units.push(unit as u16);
            }
            let mut text = String::from("\"");
            for unit in char::decode_utf16(units) {
                match unit {
                    Ok('"') => text.push_str("\\\""),
                    Ok('\\') => text.push_str("\\\\"),
                    Ok('\n') => text.push_str("\\n"),
                    Ok('\r') => text.push_str("\\r"),
                    Ok('\t') => text.push_str("\\t"),
                    Ok(c) if c.is_control() => text.push_str(&format!("\\u{:04x}", c as u32)),
                    Ok(c) => text.push(c),
                    Err(error) => text.push_str(&format!("\\u{:04x}", error.unpaired_surrogate())),
                }
            }
            text.push('"');
            return Ok(Output::Text(text));
        }
        let object = reference
            .as_struct(&store)?
            .ok_or_else(|| wasmtime::Error::msg("Unsupported language value display"))?;
        let fields = object.fields(&mut store)?.collect::<Vec<_>>();
        Ok(match fields.as_slice() {
            [Val::F64(bits)] => {
                let number = f64::from_bits(*bits);
                if number.is_nan() {
                    Output::Text("##NaN".into())
                } else if number == f64::INFINITY {
                    Output::Text("##Inf".into())
                } else if number == f64::NEG_INFINITY {
                    Output::Text("##-Inf".into())
                } else {
                    Output::Number
                }
            }
            [_, Val::FuncRef(_), Val::I32(_), Val::I32(_), _] => Output::Text("#<function>".into()),
            [Val::I64(_), _, _, _, _] => Output::Text("#<type>".into()),
            _ => Output::Data,
        })
    })?;
    Ok(output)
}

fn render_data(
    session: &mut Session,
    form: &suss_reader::forms::Form,
    text: &mut String,
) -> Result<(), SessionError> {
    use suss_reader::forms::Kind;
    match &form.kind {
        Kind::Nil => text.push_str("nil"),
        Kind::Bool(value) => text.push_str(if *value { "true" } else { "false" }),
        Kind::Number(value) => {
            if value.is_nan() {
                text.push_str("##NaN");
            } else if *value == f64::INFINITY {
                text.push_str("##Inf");
            } else if *value == f64::NEG_INFINITY {
                text.push_str("##-Inf");
            } else {
                let value =
                    session.data_scalar(&suss_compile::portable::hir::Literal::Number(*value))?;
                text.push_str(&session.number_text(&value)?);
            }
        }
        Kind::String(units) => {
            text.push('"');
            for unit in char::decode_utf16(units.iter().copied()) {
                match unit {
                    Ok('"') => text.push_str("\\\""),
                    Ok('\\') => text.push_str("\\\\"),
                    Ok('\n') => text.push_str("\\n"),
                    Ok('\r') => text.push_str("\\r"),
                    Ok('\t') => text.push_str("\\t"),
                    Ok(c) if c.is_control() => text.push_str(&format!("\\u{:04x}", c as u32)),
                    Ok(c) => text.push(c),
                    Err(error) => text.push_str(&format!("\\u{:04x}", error.unpaired_surrogate())),
                }
            }
            text.push('"');
        }
        Kind::Symbol(symbol) => {
            if let Some(namespace) = &symbol.namespace {
                text.push_str(namespace);
                text.push('/');
            }
            text.push_str(&symbol.name);
        }
        Kind::Keyword(keyword) => {
            text.push(':');
            if let Some(namespace) = &keyword.namespace {
                text.push_str(namespace);
                text.push('/');
            }
            text.push_str(&keyword.name);
        }
        Kind::List(items) | Kind::Vector(items) | Kind::Map(items) | Kind::Set(items) => {
            let (open, close) = match &form.kind {
                Kind::List(_) => ("(", ")"),
                Kind::Vector(_) => ("[", "]"),
                Kind::Map(_) => ("{", "}"),
                Kind::Set(_) => ("#{", "}"),
                _ => unreachable!(),
            };
            text.push_str(open);
            for (index, item) in items.iter().enumerate() {
                if index != 0 {
                    text.push(' ');
                }
                render_data(session, item, text)?;
            }
            text.push_str(close);
        }
        _ => {
            return Err(SessionError::Host(wasmtime::Error::msg(
                "Unsupported canonical value display",
            )));
        }
    }
    if text.len() > 8_388_608 {
        return Err(SessionError::Host(wasmtime::Error::msg(
            "Value display exceeds output bound",
        )));
    }
    Ok(())
}

pub fn error_display(session: &mut Session, error: &SessionError) -> String {
    match error {
        SessionError::Language(value) => match display(session, value) {
            Ok(value) => format!("Uncaught language exception: {value}"),
            Err(_) => error.to_string(),
        },
        _ => error.to_string(),
    }
}

/// Evaluate actual parsed forms once, retaining a separate compiled macro Store.
/// A standalone source definition returns its actual compile-time function value.
pub fn evaluate_compiled(
    runtime: &mut Session,
    macros: &mut crate::portable_macros::CompiledMacros,
    source: &str,
) -> Result<String, SessionError> {
    evaluate_compiled_with_display(runtime, macros, source, &mut NativeDisplay::default())
}
pub fn evaluate_compiled_with_display(
    runtime: &mut Session,
    macros: &mut crate::portable_macros::CompiledMacros,
    source: &str,
    display: &mut NativeDisplay,
) -> Result<String, SessionError> {
    let forms = suss_reader::forms::read_forms(source)
        .and_then(suss_reader::forms::resolve_conditionals)
        .map_err(|error| {
            SessionError::Compile(suss_compile::portable::Diagnostic {
                span: error.span,
                message: error.message,
            })
        })?;
    let origin = suss_compile::portable::SourceOrigin::new(source, None);
    evaluate_forms_compiled(runtime, macros, forms, source.len(), &origin, display, true)
}

/// Execute script forms in textual order in one Runtime and one Macro Store.
/// Parse the complete input before effects, and retain its original source spans.
pub fn evaluate_script_compiled(
    runtime: &mut Session,
    macros: &mut crate::portable_macros::CompiledMacros,
    source: &str,
    path: Option<std::path::PathBuf>,
) -> Result<String, SessionError> {
    let forms = read_script_forms(source)?;
    let origin = suss_compile::portable::SourceOrigin::new(source, path);
    let prepared = prepare_script_compiled(
        runtime.compilation_snapshot(), macros, forms, source.len(), &origin,
    )?;
    // All runtime compilation succeeds before any input/dependency initializer.
    let mut result = "nil".to_owned();
    let mut display = NativeDisplay::default();
    let mut prepared = prepared.into_iter().peekable();
    while let Some(input) = prepared.next() {
        let final_form = prepared.peek().is_none();
        match input {
            PreparedScript::Macro(value) if final_form => result = value,
            PreparedScript::Macro(_) => {}
            PreparedScript::Runtime(input) => {
                let value = runtime.eval_prepared(input)?;
                if final_form {
                    result = display.display(runtime, &value)?;
                }
            }
        }
    }
    Ok(result)
}

pub(crate) fn read_script_forms(
    source: &str,
) -> Result<Vec<suss_reader::forms::Form>, SessionError> {
    suss_reader::forms::read_forms(source)
        .and_then(suss_reader::forms::resolve_conditionals)
        .map_err(|error| {
            SessionError::Compile(suss_compile::portable::Diagnostic {
                span: error.span,
                message: error.message,
            })
        })
}

/// Shared staged source preparation: compiler facts and compiled macro effects,
/// with no Runtime execution or Store requirement.
pub(crate) enum PreparedScript {
    Runtime(suss_compile::portable::modules::PreparedInput),
    Macro(String),
}
pub(crate) fn prepare_script_compiled(
    mut snapshot: crate::portable_session::CompilationSnapshot,
    macros: &mut crate::portable_macros::CompiledMacros,
    forms: Vec<suss_reader::forms::Form>,
    source_len: usize,
    origin: &suss_compile::portable::SourceOrigin,
) -> Result<Vec<PreparedScript>, SessionError> {
    let checkpoint = macros.binding_checkpoint()?;
    let mut staged_macros = Vec::new();
    let preparation = (|| -> Result<Vec<PreparedScript>, SessionError> {
        let mut prepared = Vec::new();
        for mut form in forms {
            let definition = matches!(&form.kind, suss_reader::forms::Kind::List(items)
            if matches!(items.first().map(|head| &head.kind),
                Some(suss_reader::forms::Kind::Symbol(symbol))
                if snapshot.environment.resolves_bootstrap_name(snapshot.phase(), symbol, "defmacro")));
            if definition {
                let suss_reader::forms::Kind::List(items) = &mut form.kind else {
                    unreachable!()
                };
                items[0].kind = suss_reader::forms::Kind::Symbol(suss_reader::Symbol {
                    namespace: Some("suss.core".into()),
                    name: "defmacro".into(),
                });
                let namespace = snapshot.environment.current_namespace(snapshot.phase());
                if let Some(suss_reader::forms::Form {
                    kind: suss_reader::forms::Kind::Symbol(name),
                    ..
                }) = items.get(1)
                {
                    let global = suss_compile::portable::resolve::Environment::default()
                        .declare_cell(
                            suss_compile::portable::resolve::Phase::Macro,
                            namespace,
                            &name.name,
                        )
                        .map_err(SessionError::Compile)?;
                    staged_macros.push((macros.binding_checkpoint()?, global));
                }
                macros.enter_namespace(namespace)?;
                prepared.push(PreparedScript::Macro(macros.define_form_display(
                    form,
                    0..source_len,
                    Some(origin),
                )?));
            } else {
                let input = snapshot.prepare_with_origin(
                    vec![form],
                    0..source_len,
                    macros,
                    Some(origin),
                )?;
                snapshot.environment = input.fragment.environment.clone();
                snapshot
                    .provided
                    .extend(input.modules.iter().map(|module| module.identity.clone()));
                prepared.push(PreparedScript::Runtime(input));
            }
        }
        Ok(prepared)
    })();
    let prepared = match preparation {
        Ok(prepared) => prepared,
        Err(error) => {
            for (before, global) in staged_macros.into_iter().rev() {
                macros.restore_bindings(before, &[global])?;
            }
            macros.restore_bindings(checkpoint, &[])?;
            return Err(error);
        }
    };
    drop(checkpoint);
    Ok(prepared)
}

fn evaluate_forms_compiled(
    runtime: &mut Session,
    macros: &mut crate::portable_macros::CompiledMacros,
    mut forms: Vec<suss_reader::forms::Form>,
    source_len: usize,
    origin: &suss_compile::portable::SourceOrigin,
    display: &mut NativeDisplay,
    display_result: bool,
) -> Result<String, SessionError> {
    if forms.len() == 1 {
        if let suss_reader::forms::Kind::List(items) = &forms[0].kind {
            if let Some(suss_reader::forms::Form {
                kind: suss_reader::forms::Kind::Symbol(symbol),
                ..
            }) = items.first()
            {
                if runtime.resolves_macro_definition(symbol) {
                    // Resolve aliases/refers through the actual Runtime scope;
                    // only the structural bootstrap head is canonicalized.
                    let suss_reader::forms::Kind::List(items) = &mut forms[0].kind else {
                        unreachable!()
                    };
                    items[0].kind = suss_reader::forms::Kind::Symbol(suss_reader::Symbol {
                        namespace: Some("suss.core".into()),
                        name: "defmacro".into(),
                    });
                    macros.enter_namespace(runtime.current_namespace())?;
                    return macros.define_form_display(
                        forms.into_iter().next().unwrap(),
                        0..source_len,
                        Some(origin),
                    );
                }
            }
        }
    }
    let prepared = runtime.compilation_snapshot().prepare_with_origin(
        forms,
        0..source_len,
        macros,
        Some(origin),
    )?;
    let value = runtime.eval_prepared(prepared)?;
    if display_result {
        display.display(runtime, &value)
    } else {
        Ok(String::new())
    }
}
/// Provision both replacement Stores before discarding either previous phase.
pub fn reset_compiled(
    runtime: &mut Session,
    macros: &mut crate::portable_macros::CompiledMacros,
) -> Result<(), SessionError> {
    let replacement_runtime = runtime.replacement()?;
    let replacement_macros = macros.replacement()?;
    *runtime = replacement_runtime;
    *macros = replacement_macros;
    Ok(())
}
