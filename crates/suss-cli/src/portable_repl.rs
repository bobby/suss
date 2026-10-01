//! Value display for the compiled native REPL, without evaluating source again.
use crate::portable_session::{Session, SessionError, SessionValue};
use wasmtime::Val;

pub fn display(session: &mut Session, value: &SessionValue) -> Result<String, SessionError> {
    enum Output {
        Text(String),
        Number,
    }
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
            _ => {
                return Err(wasmtime::Error::msg(
                    "Value display does not yet support this runtime object",
                ));
            }
        })
    })?;
    match output {
        Output::Text(text) => Ok(text),
        Output::Number => session.number_text(value),
    }
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
    let mut forms = suss_reader::forms::read_forms(source)
        .and_then(suss_reader::forms::resolve_conditionals)
        .map_err(|error| {
            SessionError::Compile(suss_compile::portable::Diagnostic {
                span: error.span,
                message: error.message,
            })
        })?;
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
                    return macros
                        .define_form_display(forms.into_iter().next().unwrap(), 0..source.len());
                }
            }
        }
    }
    let prepared = runtime
        .compilation_snapshot()
        .prepare(forms, 0..source.len(), macros)?;
    let value = runtime.eval_prepared(prepared)?;
    display(runtime, &value)
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
