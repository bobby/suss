//! An environment's own state as EDN, for core's persisted analysis (#299).
//! The values it refers to are written by `core_analysis`.
use super::{Binding, ControlForm, Environment, Global, NominalForm, Phase, ProtocolMethod, Scope};
use crate::portable::core_analysis::{
    Decoder, Encoder, R, decode_arithmetic, decode_array, decode_bitwise, decode_comparison,
    decode_phase, encode_arithmetic, encode_array, encode_bitwise, encode_comparison, encode_phase,
    fixed, index, items, number, unit_enum,
};
use std::collections::{BTreeMap, BTreeSet};
use suss_reader::edn::Value;

unit_enum!(encode_nominal_form, decode_nominal_form, NominalForm {
    Deftype, DeftypeInternal, DefrecordInternal, Defprotocol, ExtendType, Instance, Satisfies,
    Implements, Reify,
});
unit_enum!(encode_control, decode_control, ControlForm {
    When, WhenNot, IfNot, IfLet, And, Or, Cond, Case, Declare, CachingHash, ThreadFirst, AsThread,
    Zero, Positive, Negative, Increment, Decrement, UncheckedGet, UncheckedSet,
});

impl Global {
    pub(crate) fn from_parts(phase: Phase, namespace: &str, name: &str) -> Self {
        Self { phase, namespace: namespace.into(), name: name.into() }
    }
}

fn phase_names(encoder: &mut Encoder<'_>, set: &BTreeSet<(Phase, String)>) -> Value {
    Value::Vector(set.iter().flat_map(|(phase, name)| [encode_phase(*phase), encoder.string(name)]).collect())
}
fn decode_phase_names(decoder: &Decoder, value: &Value) -> R<BTreeSet<(Phase, String)>> {
    items(value)?
        .chunks(2)
        .map(|pair| match pair {
            [phase, name] => Ok((decode_phase(phase)?, decoder.string(name)?)),
            _ => Err("Odd namespace set".to_owned()),
        })
        .collect()
}
fn globals(encoder: &mut Encoder<'_>, set: &BTreeSet<Global>) -> Value {
    Value::Vector(set.iter().map(|g| encoder.global(g)).collect())
}
fn decode_globals(decoder: &Decoder, value: &Value) -> R<BTreeSet<Global>> {
    items(value)?.iter().map(|g| decoder.global(g)).collect()
}
fn strings(encoder: &mut Encoder<'_>, set: &BTreeSet<String>) -> Value {
    Value::Vector(set.iter().map(|s| encoder.string(s)).collect())
}
fn decode_strings(decoder: &Decoder, value: &Value) -> R<BTreeSet<String>> {
    items(value)?.iter().map(|s| decoder.string(s)).collect()
}
fn string_map(encoder: &mut Encoder<'_>, map: &BTreeMap<String, String>) -> Value {
    Value::Vector(map.iter().flat_map(|(k, v)| [encoder.string(k), encoder.string(v)]).collect())
}
fn decode_string_map(decoder: &Decoder, value: &Value) -> R<BTreeMap<String, String>> {
    items(value)?
        .chunks(2)
        .map(|pair| match pair {
            [k, v] => Ok((decoder.string(k)?, decoder.string(v)?)),
            _ => Err("Odd map".to_owned()),
        })
        .collect()
}

