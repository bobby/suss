//! Value display for the compiled native REPL, without evaluating source again.
use crate::portable_session::{Session, SessionError, SessionValue};
use wasmtime::Val;

enum Output {
    Literal(&'static str),
    Text(String),
    Number,
    Printed,
}

/// Own the session's `pr-str` for one native session; discard on reset.
#[derive(Default)]
pub struct NativeDisplay {
    printer: Option<SessionValue>,
}
impl NativeDisplay {
    pub fn clear(&mut self) {
        self.printer = None;
    }
    /// Display a value as the guest's `pr-str` prints it (#288). Futures and
    /// streams keep their nominal text, so printing never awaits one; a
    /// session without core displays only strings, numbers and literals.
    pub fn display(
        &mut self,
        session: &mut Session,
        value: &SessionValue,
    ) -> Result<String, SessionError> {
        if let Some(leaf) = crate::portable_macro_data::display_leaf(session, value)? {
            return Ok(leaf.text().into());
        }
        let host = match inspect_display(session, value)? {
            Output::Literal(text) => return Ok(text.into()),
            host => host,
        };
        if self.printer.is_none() {
            match session.eval("suss.core/pr-str") {
                Ok(printer) => self.printer = Some(printer),
                // Without core, the host formats strings and numbers alone.
                Err(error) => {
                    return match host {
                        Output::Text(text) => Ok(text),
                        Output::Number => session.number_text(value),
                        Output::Literal(_) | Output::Printed => Err(error),
                    };
                }
            }
        }
        let printer = self.printer.as_ref().unwrap();
        let text = session.invoke(printer, &[value])?;
        string_text(session, &text)?
            .ok_or_else(|| SessionError::Host(wasmtime::Error::msg("pr-str must return a string")))
    }
}
pub fn display(session: &mut Session, value: &SessionValue) -> Result<String, SessionError> {
    NativeDisplay::default().display(session, value)
}
/// A string's text, with lone surrogates escaped, or None for another value.
fn string_text(
    session: &mut Session,
    value: &SessionValue,
) -> Result<Option<String>, SessionError> {
    session.inspect(value, |mut store, value| {
        let Some(array) = value
            .unwrap_anyref()
            .and_then(|reference| reference.as_array(&store).ok().flatten())
        else {
            return Ok(None);
        };
        if !matches!(array.ty(&store)?.element_type(), wasmtime::StorageType::I16) {
            return Ok(None);
        }
        if array.len(&store)? > 8_388_608 {
            return Err(wasmtime::Error::msg("Value display exceeds output bound"));
        }
        let units = array
            .elems(&mut store)?
            .map(|unit| unit.unwrap_i32() as u16)
            .collect::<Vec<_>>();
        let mut text = String::new();
        for unit in char::decode_utf16(units) {
            match unit {
                Ok(c) => text.push(c),
                Err(error) => text.push_str(&format!("\\u{:04x}", error.unpaired_surrogate())),
            }
        }
        Ok(Some(text))
    })
}
fn inspect_display(session: &mut Session, value: &SessionValue) -> Result<Output, SessionError> {
    let output = session.inspect(value, |mut store, value| {
        if let Some(tag) = crate::portable_session::sentinel_tag(&mut store, &value)? {
            return Ok(Output::Literal(match tag as i32 {
                crate::runtime_abi::NIL => "nil",
                crate::runtime_abi::FALSE => "false",
                crate::runtime_abi::TRUE => "true",
                crate::runtime_abi::UNDEFINED => "#<undefined>",
                _ => return Err(wasmtime::Error::msg("Unknown language sentinel")),
            }));
        }
        if let Some(crate::portable_session::RuntimeNumber::Long(_)) =
            crate::portable_session::runtime_number(&mut store, &value)?
        {
            return Ok(Output::Number);
        }
        let reference = value
            .unwrap_anyref()
            .expect("a non-sentinel value has a reference");
        if let Some(array) = reference.as_array(&store)? {
            if !matches!(array.ty(&store)?.element_type(), wasmtime::StorageType::I16) {
                return Ok(Output::Printed);
            }
            let mut units = Vec::new();
            for unit in array.elems(&mut store)? {
                let Val::I32(unit) = unit else {
                    return Ok(Output::Printed);
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
            _ => Output::Printed,
        })
    })?;
    Ok(output)
}

pub fn error_display(session: &mut Session, error: &SessionError) -> String {
    match error {
        SessionError::Language(value) => match exception_text(session, value) {
            Ok(value) => format!("Uncaught language exception: {value}"),
            Err(_) => error.to_string(),
        },
        _ => error.to_string(),
    }
}
/// A thrown error's message, as Clojure's REPL reports one, or else the thrown
/// value as `pr-str` prints it (#288).
fn exception_text(session: &mut Session, value: &SessionValue) -> Result<String, SessionError> {
    if let Ok(ex_message) = session.eval("suss.core/ex-message") {
        let message = session.invoke(&ex_message, &[value])?;
        if let Some(text) = string_text(session, &message)? {
            return Ok(text);
        }
    }
    display(session, value)
}

/// REPL error text: an exhausted per-input budget names itself and the flag
/// that raises it instead of a Wasm backtrace (#289). Runtime traps spend the
/// evaluation budget; macro definitions retag theirs as diagnostics
/// ([`macro_definition_error`]). Only diagnostics that wrap the per-input
/// compilation budget qualify, never the compiler's fixed internal counters.
pub fn repl_error_display(
    runtime: &mut Session,
    macros: &crate::portable_macros::CompiledMacros,
    error: &SessionError,
) -> String {
    let evaluation = runtime.options().fuel_per_operation;
    let compilation = crate::portable::ExpansionHost::expansion_work_fuel(macros);
    let exhausted = |budget: &str, limit: u64, flag: &str| {
        format!("{budget} fuel exhausted (limit {limit} per input); raise it with {flag}, or omit it for no limit")
    };
    match error {
        SessionError::Trap(trap)
            if evaluation != u64::MAX
                && trap.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel) =>
        {
            exhausted("Evaluation", evaluation, "--fuel")
        }
        SessionError::Compile(diagnostic)
            if compilation != u64::MAX && compilation_budget_spent(&diagnostic.message) =>
        {
            exhausted("Compilation", compilation, "--compile-fuel")
        }
        _ => error_display(runtime, error),
    }
}

fn compilation_budget_spent(message: &str) -> bool {
    !message.contains("Interrupted")
        && (message.contains("all fuel consumed by WebAssembly")
            || message.contains("budget exhausted")
            || message.contains("Macro expansion native work fuel exhausted")
            || message.contains("IR verification fuel exhausted"))
}

/// A macro definition runs in the macro session: its fuel trap spent the
/// compilation budget, so report it as a compilation diagnostic.
fn macro_definition_error(error: SessionError, span: std::ops::Range<usize>) -> SessionError {
    match error {
        SessionError::Trap(trap)
            if trap.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel) =>
        {
            SessionError::Compile(crate::portable::Diagnostic {
                span,
                message: format!("Macro definition budget exhausted: {trap:#}"),
            })
        }
        error => error,
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
    let value = evaluate_compiled_value(runtime, macros, source)?;
    display_compiled_value(runtime, macros, &value, display)
}

/// Retain the result in the Store that owns it until explicit display.
pub enum CompiledValue {
    Runtime(SessionValue),
    Macro(SessionValue),
    /// A definition's var, displayed as `#'ns/name` without its value (#288).
    Var(String),
}
fn var_text(namespace: &str, name: &str) -> String {
    format!("#'{namespace}/{name}")
}
/// The var an input's value stands for, unless a conditional `def` didn't run.
fn result_var(
    session: &mut Session,
    definition: Option<crate::portable::hir::ResultDefinition>,
    value: &SessionValue,
) -> Result<Option<String>, SessionError> {
    let Some(definition) = definition else {
        return Ok(None);
    };
    if definition.conditional && matches!(inspect_display(session, value)?, Output::Literal("nil"))
    {
        return Ok(None);
    }
    Ok(Some(var_text(
        definition.global.namespace(),
        definition.global.name(),
    )))
}
pub fn display_compiled_value(
    runtime: &mut Session,
    macros: &mut crate::portable_macros::CompiledMacros,
    value: &CompiledValue,
    display: &mut NativeDisplay,
) -> Result<String, SessionError> {
    match value {
        CompiledValue::Runtime(value) => display.display(runtime, value),
        CompiledValue::Macro(value) => macros.display_value(value),
        CompiledValue::Var(text) => Ok(text.clone()),
    }
}
pub fn evaluate_compiled_value(
    runtime: &mut Session,
    macros: &mut crate::portable_macros::CompiledMacros,
    source: &str,
) -> Result<CompiledValue, SessionError> {
    let forms = read_script_forms(source)?;
    let origin = crate::portable::SourceOrigin::new(source, None);
    evaluate_forms_compiled(runtime, macros, forms, source.len(), &origin)
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
    let origin = crate::portable::SourceOrigin::new(source, path);
    let prepared = prepare_script_compiled(
        runtime.compilation_snapshot(),
        macros,
        forms,
        source.len(),
        &origin,
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
                let definition = input.fragment.definition.clone();
                let value = runtime.eval_prepared(input)?;
                if final_form {
                    result = match result_var(runtime, definition, &value)? {
                        Some(var) => var,
                        None => display.display(runtime, &value)?,
                    };
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
            SessionError::Compile(crate::portable::Diagnostic {
                span: error.span,
                message: error.message,
            })
        })
}

/// Shared staged source preparation: compiler facts and compiled macro effects,
/// with no Runtime execution or Store requirement.
pub(crate) enum PreparedScript {
    Runtime(crate::portable::modules::PreparedInput),
    Macro(String),
}
pub(crate) fn prepare_script_compiled(
    snapshot: crate::portable_session::CompilationSnapshot,
    macros: &mut crate::portable_macros::CompiledMacros,
    forms: Vec<suss_reader::forms::Form>,
    source_len: usize,
    origin: &crate::portable::SourceOrigin,
) -> Result<Vec<PreparedScript>, SessionError> {
    prepare_script_compiled_batch(snapshot, macros, forms, source_len, origin)
        .map(|(prepared, _)| prepared)
}
/// Return the completed immutable input snapshot for multi-source AOT batches.
/// The ordinary script entrypoint retains its existing result and transaction.
pub(crate) fn prepare_script_compiled_batch(
    mut snapshot: crate::portable_session::CompilationSnapshot,
    macros: &mut crate::portable_macros::CompiledMacros,
    forms: Vec<suss_reader::forms::Form>,
    source_len: usize,
    origin: &crate::portable::SourceOrigin,
) -> Result<
    (
        Vec<PreparedScript>,
        crate::portable_session::CompilationSnapshot,
    ),
    SessionError,
> {
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
                    let global = crate::portable::resolve::Environment::default()
                        .declare_cell(
                            crate::portable::resolve::Phase::Macro,
                            namespace,
                            &name.name,
                        )
                        .map_err(SessionError::Compile)?;
                    staged_macros.push((macros.binding_checkpoint()?, global));
                }
                // Auto-resolved keywords use the runtime namespace's aliases,
                // which the macro session does not see (#30).
                crate::portable::source::resolve_auto_keywords(
                    std::slice::from_mut(&mut form),
                    &snapshot.environment,
                    snapshot.phase(),
                )
                .map_err(SessionError::Compile)?;
                macros.enter_namespace(namespace)?;
                let var = match defined_name(&form) {
                    Some(name) => var_text(namespace, name),
                    None => String::new(),
                };
                macros
                    .define_form_with_origin(form, 0..source_len, Some(origin))
                    .map_err(|error| macro_definition_error(error, 0..source_len))?;
                prepared.push(PreparedScript::Macro(var));
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
    Ok((prepared, snapshot))
}

