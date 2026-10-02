//! Bounded native transport between reader forms and real compiled macro values.
//! Macro bodies execute in Wasm; this module only reads their nominal data results.
use crate::portable_session::{Session, SessionError, SessionValue};
use std::{collections::BTreeMap, ops::Range};
use suss_compile::portable::Diagnostic;
use suss_reader::forms::{Form, Kind};
use wasmtime::{AnyRef, Rooted, StoreContextMut, Val};

#[derive(Clone, Copy)]
enum Class {
    Symbol,
    Keyword,
    List,
    EmptyList,
    Cons,
    IndexedSeq,
    SourceArray,
}
/// Captured canonical class roots remain valid through redefinition and GC.
/// A reset or another Store invalidates this bridge; construct a new one there.
pub struct FormBridge {
    roots: Vec<SessionValue>,
    classes: BTreeMap<i64, Class>,
}
impl FormBridge {
    pub fn new(session: &mut Session) -> Result<Self, SessionError> {
        let mut roots = Vec::new();
        let mut classes = BTreeMap::new();
        for (name, class) in [
            ("Symbol", Class::Symbol),
            ("Keyword", Class::Keyword),
            ("List", Class::List),
            ("EmptyList", Class::EmptyList),
            ("Cons", Class::Cons),
            ("IndexedSeq", Class::IndexedSeq),
        ] {
            let value = session.eval(&format!("suss.core/{name}"))?;
            let id = session.inspect(&value, |mut store, value| {
                let closure = fields(&mut store, &value, 5)?;
                let owner = fields(&mut store, &closure[0], 4)?;
                let storage = array(&mut store, &owner[1])?;
                if storage.len() != 2 {
                    return Err(error("Invalid class property owner"));
                }
                let descriptor = fields(&mut store, &storage[0], 5)?;
                match descriptor[0] {
                    Val::I64(id) => Ok(id),
                    _ => Err(error("Invalid class descriptor identity")),
                }
            })?;
            if classes.insert(id, class).is_some() {
                return Err(SessionError::Host(error(
                    "Duplicate macro data class identity",
                )));
            }
            roots.push(value);
        }
        // Source arrays have a private runtime descriptor rather than a public
        // class constructor. Capture its actual identity from one owned sample.
        let value = session.eval("(suss.core/array)")?;
        let id = session.inspect(&value, |mut store, value| {
            let owner = fields(&mut store, &value, 4)?;
            let descriptor = fields(&mut store, &owner[0], 5)?;
            match descriptor[0] {
                Val::I64(id) => Ok(id),
                _ => Err(error("Invalid source array descriptor identity")),
            }
        })?;
        if classes.insert(id, Class::SourceArray).is_some() {
            return Err(SessionError::Host(error(
                "Duplicate macro data array identity",
            )));
        }
        roots.push(value);
        Ok(Self { roots, classes })
    }
    fn check(&self, session: &mut Session) -> Result<(), SessionError> {
        for root in &self.roots {
            session.inspect(root, |_, _| Ok(()))?;
        }
        Ok(())
    }
    pub fn quote(&self, session: &mut Session, form: Form) -> Result<SessionValue, SessionError> {
        self.check(session)?;
        let span = form.span.clone();
        let operator = Form {
            span: span.clone(),
            metadata: vec![],
            kind: Kind::Symbol(suss_reader::Symbol {
                namespace: None,
                name: "quote".into(),
            }),
        };
        session.eval_forms(
            vec![Form {
                span: span.clone(),
                metadata: vec![],
                kind: Kind::List(vec![operator, form]),
            }],
            span,
        )
    }
    /// Original result locations are not encoded in runtime values. Attribute
    /// expansion data to the supplied macro call site; never invent source bytes.
    pub fn read(
        &self,
        session: &mut Session,
        value: &SessionValue,
        span: Range<usize>,
    ) -> Result<Form, SessionError> {
        self.check(session)?;
        session
            .inspect(value, |mut store, value| {
                let mut budget = Budget {
                    nodes: 4096,
                    units: 1_048_576,
                };
                decode(&mut store, &value, &self.classes, &span, 0, &mut budget)
            })
            .map_err(|failure| match failure {
                SessionError::Host(failure) => SessionError::Compile(Diagnostic {
                    span,
                    message: format!("Invalid or unsupported compiled macro data: {failure}"),
                }),
                failure => failure,
            })
    }
}
fn error(message: &str) -> wasmtime::Error {
    wasmtime::Error::msg(message.to_owned())
}
fn reference(value: &Val) -> wasmtime::Result<&Rooted<AnyRef>> {
    value
        .anyref()
        .flatten()
        .ok_or_else(|| error("Expected non-null language reference"))
}
fn fields(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    count: usize,
) -> wasmtime::Result<Vec<Val>> {
    let object = reference(value)?
        .as_struct(&*store)?
        .ok_or_else(|| error("Expected typed structure"))?;
    let fields = object.fields(&mut *store)?.collect::<Vec<_>>();
    if fields.len() != count {
        return Err(error("Unexpected structure layout"));
    }
    Ok(fields)
}
fn array(store: &mut StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<Vec<Val>> {
    let array = reference(value)?
        .as_array(&*store)?
        .ok_or_else(|| error("Expected field array"))?;
    if array.len(&*store)? > 4096 {
        return Err(error("Macro data field array exceeds bound"));
    }
    Ok(array.elems(&mut *store)?.collect())
}
fn sentinel(store: &StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<Option<u32>> {
    Ok(reference(value)?
        .as_i31(store)?
        .map(|value| value.get_u32()))
}
fn nil(store: &StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<bool> {
    Ok(sentinel(store, value)? == Some(0))
}
fn metadata(store: &StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<()> {
    if nil(store, value)? {
        Ok(())
    } else {
        Err(error(
            "Runtime macro metadata requires persistent metadata maps, not yet integrated",
        ))
    }
}
fn text(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    budget: &mut Budget,
) -> wasmtime::Result<Vec<u16>> {
    let array = reference(value)?
        .as_array(&*store)?
        .ok_or_else(|| error("Expected UTF16 string"))?;
    if !matches!(
        array.ty(&*store)?.element_type(),
        wasmtime::StorageType::I16
    ) {
        return Err(error("Expected UTF16 element storage"));
    }
    budget.units = budget
        .units
        .checked_sub(array.len(&*store)? as usize)
        .ok_or_else(|| error("Macro data exceeds total UTF16 storage bound"))?;
    array
        .elems(&mut *store)?
        .map(|unit| match unit {
            Val::I32(unit) => Ok(unit as u16),
            _ => Err(error("Invalid UTF16 element")),
        })
        .collect()
}
fn name(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    budget: &mut Budget,
) -> wasmtime::Result<String> {
    String::from_utf16(&text(store, value, budget)?)
        .map_err(|_| error("Macro identifier contains a lone surrogate"))
}
fn object(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    classes: &BTreeMap<i64, Class>,
) -> wasmtime::Result<(Class, Vec<Val>)> {
    let storage = fields(store, value, 4)?;
    let descriptor = fields(store, &storage[0], 5)?;
    let Val::I64(id) = descriptor[0] else {
        return Err(error("Invalid nominal descriptor identity"));
    };
    let class = classes
        .get(&id)
        .copied()
        .ok_or_else(|| error("Unrecognized nominal macro data type"))?;
    Ok((class, array(store, &storage[1])?))
}
struct Budget {
    nodes: usize,
    units: usize,
}
fn spend(budget: &mut Budget) -> wasmtime::Result<()> {
    budget.nodes = budget
        .nodes
        .checked_sub(1)
        .ok_or_else(|| error("Macro data traversal exceeds 4096 nodes"))?;
    Ok(())
}
fn decode(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    classes: &BTreeMap<i64, Class>,
    span: &Range<usize>,
    depth: usize,
    budget: &mut Budget,
) -> wasmtime::Result<Form> {
    if depth >= 64 {
        return Err(error("Macro data nesting exceeds 64"));
    }
    spend(budget)?;
    let reference = reference(value)?;
    let kind = if let Some(sentinel) = sentinel(store, value)? {
        match sentinel {
            0 => Kind::Nil,
            2 => Kind::Bool(false),
            4 => Kind::Bool(true),
            _ => return Err(error("Unknown language sentinel")),
        }
    } else if reference.as_array(&*store)?.is_some() {
        Kind::String(text(store, value, budget)?)
    } else {
        let structure = reference
            .as_struct(&*store)?
            .ok_or_else(|| error("Unsupported macro value"))?;
        let values = structure.fields(&mut *store)?.collect::<Vec<_>>();
        if let [Val::F64(bits)] = values.as_slice() {
            Kind::Number(f64::from_bits(*bits))
        } else {
            let (class, data) = object(store, value, classes)?;
            match class {
                Class::Symbol | Class::Keyword => {
                    let count = if matches!(class, Class::Symbol) { 5 } else { 4 };
                    if data.len() != count {
                        return Err(error("Invalid identifier field layout"));
                    }
                    if count == 5 {
                        metadata(store, &data[4])?;
                    }
                    let namespace = if nil(store, &data[0])? {
                        None
                    } else {
                        Some(name(store, &data[0], budget)?)
                    };
                    let name = name(store, &data[1], budget)?;
                    if count == 5 {
                        Kind::Symbol(suss_reader::Symbol { namespace, name })
                    } else {
                        Kind::Keyword(suss_reader::Keyword { namespace, name })
                    }
                }
                Class::SourceArray => return Err(error("Raw source arrays are not macro syntax")),
                Class::List | Class::Cons | Class::EmptyList | Class::IndexedSeq => {
                    Kind::List(sequence(store, value, classes, span, depth, budget)?)
                }
            }
        }
    };
    Ok(Form {
        span: span.clone(),
        metadata: vec![],
        kind,
    })
}
fn sequence(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    classes: &BTreeMap<i64, Class>,
    span: &Range<usize>,
    depth: usize,
    budget: &mut Budget,
) -> wasmtime::Result<Vec<Form>> {
    let mut cursor = value.clone();
    let mut items = Vec::new();
    let mut counts = Vec::new();
    loop {
        if nil(store, &cursor)? {
            break;
        }
        spend(budget)?;
        let (class, data) = object(store, &cursor, classes)?;
        match class {
            Class::EmptyList => {
                if data.len() != 1 {
                    return Err(error("Invalid empty list layout"));
                }
                metadata(store, &data[0])?;
                break;
            }
            Class::List | Class::Cons => {
                let count = if matches!(class, Class::List) { 5 } else { 4 };
                if data.len() != count {
                    return Err(error("Invalid sequence field layout"));
                }
                metadata(store, &data[0])?;
                if count == 5 {
                    let boxed = fields(store, &data[3], 1)?;
                    let [Val::F64(bits)] = boxed.as_slice() else {
                        return Err(error("Invalid list count"));
                    };
                    counts.push((items.len(), f64::from_bits(*bits)));
                }
                items.push(decode(store, &data[1], classes, span, depth + 1, budget)?);
                cursor = data[2].clone();
            }
            Class::IndexedSeq => {
                append_indexed(store, &data, classes, span, depth, budget, &mut items)?;
                break;
            }
            _ => return Err(error("Unsupported macro sequence tail")),
        }
    }
    for (index, count) in counts {
        if count != (items.len() - index) as f64 {
            return Err(error("List count disagrees with sequence storage"));
        }
    }
    Ok(items)
}

fn append_indexed(
    store: &mut StoreContextMut<'_, ()>,
    data: &[Val],
    classes: &BTreeMap<i64, Class>,
    span: &Range<usize>,
    depth: usize,
    budget: &mut Budget,
    items: &mut Vec<Form>,
) -> wasmtime::Result<()> {
    if data.len() != 3 {
        return Err(error("Invalid IndexedSeq field layout"));
    }
    metadata(store, &data[2])?;
    let index = fields(store, &data[1], 1)?;
    let [Val::F64(bits)] = index.as_slice() else {
        return Err(error("Invalid IndexedSeq index"));
    };
    let index = f64::from_bits(*bits);
    if !index.is_finite() || index < 0.0 || index.fract() != 0.0 {
        return Err(error("Invalid IndexedSeq index"));
    }
    let backing = if let Some(array) = reference(&data[0])?.as_array(&*store)? {
        if !matches!(
            array.ty(&*store)?.element_type(),
            wasmtime::StorageType::I16
        ) {
            return Err(error("IndexedSeq string needs UTF16 storage"));
        }
        (array, true)
    } else {
        let (class, owner) = object(store, &data[0], classes)?;
        if !matches!(class, Class::SourceArray) || owner.len() != 1 {
            return Err(error("IndexedSeq needs actual source array storage"));
        }
        let array = reference(&owner[0])?
            .as_array(&*store)?
            .ok_or_else(|| error("Invalid source array element storage"))?;
        if !matches!(
            array.ty(&*store)?.element_type(),
            wasmtime::StorageType::ValType(wasmtime::ValType::Ref(_))
        ) {
            return Err(error("Invalid source array element storage"));
        }
        (array, false)
    };
    let length = backing.0.len(&*store)?;
    if index > f64::from(length) {
        return Err(error("IndexedSeq index exceeds backing storage"));
    }
    let index = index as u32;
    if (length - index) as usize > budget.nodes {
        return Err(error("IndexedSeq exceeds macro data traversal bound"));
    }
    for offset in index..length {
        let value = backing.0.get(&mut *store, offset)?;
        if backing.1 {
            spend(budget)?;
            budget.units = budget
                .units
                .checked_sub(1)
                .ok_or_else(|| error("Macro data exceeds total UTF16 storage bound"))?;
            let Val::I32(unit) = value else {
                return Err(error("Invalid UTF16 indexed unit"));
            };
            items.push(Form {
                span: span.clone(),
                metadata: vec![],
                kind: Kind::String(vec![unit as u16]),
            });
        } else {
            items.push(decode(store, &value, classes, span, depth + 1, budget)?);
        }
    }
    Ok(())
}