impl Environment {
    /// This environment's state, after its definitions have been written.
    pub(crate) fn encode(&self, encoder: &mut Encoder<'_>) -> R<Value> {
        let Self {
            reader_state, namespaces, bindings, definitions, source_generation, scopes, current,
            materialized_bootstrap, macro_exports, macro_namespaces, protocols, core_protocols,
        } = self;
        let mut definition_rows = Vec::new();
        for (global, info) in definitions {
            definition_rows.push(encoder.global(global));
            definition_rows.push(encoder.definition(info)?);
        }
        let mut binding_rows = Vec::new();
        for (global, binding) in bindings {
            let tagged = |tag: usize, payload: Value| Value::Vector(vec![number(tag), payload]);
            let row = match binding {
                Binding::BootstrapBitwise { global: g, operation } => {
                    Value::Vector(vec![number(0), encoder.global(g), encode_bitwise(*operation)])
                }
                Binding::BootstrapControl { global: g, operation } => {
                    Value::Vector(vec![number(1), encoder.global(g), encode_control(*operation)])
                }
                Binding::BootstrapComparison { global: g, operation } => {
                    Value::Vector(vec![number(2), encoder.global(g), encode_comparison(*operation)])
                }
                Binding::BootstrapArray { global: g, operation } => {
                    Value::Vector(vec![number(3), encoder.global(g), encode_array(*operation)])
                }
                Binding::Core { global: g, export } => {
                    Value::Vector(vec![number(4), encoder.global(g), encoder.string(export)])
                }
                Binding::Cell(g) => tagged(5, encoder.global(g)),
                Binding::InternalCell(g) => tagged(6, encoder.global(g)),
                Binding::ReaderCell(g) => tagged(7, encoder.global(g)),
                Binding::Arithmetic { global: g, operator } => {
                    Value::Vector(vec![number(8), encoder.global(g), encode_arithmetic(*operator)])
                }
                Binding::BootstrapLet(g) => tagged(9, encoder.global(g)),
                Binding::BootstrapLoop(g) => tagged(10, encoder.global(g)),
                Binding::BootstrapFn(g) => tagged(11, encoder.global(g)),
                Binding::BootstrapDefonce(g) => tagged(12, encoder.global(g)),
                Binding::BootstrapBinding(g) => tagged(13, encoder.global(g)),
                Binding::Nominal { global: g, form } => {
                    Value::Vector(vec![number(14), encoder.global(g), encode_nominal_form(*form)])
                }
            };
            binding_rows.push(encoder.global(global));
            binding_rows.push(row);
        }
        let mut scope_rows = Vec::new();
        for ((phase, name), scope) in scopes {
            let Scope {
                namespace, aliases, refers, used_refers, renamed_refers, excluded_core, macro_aliases,
                macro_refers, used_macro_refers, renamed_macro_refers,
            } = scope;
            scope_rows.push(Value::Vector(vec![
                encode_phase(*phase),
                encoder.string(name),
                encoder.string(namespace),
                string_map(encoder, aliases),
                Value::Vector(refers.iter().flat_map(|(k, g)| [encoder.string(k), encoder.global(g)]).collect()),
                strings(encoder, used_refers),
                strings(encoder, renamed_refers),
                strings(encoder, excluded_core),
                string_map(encoder, macro_aliases),
                Value::Vector(
                    macro_refers
                        .iter()
                        .flat_map(|(k, (a, b))| [encoder.string(k), encoder.string(a), encoder.string(b)])
                        .collect(),
                ),
                strings(encoder, used_macro_refers),
                strings(encoder, renamed_macro_refers),
            ]));
        }
        let mut protocol_rows = Vec::new();
        for (global, methods) in protocols {
            let methods = methods
                .iter()
                .map(|method| {
                    let signatures =
                        method.signatures.iter().flat_map(|(arity, key)| [number(*arity), number(*key)]).collect();
                    Value::Vector(vec![encoder.string(&method.name), Value::Vector(signatures)])
                })
                .collect();
            protocol_rows.push(encoder.global(global));
            protocol_rows.push(Value::Vector(methods));
        }
        Ok(Value::Vector(vec![
            number(usize::try_from(reader_state.next_gensym()).map_err(|e| e.to_string())?),
            phase_names(encoder, namespaces),
            Value::Vector(binding_rows),
            Value::Vector(definition_rows),
            number(usize::try_from(*source_generation).map_err(|e| e.to_string())?),
            Value::Vector(scope_rows),
            Value::Vector(current.iter().flat_map(|(phase, name)| [encode_phase(*phase), encoder.string(name)]).collect()),
            globals(encoder, materialized_bootstrap),
            Value::Vector(
                macro_exports
                    .iter()
                    .flat_map(|(phase, a, b)| [encode_phase(*phase), encoder.string(a), encoder.string(b)])
                    .collect(),
            ),
            phase_names(encoder, macro_namespaces),
            Value::Vector(protocol_rows),
            globals(encoder, core_protocols),
        ]))
    }