fn evaluate_forms_compiled(
    runtime: &mut Session,
    macros: &mut crate::portable_macros::CompiledMacros,
    mut forms: Vec<suss_reader::forms::Form>,
    source_len: usize,
    origin: &crate::portable::SourceOrigin,
) -> Result<CompiledValue, SessionError> {
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
                    let form = forms.into_iter().next().unwrap();
                    let var =
                        defined_name(&form).map(|name| var_text(runtime.current_namespace(), name));
                    let value = macros
                        .define_form_with_origin(form, 0..source_len, Some(origin))
                        .map_err(|error| macro_definition_error(error, 0..source_len))?;
                    return Ok(match var {
                        Some(var) => CompiledValue::Var(var),
                        None => CompiledValue::Macro(value),
                    });
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
    let definition = prepared.fragment.definition.clone();
    let value = runtime.eval_prepared(prepared)?;
    Ok(match result_var(runtime, definition, &value)? {
        Some(var) => CompiledValue::Var(var),
        None => CompiledValue::Runtime(value),
    })
}
/// The name a `defmacro` form defines.
fn defined_name(form: &suss_reader::forms::Form) -> Option<&str> {
    match &form.kind {
        suss_reader::forms::Kind::List(items) => match items.get(1).map(|name| &name.kind) {
            Some(suss_reader::forms::Kind::Symbol(name)) => Some(&name.name),
            _ => None,
        },
        _ => None,
    }
}
/// Provision both replacement Stores before discarding either previous phase.
pub fn reset_compiled(
    runtime: &mut Session,
    macros: &mut crate::portable_macros::CompiledMacros,
) -> Result<(), SessionError> {
    let replacement_runtime = runtime.replacement()?;
    let replacement_macros = macros.replacement()?;
    runtime.retire_for_reset()?;
    macros.retire_for_reset()?;
    *runtime = replacement_runtime;
    *macros = replacement_macros;
    Ok(())
}
