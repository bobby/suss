//! Bounded native transport between reader forms and real compiled macro values.
//! Macro bodies execute in Wasm; this module constructs and reads nominal data.
use crate::portable_session::{Session, SessionError, SessionValue};
use std::{collections::BTreeMap, ops::Range};
use suss_compile::portable::Diagnostic;
use suss_reader::forms::{Form, Kind};
use wasmtime::{AnyRef, Rooted, StoreContextMut, Val};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    Symbol,
    Keyword,
    List,
    EmptyList,
    Cons,
    IndexedSeq,
    SourceArray,
    PersistentVector,
    VectorNode,
    PersistentArrayMap,
    PersistentHashMap,
    BitmapIndexedNode,
    ArrayNode,
    HashCollisionNode,
    NodeSeq,
    ArrayNodeSeq,
    MapEntry,
    PersistentArrayMapSeq,
    ChunkedSeq,
}
/// Captured canonical class roots remain valid through redefinition and GC.
/// A reset or another Store invalidates this bridge; construct a new one there.
pub struct FormBridge {
    roots: Vec<SessionValue>,
    classes: BTreeMap<i64, Class>,
    constructors: BTreeMap<Class, usize>,
    factories: BTreeMap<&'static str, SessionValue>,
}
impl FormBridge {
    pub fn new(session: &mut Session) -> Result<Self, SessionError> {
        let mut roots = Vec::new();
        let mut classes = BTreeMap::new();
        let mut constructors = BTreeMap::new();
        for (name, class) in [
            ("Symbol", Class::Symbol),
            ("Keyword", Class::Keyword),
            ("List", Class::List),
            ("EmptyList", Class::EmptyList),
            ("Cons", Class::Cons),
            ("IndexedSeq", Class::IndexedSeq),
            ("PersistentVector", Class::PersistentVector),
            ("VectorNode", Class::VectorNode),
            ("PersistentArrayMap", Class::PersistentArrayMap),
            ("PersistentHashMap", Class::PersistentHashMap),
            ("BitmapIndexedNode", Class::BitmapIndexedNode),
            ("ArrayNode", Class::ArrayNode),
            ("HashCollisionNode", Class::HashCollisionNode),
            ("NodeSeq", Class::NodeSeq),
            ("ArrayNodeSeq", Class::ArrayNodeSeq),
            ("MapEntry", Class::MapEntry),
            ("PersistentArrayMapSeq", Class::PersistentArrayMapSeq),
            ("ChunkedSeq", Class::ChunkedSeq),
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
            constructors.insert(class, roots.len());
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
        let mut factories = BTreeMap::new();
        for (name, source) in [
            (
                "array-map",
                "(.-createAsIfByAssoc suss.core/PersistentArrayMap)",
            ),
            ("vector", "(.-fromArray suss.core/PersistentVector)"),
            ("hash-map", "(.-fromArrays suss.core/PersistentHashMap)"),
            ("empty-map", "(.-EMPTY suss.core/PersistentArrayMap)"),
            ("empty-list", "(.-EMPTY suss.core/List)"),
            ("with-meta", "suss.core/with-meta"),
        ] {
            factories.insert(name, session.eval(source)?);
        }
        Ok(Self {
            roots,
            classes,
            constructors,
            factories,
        })
    }
    fn check(&self, session: &mut Session) -> Result<(), SessionError> {
        for root in &self.roots {
            session.inspect(root, |_, _| Ok(()))?;
        }
        Ok(())
    }
    /// Native canonical data construction preserves sharing in analysis graphs.
    /// Inputs and factories are rooted in this Store; no source is interpreted.
    pub fn scalar(
        &self,
        session: &mut Session,
        literal: &suss_compile::portable::hir::Literal,
    ) -> Result<SessionValue, SessionError> {
        self.check(session)?;
        session.data_scalar(literal)
    }
    pub fn map_values(
        &self,
        session: &mut Session,
        entries: &[(SessionValue, SessionValue)],
    ) -> Result<SessionValue, SessionError> {
        self.check(session)?;
        if entries.is_empty() {
            return Ok(self.factories["empty-map"].clone());
        }
        if entries.len() <= 8 {
            let arguments: Vec<_> = entries
                .iter()
                .flat_map(|(key, value)| [key, value])
                .collect();
            let array = session.data_array(&arguments)?;
            session.invoke(&self.factories["array-map"], &[&array])
        } else {
            let keys: Vec<_> = entries.iter().map(|(key, _)| key).collect();
            let values: Vec<_> = entries.iter().map(|(_, value)| value).collect();
            let keys = session.data_array(&keys)?;
            let values = session.data_array(&values)?;
            session.invoke(&self.factories["hash-map"], &[&keys, &values])
        }
    }
    pub fn vector_values(
        &self,
        session: &mut Session,
        items: &[SessionValue],
    ) -> Result<SessionValue, SessionError> {
        self.check(session)?;
        let arguments: Vec<_> = items.iter().collect();
        let array = session.data_array(&arguments)?;
        let owned = self.scalar(session, &suss_compile::portable::hir::Literal::Bool(true))?;
        session.invoke(&self.factories["vector"], &[&array, &owned])
    }
    pub fn list_values(
        &self,
        session: &mut Session,
        items: &[SessionValue],
    ) -> Result<SessionValue, SessionError> {
        self.check(session)?;
        let nil = self.scalar(session, &suss_compile::portable::hir::Literal::Nil)?;
        let mut tail = self.factories["empty-list"].clone();
        for (index, value) in items.iter().enumerate().rev() {
            let count = self.scalar(
                session,
                &suss_compile::portable::hir::Literal::Number((items.len() - index) as f64),
            )?;
            tail = session.data_construct(
                &self.roots[self.constructors[&Class::List]],
                &[&nil, value, &tail, &count, &nil],
            )?;
        }
        Ok(tail)
    }
    pub fn identifier(
        &self,
        session: &mut Session,
        namespace: Option<&str>,
        name: &str,
        keyword: bool,
        metadata: Option<&SessionValue>,
    ) -> Result<SessionValue, SessionError> {
        self.check(session)?;
        let nil = self.scalar(session, &suss_compile::portable::hir::Literal::Nil)?;
        let ns = if let Some(namespace) = namespace {
            self.scalar(
                session,
                &suss_compile::portable::hir::Literal::String(namespace.encode_utf16().collect()),
            )?
        } else {
            nil.clone()
        };
        let name_value = self.scalar(
            session,
            &suss_compile::portable::hir::Literal::String(name.encode_utf16().collect()),
        )?;
        let fqn = namespace.map_or_else(
            || name.to_owned(),
            |namespace| format!("{namespace}/{name}"),
        );
        let fqn = self.scalar(
            session,
            &suss_compile::portable::hir::Literal::String(fqn.encode_utf16().collect()),
        )?;
        let hash = self.scalar(
            session,
            &suss_compile::portable::hir::Literal::Number(
                suss_compile::portable::hir::identifier_hash(namespace, name, keyword) as f64,
            ),
        )?;
        if keyword {
            if metadata.is_some() {
                return Err(SessionError::Host(error(
                    "Keyword compiler data cannot carry metadata",
                )));
            }
            session.data_construct(
                &self.roots[self.constructors[&Class::Keyword]],
                &[&ns, &name_value, &fqn, &hash],
            )
        } else {
            session.data_construct(
                &self.roots[self.constructors[&Class::Symbol]],
                &[&ns, &name_value, &fqn, &hash, metadata.unwrap_or(&nil)],
            )
        }
    }
    pub fn quote(&self, session: &mut Session, form: Form) -> Result<SessionValue, SessionError> {
        self.check(session)?;
        let mut budget = Budget {
            nodes: 4096,
            units: 1_048_576,
        };
        self.form_value(session, &form, 0, &mut budget)
    }
    fn form_value(
        &self,
        session: &mut Session,
        form: &Form,
        depth: usize,
        budget: &mut Budget,
    ) -> Result<SessionValue, SessionError> {
        use suss_compile::portable::hir::Literal;
        let failure = |message: &str| {
            SessionError::Compile(Diagnostic {
                span: form.span.clone(),
                message: message.into(),
            })
        };
        if depth >= 64 {
            return Err(failure("Macro form construction exceeds 64 levels"));
        }
        spend(budget).map_err(SessionError::Host)?;
        let units = match &form.kind {
            Kind::String(value) => value.len(),
            Kind::Symbol(value) => {
                2 * (value.name.encode_utf16().count()
                    + value
                        .namespace
                        .as_ref()
                        .map_or(0, |ns| ns.encode_utf16().count() + 1))
            }
            Kind::Keyword(value) => {
                2 * (value.name.encode_utf16().count()
                    + value
                        .namespace
                        .as_ref()
                        .map_or(0, |ns| ns.encode_utf16().count() + 1))
            }
            _ => 0,
        };
        budget.units = budget
            .units
            .checked_sub(units)
            .ok_or_else(|| failure("Macro form construction exceeds UTF-16 storage bound"))?;
        let value = match &form.kind {
            Kind::Nil => self.scalar(session, &Literal::Nil)?,
            Kind::Bool(value) => self.scalar(session, &Literal::Bool(*value))?,
            Kind::Number(value) => self.scalar(session, &Literal::Number(*value))?,
            Kind::String(value) => self.scalar(session, &Literal::String(value.clone()))?,
            Kind::Symbol(value) => self.identifier(
                session,
                value.namespace.as_deref(),
                &value.name,
                false,
                None,
            )?,
            Kind::Keyword(value) => {
                self.identifier(session, value.namespace.as_deref(), &value.name, true, None)?
            }
            Kind::List(items) | Kind::Vector(items) => {
                let items = items
                    .iter()
                    .map(|item| self.form_value(session, item, depth + 1, budget))
                    .collect::<Result<Vec<_>, _>>()?;
                if matches!(form.kind, Kind::List(_)) {
                    self.list_values(session, &items)?
                } else {
                    self.vector_values(session, &items)?
                }
            }
            Kind::Map(items) => {
                if items.len() % 2 != 0 {
                    return Err(failure("Macro form map requires paired entries"));
                }
                let mut entries = Vec::new();
                for pair in items.chunks_exact(2) {
                    entries.push((
                        self.form_value(session, &pair[0], depth + 1, budget)?,
                        self.form_value(session, &pair[1], depth + 1, budget)?,
                    ));
                }
                self.map_values(session, &entries)?
            }
            Kind::Set(_) => {
                return Err(failure(
                    "Native macro set data requires persistent set support",
                ));
            }
            Kind::Conditional(_) | Kind::Discard(_) | Kind::Prefix { .. } => {
                return Err(failure(
                    "Native macro data requires resolved reader prefixes",
                ));
            }
        };
        if form.metadata.is_empty() {
            return Ok(value);
        }
        if !matches!(
            form.kind,
            Kind::Symbol(_) | Kind::List(_) | Kind::Vector(_) | Kind::Map(_) | Kind::Set(_)
        ) {
            return Err(failure("Metadata requires a symbol or collection"));
        }
        let pairs = suss_compile::portable::hir::reader_metadata_pairs(form)
            .map_err(SessionError::Compile)?;
        let metadata = Form {
            span: form.span.clone(),
            metadata: vec![],
            kind: Kind::Map(pairs),
        };
        let metadata = self.form_value(session, &metadata, depth + 1, budget)?;
        session.invoke(&self.factories["with-meta"], &[&value, &metadata])
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
fn metadata(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    classes: &BTreeMap<i64, Class>,
    span: &Range<usize>,
    depth: usize,
    budget: &mut Budget,
) -> wasmtime::Result<Vec<Form>> {
    if nil(store, value)? {
        return Ok(vec![]);
    }
    let form = decode(store, value, classes, span, depth + 1, budget)?;
    if !matches!(form.kind, Kind::Map(_)) {
        return Err(error(
            "Runtime macro metadata requires a canonical persistent map",
        ));
    }
    Ok(vec![form])
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
fn absent(store: &StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<bool> {
    // ABI2 nil (0) and undefined (6) both satisfy the pinned nil? storage test.
    Ok(matches!(sentinel(store, value)?, Some(0 | 6)))
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
    let mut reader_metadata = vec![];
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
            let slot = match class {
                Class::Symbol => Some(4),
                Class::PersistentVector
                | Class::PersistentArrayMap
                | Class::PersistentHashMap
                | Class::NodeSeq
                | Class::ArrayNodeSeq
                | Class::List
                | Class::Cons
                | Class::EmptyList => Some(0),
                Class::IndexedSeq | Class::PersistentArrayMapSeq => Some(2),
                Class::ChunkedSeq => Some(4),
                _ => None,
            };
            if let Some(value) = slot.and_then(|index| data.get(index)) {
                reader_metadata = metadata(store, value, classes, span, depth, budget)?;
            }
            match class {
                Class::Symbol | Class::Keyword => {
                    let count = if matches!(class, Class::Symbol) { 5 } else { 4 };
                    if data.len() != count {
                        return Err(error("Invalid identifier field layout"));
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
                Class::PersistentVector => {
                    Kind::Vector(vector(store, &data, classes, span, depth, budget)?)
                }
                Class::PersistentArrayMap => {
                    if data.len() != 4 {
                        return Err(error("Invalid persistent array map field layout"));
                    }
                    let count = integer(store, &data[1])?;
                    let entries = source_elements(store, &data[2], classes)?;
                    if entries.len() != count * 2 || entries.len() > budget.nodes {
                        return Err(error("Map count disagrees with bounded pair storage"));
                    }
                    Kind::Map(
                        entries
                            .iter()
                            .map(|entry| decode(store, entry, classes, span, depth + 1, budget))
                            .collect::<wasmtime::Result<Vec<_>>>()?,
                    )
                }
                Class::PersistentHashMap => {
                    Kind::Map(hash_map(store, &data, classes, span, depth, budget)?)
                }
                Class::BitmapIndexedNode | Class::ArrayNode | Class::HashCollisionNode => {
                    return Err(error("Hash trie nodes are not macro syntax"));
                }
                Class::MapEntry => {
                    if data.len() != 3 {
                        return Err(error("Invalid map entry field layout"));
                    }
                    Kind::Vector(
                        data[..2]
                            .iter()
                            .map(|entry| decode(store, entry, classes, span, depth + 1, budget))
                            .collect::<wasmtime::Result<Vec<_>>>()?,
                    )
                }
                Class::PersistentArrayMapSeq
                | Class::ChunkedSeq
                | Class::NodeSeq
                | Class::ArrayNodeSeq => {
                    Kind::List(sequence(store, value, classes, span, depth, budget)?)
                }
                Class::VectorNode => return Err(error("Vector trie nodes are not macro syntax")),
                Class::SourceArray => return Err(error("Raw source arrays are not macro syntax")),
                Class::List | Class::Cons | Class::EmptyList | Class::IndexedSeq => {
                    Kind::List(sequence(store, value, classes, span, depth, budget)?)
                }
            }
        }
    };
    Ok(Form {
        span: span.clone(),
        metadata: reader_metadata,
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
    if depth >= 64 {
        return Err(error("Macro sequence nesting exceeds 64"));
    }
    let mut cursor = value.clone();
    let mut items = Vec::new();
    let mut counts = Vec::new();
    let mut first_segment = true;
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
                if !first_segment {
                    metadata(store, &data[0], classes, span, depth, budget)?;
                }
                break;
            }
            Class::List | Class::Cons => {
                let count = if matches!(class, Class::List) { 5 } else { 4 };
                if data.len() != count {
                    return Err(error("Invalid sequence field layout"));
                }
                if !first_segment {
                    metadata(store, &data[0], classes, span, depth, budget)?;
                }
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
            Class::NodeSeq | Class::ArrayNodeSeq => {
                if data.len() != 5 {
                    return Err(error("Invalid hash node sequence field layout"));
                }
                if !first_segment {
                    metadata(store, &data[0], classes, span, depth, budget)?;
                }
                let nodes = source_elements(store, &data[1], classes)?;
                let index = integer(store, &data[2])?;
                let is_array = matches!(class, Class::ArrayNodeSeq);
                if index > nodes.len()
                    || (is_array && nodes.len() != 32)
                    || (!is_array && (nodes.len() % 2 != 0 || index % 2 != 0))
                {
                    return Err(error("Invalid bounded hash node sequence cursor"));
                }
                if !absent(store, &data[3])? {
                    let current = sequence(store, &data[3], classes, span, depth + 1, budget)?;
                    if current.is_empty() {
                        return Err(error("Hash node sequence retains an empty child cursor"));
                    }
                    items.extend(current);
                } else if is_array || index == nodes.len() || absent(store, &nodes[index])? {
                    return Err(error("Hash node sequence has no current entry"));
                }
                let mut pairs = Vec::new();
                if is_array {
                    for node in &nodes[index..] {
                        if !absent(store, node)? {
                            hash_node(store, node, classes, 0, budget, &mut pairs)?;
                        }
                    }
                } else {
                    for pair in nodes[index..].chunks_exact(2) {
                        if absent(store, &pair[0])? {
                            if !absent(store, &pair[1])? {
                                hash_node(store, &pair[1], classes, 0, budget, &mut pairs)?;
                            }
                        } else {
                            pairs.extend_from_slice(pair);
                        }
                        if pairs.len() > budget.nodes {
                            return Err(error("Hash node sequence exceeds bounded pair storage"));
                        }
                    }
                }
                for pair in pairs.chunks_exact(2) {
                    spend(budget)?;
                    let pair = pair
                        .iter()
                        .map(|entry| decode(store, entry, classes, span, depth + 2, budget))
                        .collect::<wasmtime::Result<Vec<_>>>()?;
                    items.push(Form {
                        span: span.clone(),
                        metadata: vec![],
                        kind: Kind::Vector(pair),
                    });
                }
                break;
            }
            Class::PersistentArrayMapSeq => {
                if data.len() != 3 {
                    return Err(error("Invalid array map sequence field layout"));
                }
                if !first_segment {
                    metadata(store, &data[2], classes, span, depth, budget)?;
                }
                let index = integer(store, &data[1])?;
                let entries = source_elements(store, &data[0], classes)?;
                if entries.len() % 2 != 0
                    || index % 2 != 0
                    || index >= entries.len()
                    || entries.len() - index > budget.nodes
                {
                    return Err(error("Invalid bounded array map sequence pair storage"));
                }
                for pair in entries[index..].chunks_exact(2) {
                    spend(budget)?;
                    let pair = pair
                        .iter()
                        // The generated entry vector is one syntax level below
                        // this sequence; its key/value children are another.
                        .map(|entry| decode(store, entry, classes, span, depth + 2, budget))
                        .collect::<wasmtime::Result<Vec<_>>>()?;
                    items.push(Form {
                        span: span.clone(),
                        metadata: vec![],
                        kind: Kind::Vector(pair),
                    });
                }
                break;
            }
            Class::ChunkedSeq => {
                if data.len() != 6 {
                    return Err(error("Invalid chunked vector sequence layout"));
                }
                if !first_segment {
                    metadata(store, &data[4], classes, span, depth, budget)?;
                }
                let (owner, vector_data) = object(store, &data[0], classes)?;
                if !matches!(owner, Class::PersistentVector) || vector_data.len() != 6 {
                    return Err(error(
                        "Chunked vector sequence needs canonical vector storage",
                    ));
                }
                let count = integer(store, &vector_data[1])?;
                let mut index = integer(store, &data[2])?;
                let mut offset = integer(store, &data[3])?;
                let mut node = source_elements(store, &data[1], classes)?;
                if index >= count || offset >= node.len() {
                    return Err(error("Invalid chunked vector sequence index"));
                }
                // Validate the backing vector even when the first node exhausts
                // the sequence; use the supplied node for its actual contents.
                vector_leaf(store, &vector_data, index, classes, budget)?;
                loop {
                    if node.is_empty() || node.len() > 32 || index + node.len() > count {
                        return Err(error("Invalid bounded chunked vector sequence node"));
                    }
                    for entry in &node[offset..] {
                        items.push(decode(store, entry, classes, span, depth + 1, budget)?);
                    }
                    index += node.len();
                    if index == count {
                        break;
                    }
                    node = vector_leaf(store, &vector_data, index, classes, budget)?;
                    offset = 0;
                }
                break;
            }
            Class::IndexedSeq => {
                if !first_segment && data.len() == 3 {
                    metadata(store, &data[2], classes, span, depth, budget)?;
                }
                append_indexed(store, &data, classes, span, depth, budget, &mut items)?;
                break;
            }
            _ => return Err(error("Unsupported macro sequence tail")),
        }
        first_segment = false;
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

fn integer(store: &mut StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<usize> {
    let data = fields(store, value, 1)?;
    let [Val::F64(bits)] = data.as_slice() else {
        return Err(error("Invalid vector integer field"));
    };
    let n = f64::from_bits(*bits);
    if !n.is_finite() || n < 0.0 || n.fract() != 0.0 || n > 4096.0 {
        return Err(error("Vector integer field exceeds transport bounds"));
    }
    Ok(n as usize)
}
fn source_elements(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    classes: &BTreeMap<i64, Class>,
) -> wasmtime::Result<Vec<Val>> {
    let (class, owner) = object(store, value, classes)?;
    if !matches!(class, Class::SourceArray) || owner.len() != 1 {
        return Err(error("Vector needs canonical source array storage"));
    }
    array(store, &owner[0])
}
fn node_elements(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    classes: &BTreeMap<i64, Class>,
    budget: &mut Budget,
) -> wasmtime::Result<Vec<Val>> {
    spend(budget)?;
    let (class, data) = object(store, value, classes)?;
    if !matches!(class, Class::VectorNode) || data.len() != 2 {
        return Err(error("Invalid persistent vector trie node"));
    }
    let items = source_elements(store, &data[1], classes)?;
    if items.len() != 32 {
        return Err(error("Invalid vector trie array width"));
    }
    Ok(items)
}
fn vector(
    store: &mut StoreContextMut<'_, ()>,
    data: &[Val],
    classes: &BTreeMap<i64, Class>,
    span: &Range<usize>,
    depth: usize,
    budget: &mut Budget,
) -> wasmtime::Result<Vec<Form>> {
    if data.len() != 6 {
        return Err(error("Invalid persistent vector layout"));
    }
    let count = integer(store, &data[1])?;
    let shift = integer(store, &data[2])?;
    if shift < 5 || shift > 30 || shift % 5 != 0 {
        return Err(error("Invalid vector trie shift"));
    }
    let tail = source_elements(store, &data[4], classes)?;
    let offset = if count < 32 {
        0
    } else {
        ((count - 1) >> 5) << 5
    };
    if tail.len() != count - offset {
        return Err(error("Vector count disagrees with tail storage"));
    }
    // persistent! clears only the root edit token. Descendant nodes may retain
    // their former token; the pinned persistent trie legitimately shares them.
    let (_, root_data) = object(store, &data[3], classes)?;
    if root_data.len() != 2 || !nil(store, &root_data[0])? {
        return Err(error("Persistent vector root retains an active edit token"));
    }
    let root = node_elements(store, &data[3], classes, budget)?;
    let mut items = Vec::new();
    for base in (0..offset).step_by(32) {
        let mut node = root.clone();
        let mut level = shift;
        while level > 0 {
            node = node_elements(store, &node[(base >> level) & 31], classes, budget)?;
            level -= 5;
        }
        for value in node {
            items.push(decode(store, &value, classes, span, depth + 1, budget)?);
        }
    }
    for value in tail {
        items.push(decode(store, &value, classes, span, depth + 1, budget)?);
    }
    Ok(items)
}

// Read the same canonical trie/tail storage used by unchecked-array-for. This
// adapter decodes syntax; it never invokes arbitrary user collection protocols.
fn vector_leaf(
    store: &mut StoreContextMut<'_, ()>,
    data: &[Val],
    index: usize,
    classes: &BTreeMap<i64, Class>,
    budget: &mut Budget,
) -> wasmtime::Result<Vec<Val>> {
    let count = integer(store, &data[1])?;
    let shift = integer(store, &data[2])?;
    if shift < 5 || shift > 30 || shift % 5 != 0 || index >= count {
        return Err(error("Invalid chunked vector trie range"));
    }
    let tail_offset = if count < 32 {
        0
    } else {
        ((count - 1) >> 5) << 5
    };
    let (_, root_data) = object(store, &data[3], classes)?;
    if root_data.len() != 2 || !nil(store, &root_data[0])? {
        return Err(error("Persistent vector root retains an active edit token"));
    }
    let mut node = node_elements(store, &data[3], classes, budget)?;
    if index >= tail_offset {
        let tail = source_elements(store, &data[4], classes)?;
        if tail.len() != count - tail_offset {
            return Err(error("Vector count disagrees with tail storage"));
        }
        return Ok(tail);
    }
    let mut level = shift;
    while level > 0 {
        node = node_elements(store, &node[(index >> level) & 31], classes, budget)?;
        level -= 5;
    }
    Ok(node)
}

// Traverse the pinned inode ordering directly, without invoking user protocols.
// Canonical descriptors, logical counts, bounded depth and the shared budget
// guard this transport even when source code mutates a node's physical fields.
fn hash_map(
    store: &mut StoreContextMut<'_, ()>,
    data: &[Val],
    classes: &BTreeMap<i64, Class>,
    span: &Range<usize>,
    depth: usize,
    budget: &mut Budget,
) -> wasmtime::Result<Vec<Form>> {
    if data.len() != 6 {
        return Err(error("Invalid persistent hash map field layout"));
    }
    let count = integer(store, &data[1])?;
    if count > budget.nodes / 2 {
        return Err(error("Hash map exceeds bounded pair storage"));
    }
    let has_nil = match sentinel(store, &data[3])? {
        Some(2) => false,
        Some(4) => true,
        _ => return Err(error("Invalid hash map nil-key flag")),
    };
    let mut pairs = Vec::new();
    if has_nil {
        pairs.push(Val::AnyRef(Some(AnyRef::from_i31(
            &mut *store,
            wasmtime::I31::new_u32(0).expect("zero fits i31"),
        ))));
        pairs.push(data[4].clone());
    }
    if !nil(store, &data[2])? {
        hash_node(store, &data[2], classes, 0, budget, &mut pairs)?;
    }
    if pairs.len() != count * 2 {
        return Err(error("Hash map count disagrees with trie storage"));
    }
    pairs
        .iter()
        .map(|entry| decode(store, entry, classes, span, depth + 1, budget))
        .collect()
}
fn bitmap(store: &mut StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<u32> {
    let data = fields(store, value, 1)?;
    let [Val::F64(bits)] = data.as_slice() else {
        return Err(error("Invalid hash trie bitmap"));
    };
    let n = f64::from_bits(*bits);
    if !n.is_finite() || n.fract() != 0.0 || n < i32::MIN as f64 || n > i32::MAX as f64 {
        return Err(error("Invalid signed hash trie bitmap"));
    }
    Ok(n as i32 as u32)
}
fn hash_node(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    classes: &BTreeMap<i64, Class>,
    level: usize,
    budget: &mut Budget,
    pairs: &mut Vec<Val>,
) -> wasmtime::Result<()> {
    if level > 7 {
        return Err(error("Hash trie depth exceeds 32-bit hash path"));
    }
    spend(budget)?;
    let (class, data) = object(store, value, classes)?;
    match class {
        Class::BitmapIndexedNode => {
            if data.len() != 3 {
                return Err(error("Invalid bitmap node field layout"));
            }
            let count = bitmap(store, &data[1])?.count_ones() as usize;
            let entries = source_elements(store, &data[2], classes)?;
            if entries.len() < count * 2 || entries.len() % 2 != 0 {
                return Err(error("Bitmap population exceeds pair storage"));
            }
            for pair in entries[..count * 2].chunks_exact(2) {
                if absent(store, &pair[0])? {
                    hash_node(store, &pair[1], classes, level + 1, budget, pairs)?;
                } else {
                    pairs.extend_from_slice(pair);
                }
            }
        }
        Class::ArrayNode => {
            if data.len() != 3 {
                return Err(error("Invalid array node field layout"));
            }
            let count = integer(store, &data[1])?;
            let entries = source_elements(store, &data[2], classes)?;
            if entries.len() != 32 {
                return Err(error("Invalid hash array node width"));
            }
            let mut occupied = 0;
            for entry in entries {
                if !absent(store, &entry)? {
                    occupied += 1;
                    hash_node(store, &entry, classes, level + 1, budget, pairs)?;
                }
            }
            if occupied != count {
                return Err(error("Array node count disagrees with storage"));
            }
        }
        Class::HashCollisionNode => {
            if data.len() != 4 {
                return Err(error("Invalid collision node field layout"));
            }
            bitmap(store, &data[1])?;
            let count = integer(store, &data[2])?;
            let entries = source_elements(store, &data[3], classes)?;
            if count == 0 || entries.len() < count * 2 || entries.len() % 2 != 0 {
                return Err(error("Collision count exceeds pair storage"));
            }
            for pair in entries[..count * 2].chunks_exact(2) {
                if absent(store, &pair[0])? {
                    return Err(error("Nil key in hash collision node"));
                }
                pairs.extend_from_slice(pair);
            }
        }
        _ => return Err(error("Hash map needs canonical trie nodes")),
    }
    if pairs.len() > budget.nodes {
        return Err(error("Hash trie exceeds bounded pair storage"));
    }
    Ok(())
}