    /// The environment that [`Environment::encode`] wrote.
    pub(crate) fn decode(decoder: &Decoder, value: &Value) -> R<Self> {
        let [next_gensym, namespaces, bindings, definitions, source_generation, scopes, current,
            materialized_bootstrap, macro_exports, macro_namespaces, protocols, core_protocols] = fixed(value)?;
        // Core bindings name a static export, so they come from the default
        // environment, which defines them all.
        let defaults = Environment::default();
        let mut binding_map = BTreeMap::new();
        for pair in items(bindings)?.chunks(2) {
            let [global, row] = pair else { return Err("Odd bindings".into()) };
            let global = decoder.global(global)?;
            let row = items(row)?;
            let (tag, payload) = row.split_first().ok_or("Empty binding")?;
            let binding = match (index(tag)?, payload) {
                (0, [g, op]) => Binding::BootstrapBitwise { global: decoder.global(g)?, operation: decode_bitwise(op)? },
                (1, [g, op]) => Binding::BootstrapControl { global: decoder.global(g)?, operation: decode_control(op)? },
                (2, [g, op]) => Binding::BootstrapComparison { global: decoder.global(g)?, operation: decode_comparison(op)? },
                (3, [g, op]) => Binding::BootstrapArray { global: decoder.global(g)?, operation: decode_array(op)? },
                (4, [g, export]) => {
                    let (g, export) = (decoder.global(g)?, decoder.string(export)?);
                    match defaults.bindings.get(&global) {
                        Some(binding @ Binding::Core { global: d, export: e }) if *d == g && *e == export => binding.clone(),
                        _ => return Err(format!("No core export {export}")),
                    }
                }
                (5, [g]) => Binding::Cell(decoder.global(g)?),
                (6, [g]) => Binding::InternalCell(decoder.global(g)?),
                (7, [g]) => Binding::ReaderCell(decoder.global(g)?),
                (8, [g, op]) => Binding::Arithmetic { global: decoder.global(g)?, operator: decode_arithmetic(op)? },
                (9, [g]) => Binding::BootstrapLet(decoder.global(g)?),
                (10, [g]) => Binding::BootstrapLoop(decoder.global(g)?),
                (11, [g]) => Binding::BootstrapFn(decoder.global(g)?),
                (12, [g]) => Binding::BootstrapDefonce(decoder.global(g)?),
                (13, [g]) => Binding::BootstrapBinding(decoder.global(g)?),
                (14, [g, form]) => Binding::Nominal { global: decoder.global(g)?, form: decode_nominal_form(form)? },
                _ => return Err("Bad binding".into()),
            };
            binding_map.insert(global, binding);
        }
        let mut definition_map = BTreeMap::new();
        for pair in items(definitions)?.chunks(2) {
            let [global, info] = pair else { return Err("Odd definitions".into()) };
            definition_map.insert(decoder.global(global)?, decoder.definition(info)?);
        }
        let mut scope_map = BTreeMap::new();
        for row in items(scopes)? {
            let [phase, name, namespace, aliases, refers, used_refers, renamed_refers, excluded_core,
                macro_aliases, macro_refers, used_macro_refers, renamed_macro_refers] = fixed(row)?;
            let scope = Scope {
                namespace: decoder.string(namespace)?,
                aliases: decode_string_map(decoder, aliases)?,
                refers: items(refers)?
                    .chunks(2)
                    .map(|pair| match pair {
                        [k, g] => Ok((decoder.string(k)?, decoder.global(g)?)),
                        _ => Err("Odd refers".to_owned()),
                    })
                    .collect::<R<_>>()?,
                used_refers: decode_strings(decoder, used_refers)?,
                renamed_refers: decode_strings(decoder, renamed_refers)?,
                excluded_core: decode_strings(decoder, excluded_core)?,
                macro_aliases: decode_string_map(decoder, macro_aliases)?,
                macro_refers: items(macro_refers)?
                    .chunks(3)
                    .map(|triple| match triple {
                        [k, a, b] => Ok((decoder.string(k)?, (decoder.string(a)?, decoder.string(b)?))),
                        _ => Err("Odd macro refers".to_owned()),
                    })
                    .collect::<R<_>>()?,
                used_macro_refers: decode_strings(decoder, used_macro_refers)?,
                renamed_macro_refers: decode_strings(decoder, renamed_macro_refers)?,
            };
            scope_map.insert((decode_phase(phase)?, decoder.string(name)?), scope);
        }
        let mut protocol_map = BTreeMap::new();
        for pair in items(protocols)?.chunks(2) {
            let [global, methods] = pair else { return Err("Odd protocols".into()) };
            let methods = items(methods)?
                .iter()
                .map(|method| {
                    let [name, signatures] = fixed(method)?;
                    let signatures = items(signatures)?
                        .chunks(2)
                        .map(|pair| match pair {
                            [arity, key] => Ok((index(arity)?, index(key)?)),
                            _ => Err("Odd signatures".to_owned()),
                        })
                        .collect::<R<_>>()?;
                    Ok(ProtocolMethod { name: decoder.string(name)?, signatures })
                })
                .collect::<R<_>>()?;
            protocol_map.insert(decoder.global(global)?, methods);
        }
        let current = items(current)?
            .chunks(2)
            .map(|pair| match pair {
                [phase, name] => Ok((decode_phase(phase)?, decoder.string(name)?)),
                _ => Err("Odd current namespaces".to_owned()),
            })
            .collect::<R<_>>()?;
        let macro_exports = items(macro_exports)?
            .chunks(3)
            .map(|triple| match triple {
                [phase, a, b] => Ok((decode_phase(phase)?, decoder.string(a)?, decoder.string(b)?)),
                _ => Err("Odd macro exports".to_owned()),
            })
            .collect::<R<_>>()?;
        Ok(Self {
            reader_state: crate::portable::syntax_quote::ReaderState::with_next_gensym(index(next_gensym)? as u64),
            namespaces: decode_phase_names(decoder, namespaces)?,
            bindings: binding_map,
            definitions: definition_map,
            source_generation: index(source_generation)? as u64,
            scopes: scope_map,
            current,
            materialized_bootstrap: decode_globals(decoder, materialized_bootstrap)?,
            macro_exports,
            macro_namespaces: decode_phase_names(decoder, macro_namespaces)?,
            protocols: protocol_map,
            core_protocols: decode_globals(decoder, core_protocols)?,
        })
    }
}
