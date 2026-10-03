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
    PersistentHashSet,
    BitmapIndexedNode,
    ArrayNode,
    HashCollisionNode,
    NodeSeq,
    ArrayNodeSeq,
    MapEntry,
    KeySeq,
    PersistentArrayMapSeq,
    ChunkedSeq,
    LazySeq,
    ChunkedCons,
    ArrayChunk,
}
#[derive(Clone, Copy)]
pub(crate) enum MetadataShape {
    Symbol,
    List { empty: bool },
    Vector,
    Map { large: bool },
    Set,
}
/// Captured canonical class roots remain valid through redefinition and GC.
/// A reset or another Store invalidates this bridge; construct a new one there.
pub struct FormBridge {
    roots: Vec<SessionValue>,
    classes: BTreeMap<i64, (Class, SessionValue)>,
    constructors: BTreeMap<Class, usize>,
    factories: BTreeMap<&'static str, SessionValue>,
}
impl FormBridge {
    pub(crate) fn with_form_metadata(
        &self,
        session: &mut Session,
        shape: MetadataShape,
        value: &SessionValue,
        metadata: &SessionValue,
        set_items: &[SessionValue],
    ) -> Result<SessionValue, SessionError> {
        self.check(session)?;
        let class = match shape {
            MetadataShape::Set => return self.set_with_metadata(session, set_items, metadata),
            MetadataShape::Symbol => Class::Symbol,
            MetadataShape::List { empty: true } => Class::EmptyList,
            MetadataShape::List { empty: false } => Class::List,
            MetadataShape::Vector => Class::PersistentVector,
            MetadataShape::Map { large: false } => Class::PersistentArrayMap,
            MetadataShape::Map { large: true } => Class::PersistentHashMap,
        };
        let mut fields = session.data_fields(value)?;
        fields[if class == Class::Symbol { 4 } else { 0 }] = metadata.clone();
        session.data_construct(
            &self.roots[self.constructors[&class]],
            &fields.iter().collect::<Vec<_>>(),
        )
    }
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
            ("PersistentHashSet", Class::PersistentHashSet),
            ("BitmapIndexedNode", Class::BitmapIndexedNode),
            ("ArrayNode", Class::ArrayNode),
            ("HashCollisionNode", Class::HashCollisionNode),
            ("NodeSeq", Class::NodeSeq),
            ("ArrayNodeSeq", Class::ArrayNodeSeq),
            ("MapEntry", Class::MapEntry),
            ("KeySeq", Class::KeySeq),
            ("PersistentArrayMapSeq", Class::PersistentArrayMapSeq),
            ("ChunkedSeq", Class::ChunkedSeq),
            ("LazySeq", Class::LazySeq),
            ("ChunkedCons", Class::ChunkedCons),
            ("ArrayChunk", Class::ArrayChunk),
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
            let descriptor = session.data_descriptor(&value, true)?;
            if classes.insert(id, (class, descriptor)).is_some() {
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
        let descriptor = session.data_descriptor(&value, false)?;
        if classes.insert(id, (Class::SourceArray, descriptor)).is_some() {
            return Err(SessionError::Host(error(
                "Duplicate macro data array identity",
            )));
        }
        roots.push(value);
        let mut factories = BTreeMap::new();
        for (name, source) in [
            ("hash", "suss.core/hash"),
            ("key-test", "suss.core/key-test"),
            ("empty-map", "(.-EMPTY suss.core/PersistentArrayMap)"),
            ("empty-set", "(.-EMPTY suss.core/PersistentHashSet)"),
            ("empty-list", "(.-EMPTY suss.core/List)"),
            ("lazy-sval", "(fn* [value] (.sval value))"),
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
        if entries.len() > 65_536 {
            return Err(SessionError::Host(error("Compiler map construction exceeds 65536 entries")));
        }
        if entries.is_empty() {
            return Ok(self.factories["empty-map"].clone());
        }
        // Bulk construction is original host code. Retained factories resolve
        // public class cells dynamically, so capturing their closures alone is
        // insufficient to keep compiler data canonical after core redefinition.
        let nil = self.scalar(session, &suss_compile::portable::hir::Literal::Nil)?;
        let mut pairs: Vec<(u32, SessionValue, SessionValue)> = Vec::new();
        let mut hashes: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        let mut nil_value = None;
        let mut nil_index = 0;
        let mut comparisons = 1_048_576usize;
        for (key, value) in entries {
            session.inspect(value, |_, _| Ok(()))?;
            let is_nil = session.inspect(key, |store, key| Ok(absent(&store, &key)?))?;
            if is_nil {
                if nil_value.is_none() {
                    nil_index = pairs.len();
                }
                nil_value = Some(value.clone());
                continue;
            }
            // Array maps compare keys directly and do not call their hash
            // protocols. Preserve that observable behavior at the threshold.
            let hash = if entries.len() <= 8 { 0 } else {
                let hash = session.invoke(&self.factories["hash"], &[key])?;
                session.inspect(&hash, |mut store, hash| {
                    let data = fields(&mut store, &hash, 1)?;
                    let [Val::F64(bits)] = data.as_slice() else {
                        return Err(error("Compiler map hash requires a numeric scalar"));
                    };
                    let value = f64::from_bits(*bits);
                    // ECMAScript ToUint32: truncation followed by modulo 2^32,
                    // with nonfinite numbers and signed zero mapped to zero.
                    Ok(if value.is_finite() { value.trunc().rem_euclid(4_294_967_296.0) as u32 } else { 0 })
                })?
            };
            let mut existing = None;
            for index in hashes.entry(hash).or_default().iter().copied() {
                comparisons = comparisons.checked_sub(1).ok_or_else(|| {
                    SessionError::Host(error("Compiler map key comparisons exceed bound"))
                })?;
                let equal = if let Some(identifier) = self.identifier_key(session, key)? {
                    self.identifier_key(session, &pairs[index].1)?.is_some_and(|other| other == identifier)
                } else {
                    let equal = session.invoke(&self.factories["key-test"], &[key, &pairs[index].1])?;
                    session.inspect(&equal, |store, value| Ok(sentinel(&store, &value)? == Some(4)))?
                };
                if equal {
                    existing = Some(index);
                    break;
                }
            }
            if let Some(index) = existing {
                pairs[index].2 = value.clone();
            } else {
                hashes.get_mut(&hash).unwrap().push(pairs.len());
                pairs.push((hash, key.clone(), value.clone()));
            }
        }
        let count = pairs.len() + usize::from(nil_value.is_some());
        let count = self.scalar(
            session,
            &suss_compile::portable::hir::Literal::Number(count as f64),
        )?;
        if entries.len() <= 8 {
            let mut data = pairs
                .iter()
                .flat_map(|(_, key, value)| [key, value])
                .collect::<Vec<_>>();
            if let Some(value) = &nil_value {
                data.splice(nil_index * 2..nil_index * 2, [&nil, value]);
            }
            let array = session.data_array(&data)?;
            session.data_construct(
                &self.roots[self.constructors[&Class::PersistentArrayMap]],
                &[&nil, &count, &array, &nil],
            )
        } else {
            let root = self.map_node(session, &pairs, 0, &nil)?;
            let has_nil = self.scalar(
                session,
                &suss_compile::portable::hir::Literal::Bool(nil_value.is_some()),
            )?;
            session.data_construct(
                &self.roots[self.constructors[&Class::PersistentHashMap]],
                &[
                    &nil,
                    &count,
                    &root,
                    &has_nil,
                    nil_value.as_ref().unwrap_or(&nil),
                    &nil,
                ],
            )
        }
    }
    // Identifier equality is independent of metadata and live constructor
    // globals. Other user values retain the captured observable key-test path.
    fn identifier_key(&self, session: &mut Session, value: &SessionValue) -> Result<Option<(Class, Vec<u16>)>, SessionError> {
        session.inspect(value, |mut store, value| {
            let Some(structure) = reference(&value)?.as_struct(&store)? else { return Ok(None); };
            let storage = structure.fields(&mut store)?.collect::<Vec<_>>();
            if storage.len() != 4 { return Ok(None); }
            let descriptor = fields(&mut store, &storage[0], 5)?;
            let Val::I64(id) = descriptor[0] else { return Ok(None); };
            if !self.classes.get(&id).is_some_and(|(class, _)| matches!(class, Class::Symbol | Class::Keyword)) { return Ok(None); }
            let (class, data) = object(&mut store, &value, &self.classes)?;
            if data.len() != if class == Class::Symbol { 5 } else { 4 } {
                return Err(error("Invalid compiler identifier key layout"));
            }
            let mut budget = Budget { include_metadata: true, nodes: 4096, units: 1_048_576, calls: None };
            Ok(Some((class, text(&mut store, &data[2], &mut budget)?)))
        })
    }
    fn map_node(
        &self,
        session: &mut Session,
        pairs: &[(u32, SessionValue, SessionValue)],
        shift: u32,
        nil: &SessionValue,
    ) -> Result<SessionValue, SessionError> {
        use suss_compile::portable::hir::Literal;
        if pairs.is_empty() {
            return Ok(nil.clone());
        }
        if pairs.len() > 1 && pairs.iter().all(|pair| pair.0 == pairs[0].0) {
            let hash = self.scalar(session, &Literal::Number(pairs[0].0 as i32 as f64))?;
            let count = self.scalar(session, &Literal::Number(pairs.len() as f64))?;
            let items = pairs
                .iter()
                .flat_map(|(_, key, value)| [key, value])
                .collect::<Vec<_>>();
            let array = session.data_array(&items)?;
            return session.data_construct(
                &self.roots[self.constructors[&Class::HashCollisionNode]],
                &[nil, &hash, &count, &array],
            );
        }
        if shift > 30 {
            return Err(SessionError::Host(error(
                "Compiler map hash path exceeds 32 bits",
            )));
        }
        let mut groups: BTreeMap<u32, Vec<(u32, SessionValue, SessionValue)>> = BTreeMap::new();
        for pair in pairs {
            groups
                .entry((pair.0 >> shift) & 31)
                .or_default()
                .push(pair.clone());
        }
        let mut bitmap = 0u32;
        let mut items = Vec::new();
        for (bit, group) in groups {
            bitmap |= 1u32 << bit;
            if group.len() == 1 {
                items.push(group[0].1.clone());
                items.push(group[0].2.clone());
            } else {
                items.push(nil.clone());
                items.push(self.map_node(session, &group, shift + 5, nil)?);
            }
        }
        let bitmap = self.scalar(session, &Literal::Number(bitmap as i32 as f64))?;
        let array = session.data_array(&items.iter().collect::<Vec<_>>())?;
        session.data_construct(
            &self.roots[self.constructors[&Class::BitmapIndexedNode]],
            &[nil, &bitmap, &array],
        )
    }

    pub fn set_values(
        &self,
        session: &mut Session,
        items: &[SessionValue],
    ) -> Result<SessionValue, SessionError> {
        self.check(session)?;
        if items.is_empty() { return Ok(self.factories["empty-set"].clone()); }
        let nil = self.scalar(session, &suss_compile::portable::hir::Literal::Nil)?;
        self.set_with_metadata(session, items, &nil)
    }
    fn set_with_metadata(
        &self,
        session: &mut Session,
        items: &[SessionValue],
        metadata: &SessionValue,
    ) -> Result<SessionValue, SessionError> {
        self.check(session)?;
        let nil = self.scalar(session, &suss_compile::portable::hir::Literal::Nil)?;
        let entries = items.iter().map(|item| (item.clone(), nil.clone())).collect::<Vec<_>>();
        let map = self.map_values(session, &entries)?;
        session.data_construct(
            &self.roots[self.constructors[&Class::PersistentHashSet]],
            &[metadata, &map, &nil],
        )
    }
    pub fn vector_values(
        &self,
        session: &mut Session,
        items: &[SessionValue],
    ) -> Result<SessionValue, SessionError> {
        self.check(session)?;
        if items.len() > 65_536 {
            return Err(SessionError::Host(error("Compiler vector construction exceeds 65536 entries")));
        }
        use suss_compile::portable::hir::Literal;
        let nil = self.scalar(session, &Literal::Nil)?;
        let tail_offset = if items.is_empty() {
            0
        } else {
            ((items.len() - 1) / 32) * 32
        };
        let tail = session.data_array(&items[tail_offset..].iter().collect::<Vec<_>>())?;
        let mut shift = 5u32;
        while (tail_offset / 32) > (1usize << shift) {
            shift += 5;
        }
        let root = self.vector_node(session, &items[..tail_offset], shift, &nil)?;
        let count = self.scalar(session, &Literal::Number(items.len() as f64))?;
        let shift = self.scalar(session, &Literal::Number(shift as f64))?;
        session.data_construct(
            &self.roots[self.constructors[&Class::PersistentVector]],
            &[&nil, &count, &shift, &root, &tail, &nil],
        )
    }
    fn vector_node(
        &self,
        session: &mut Session,
        items: &[SessionValue],
        shift: u32,
        nil: &SessionValue,
    ) -> Result<SessionValue, SessionError> {
        let mut fields = vec![nil.clone(); 32];
        if shift == 0 {
            for (field, value) in fields.iter_mut().zip(items) {
                *field = value.clone();
            }
        } else {
            for (index, chunk) in items.chunks(1usize << shift).enumerate() {
                fields[index] = self.vector_node(session, chunk, shift - 5, nil)?;
            }
        }
        let array = session.data_array(&fields.iter().collect::<Vec<_>>())?;
        session.data_construct(
            &self.roots[self.constructors[&Class::VectorNode]],
            &[nil, &array],
        )
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
            include_metadata: true,
            nodes: 4096,
            units: 1_048_576,
            calls: None,
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
        let mut set_items = None;
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
            Kind::Set(items) => {
                let items = items.iter()
                    .map(|item| self.form_value(session, item, depth + 1, budget))
                    .collect::<Result<Vec<_>, _>>()?;
                if form.metadata.is_empty() {
                    self.set_values(session, &items)?
                } else {
                    // Metadata reconstruction must use the captured class root:
                    // retained -with-meta code resolves its constructor global.
                    set_items = Some(items);
                    self.scalar(session, &Literal::Nil)?
                }
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
        if let Some(items) = set_items {
            self.set_with_metadata(session, &items, &metadata)
        } else {
            let class = match form.kind {
                Kind::Symbol(_) => Class::Symbol,
                Kind::List(ref items) if items.is_empty() => Class::EmptyList,
                Kind::List(_) => Class::List,
                Kind::Vector(_) => Class::PersistentVector,
                Kind::Map(ref items) if items.len() <= 16 => Class::PersistentArrayMap,
                Kind::Map(_) => Class::PersistentHashMap,
                _ => unreachable!("metadata-bearing data checked above"),
            };
            let mut fields = session.data_fields(&value)?;
            fields[if class == Class::Symbol { 4 } else { 0 }] = metadata;
            session.data_construct(
                &self.roots[self.constructors[&class]],
                &fields.iter().collect::<Vec<_>>(),
            )
        }
    }
    /// Original result locations are not encoded in runtime values. Attribute
    /// expansion data to the supplied macro call site; never invent source bytes.
    pub fn read(
        &self,
        session: &mut Session,
        value: &SessionValue,
        span: Range<usize>,
    ) -> Result<Form, SessionError> {
        self.read_with_metadata(session, value, span, true)
    }
    pub(crate) fn read_for_display(
        &self,
        session: &mut Session,
        value: &SessionValue,
    ) -> Result<Form, SessionError> {
        self.read_with_metadata(session, value, 0..0, false)
    }
    fn read_with_metadata(
        &self,
        session: &mut Session,
        value: &SessionValue,
        span: Range<usize>,
        include_metadata: bool,
    ) -> Result<Form, SessionError> {
        self.check(session)?;
        session
            .data_inspect_calls(
                value,
                &self.factories["lazy-sval"],
                |mut store, value, args_new, invoke, function| {
                    let mut budget = Budget {
                        include_metadata,
                        nodes: 4096,
                        units: 1_048_576,
                        calls: Some(ReadCalls {
                            args_new,
                            invoke,
                            function,
                        }),
                    };
                    decode(&mut store, &value, &self.classes, &span, 0, &mut budget)
                },
            )
            .map_err(|failure| match failure {
                SessionError::Host(failure) => SessionError::Compile(Diagnostic {
                    span,
                    message: if include_metadata { format!("Invalid or unsupported compiled macro data: {failure}") } else { format!("Invalid or unsupported value display: {failure}") },
                }),
                failure => failure,
            })
    }
}
fn error(message: &str) -> wasmtime::Error {
    wasmtime::Error::msg(message.to_owned())
}

#[cfg(test)]
mod identity_tests {
    use super::*;

    #[test]
    fn macro_data_rejects_copied_descriptor_ids_after_gc() {
        for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
            let bridge = FormBridge::new(&mut session).unwrap();
            for source in ["#{1 2}", "(keys {1 10 2 20})", "'symbol", "[1 2]", "{:a 1}", "'(1 2)"] {
                let original = session.eval(source).unwrap();
                let fake = session.inspect(&original, |mut store, value| {
                    let object = reference(&value)?.as_struct(&store)?.unwrap();
                    let mut object_fields = object.fields(&mut store)?.collect::<Vec<_>>();
                    let descriptor = reference(&object_fields[0])?.as_struct(&store)?.unwrap();
                    let descriptor_fields = descriptor.fields(&mut store)?.collect::<Vec<_>>();
                    let ty = descriptor.ty(&store)?;
                    let allocator = wasmtime::StructRefPre::new(&mut store, ty);
                    let copied = wasmtime::StructRef::new(&mut store, &allocator, &descriptor_fields)?;
                    object_fields[0] = Val::AnyRef(Some(copied.to_anyref()));
                    let ty = object.ty(&store)?;
                    let allocator = wasmtime::StructRefPre::new(&mut store, ty);
                    let fake = wasmtime::StructRef::new(&mut store, &allocator, &object_fields)?;
                    fake.to_anyref().to_owned_rooted(&mut store)
                }).unwrap();
                session.collect().unwrap();
                bridge.read(&mut session, &original, 0..1).unwrap();
                let rejected = session.inspect(&original, |mut store, _| {
                    let value = Val::AnyRef(Some(fake.to_rooted(&mut store)));
                    Ok(decode(&mut store, &value, &bridge.classes, &(0..1), 0, &mut Budget { include_metadata: true, nodes: 4096, units: 1_048_576, calls: None }).is_err())
                }).unwrap();
                assert!(rejected, "copied descriptor identity accepted for {source}");
            }
        }
    }
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
    classes: &BTreeMap<i64, (Class, SessionValue)>,
    span: &Range<usize>,
    depth: usize,
    budget: &mut Budget,
) -> wasmtime::Result<Vec<Form>> {
    if !budget.include_metadata { return Ok(vec![]); }
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
    classes: &BTreeMap<i64, (Class, SessionValue)>,
) -> wasmtime::Result<(Class, Vec<Val>)> {
    let storage = fields(store, value, 4)?;
    let descriptor = fields(store, &storage[0], 5)?;
    let Val::I64(id) = descriptor[0] else {
        return Err(error("Invalid nominal descriptor identity"));
    };
    let (class, canonical) = classes.get(&id)
        .ok_or_else(|| error("Unrecognized nominal macro data type"))?;
    let expected = canonical.rooted(store);
    if !Rooted::ref_eq(&*store, reference(&storage[0])?, &expected)? {
        return Err(error("Macro data descriptor is not the captured canonical identity"));
    }
    Ok((*class, array(store, &storage[1])?))
}
fn absent(store: &StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<bool> {
    // ABI2 nil (0) and undefined (6) both satisfy the pinned nil? storage test.
    Ok(matches!(sentinel(store, value)?, Some(0 | 6)))
}
struct ReadCalls {
    args_new: wasmtime::Func,
    invoke: wasmtime::Func,
    function: Val,
}
struct Budget {
    include_metadata: bool,
    calls: Option<ReadCalls>,
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
    classes: &BTreeMap<i64, (Class, SessionValue)>,
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
                | Class::PersistentHashSet
                | Class::NodeSeq
                | Class::ArrayNodeSeq
                | Class::List
                | Class::Cons
                | Class::EmptyList
                | Class::LazySeq => Some(0),
                Class::ChunkedCons => Some(2),
                Class::IndexedSeq | Class::PersistentArrayMapSeq => Some(2),
                Class::ChunkedSeq => Some(4),
                Class::KeySeq => Some(1),
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
                Class::PersistentHashSet => {
                    if data.len() != 3 {
                        return Err(error("Invalid persistent hash set field layout"));
                    }
                    let (map_class, map_data) = object(store, &data[1], classes)?;
                    let entries = match map_class {
                        Class::PersistentHashMap => hash_map_pairs(store, &map_data, classes, budget)?,
                        Class::PersistentArrayMap => {
                            if map_data.len() != 4 { return Err(error("Invalid set array map layout")); }
                            let count = integer(store, &map_data[1])?;
                            let entries = source_elements(store, &map_data[2], classes)?;
                            if entries.len() != count * 2 || count > budget.nodes {
                                return Err(error("Set count disagrees with bounded array map storage"));
                            }
                            entries
                        }
                        _ => return Err(error("Set needs a canonical persistent backing map")),
                    };
                    Kind::Set(entries.chunks_exact(2)
                        .map(|pair| decode(store, &pair[0], classes, span, depth + 1, budget))
                        .collect::<wasmtime::Result<Vec<_>>>()?)
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
                Class::KeySeq => {
                    if data.len() != 2 { return Err(error("Invalid key sequence field layout")); }
                    Kind::List(map_sequence_pairs(store, &data[0], classes, 0, budget)?
                        .chunks_exact(2)
                        .map(|pair| decode(store, &pair[0], classes, span, depth + 1, budget))
                        .collect::<wasmtime::Result<Vec<_>>>()?)
                }
                Class::ArrayChunk => return Err(error("Array chunks are not macro syntax")),
                Class::LazySeq | Class::ChunkedCons => {
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
    classes: &BTreeMap<i64, (Class, SessionValue)>,
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
        if absent(store, &cursor)? {
            break;
        }
        spend(budget)?;
        if reference(&cursor)?.as_array(&*store)?.is_some() {
            for unit in text(store, &cursor, budget)? {
                spend(budget)?;
                items.push(Form {
                    span: span.clone(),
                    metadata: vec![],
                    kind: Kind::String(vec![unit]),
                });
            }
            break;
        }
        let (class, data) = object(store, &cursor, classes)?;
        match class {
            Class::LazySeq => {
                if data.len() != 4 {
                    return Err(error("Invalid lazy sequence field layout"));
                }
                if !first_segment {
                    metadata(store, &data[0], classes, span, depth, budget)?;
                }
                // Invoke source sval only for the captured canonical nominal type.
                // It preserves thunk retry/cache behavior; the host normalizes
                // its known sequence storage without mutable class-cell lookups.
                let calls = budget
                    .calls
                    .as_ref()
                    .ok_or_else(|| error("Lazy data needs fueled source realization"))?;
                let mut args = [Val::null_any_ref()];
                calls
                    .args_new
                    .call(&mut *store, &[Val::I32(1)], &mut args)?;
                args[0]
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&*store)?
                    .ok_or_else(|| error("Invalid invocation arguments"))?
                    .set(&mut *store, 0, cursor.clone())?;
                let mut result = [Val::null_any_ref()];
                calls.invoke.call(
                    &mut *store,
                    &[calls.function.clone(), args[0].clone()],
                    &mut result,
                )?;
                cursor = result[0].clone();
            }
            Class::ChunkedCons => {
                if data.len() != 4 {
                    return Err(error("Invalid chunked cons field layout"));
                }
                if !first_segment {
                    metadata(store, &data[2], classes, span, depth, budget)?;
                }
                let (class, chunk) = object(store, &data[0], classes)?;
                if class != Class::ArrayChunk || chunk.len() != 3 {
                    return Err(error("Chunked cons needs canonical array chunk storage"));
                }
                let entries = source_elements(store, &chunk[0], classes)?;
                let start = integer(store, &chunk[1])?;
                let end = integer(store, &chunk[2])?;
                if start >= end || end > entries.len() || end - start > budget.nodes {
                    return Err(error("Invalid bounded array chunk cursor"));
                }
                for entry in &entries[start..end] {
                    items.push(decode(store, entry, classes, span, depth + 1, budget)?);
                }
                cursor = data[1].clone();
            }
            Class::PersistentVector => {
                if !first_segment && !data.is_empty() {
                    metadata(store, &data[0], classes, span, depth, budget)?;
                }
                items.extend(vector(store, &data, classes, span, depth, budget)?);
                break;
            }
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
            Class::KeySeq => {
                if data.len() != 2 { return Err(error("Invalid key sequence field layout")); }
                if !first_segment { metadata(store, &data[1], classes, span, depth, budget)?; }
                for pair in map_sequence_pairs(store, &data[0], classes, 0, budget)?.chunks_exact(2) {
                    items.push(decode(store, &pair[0], classes, span, depth + 1, budget)?);
                }
                break;
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
    classes: &BTreeMap<i64, (Class, SessionValue)>,
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
    classes: &BTreeMap<i64, (Class, SessionValue)>,
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
    classes: &BTreeMap<i64, (Class, SessionValue)>,
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
    classes: &BTreeMap<i64, (Class, SessionValue)>,
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
    classes: &BTreeMap<i64, (Class, SessionValue)>,
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
    classes: &BTreeMap<i64, (Class, SessionValue)>,
    span: &Range<usize>,
    depth: usize,
    budget: &mut Budget,
) -> wasmtime::Result<Vec<Form>> {
    if data.len() != 6 {
        return Err(error("Invalid persistent hash map field layout"));
    }
    if integer(store, &data[1])? > budget.nodes / 2 {
        return Err(error("Hash map exceeds bounded pair storage"));
    }
    hash_map_pairs(store, data, classes, budget)?
        .iter()
        .map(|entry| decode(store, entry, classes, span, depth + 1, budget))
        .collect()
}
fn hash_map_pairs(
    store: &mut StoreContextMut<'_, ()>,
    data: &[Val],
    classes: &BTreeMap<i64, (Class, SessionValue)>,
    budget: &mut Budget,
) -> wasmtime::Result<Vec<Val>> {
    if data.len() != 6 {
        return Err(error("Invalid persistent hash map field layout"));
    }
    let count = integer(store, &data[1])?;
    if count > budget.nodes {
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
    Ok(pairs)
}
// Canonical map cursors expose keys without interpreting discarded values or
// counting their private entry vectors as source nesting. Cursor recursion has
// its own bound, independent of reader nesting.
fn map_sequence_pairs(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    classes: &BTreeMap<i64, (Class, SessionValue)>,
    cursor_depth: usize,
    budget: &mut Budget,
) -> wasmtime::Result<Vec<Val>> {
    if cursor_depth >= 64 { return Err(error("Key sequence cursor nesting exceeds 64")); }
    if nil(store, value)? { return Ok(Vec::new()); }
    spend(budget)?;
    let (class, data) = object(store, value, classes)?;
    let mut pairs = Vec::new();
    match class {
        Class::PersistentArrayMapSeq => {
            if data.len() != 3 { return Err(error("Invalid array map key cursor layout")); }
            let index = integer(store, &data[1])?;
            let entries = source_elements(store, &data[0], classes)?;
            if entries.len() % 2 != 0 || index % 2 != 0 || index >= entries.len() {
                return Err(error("Invalid array map key cursor range"));
            }
            pairs.extend_from_slice(&entries[index..]);
        }
        Class::NodeSeq | Class::ArrayNodeSeq => {
            if data.len() != 5 { return Err(error("Invalid hash key cursor layout")); }
            let nodes = source_elements(store, &data[1], classes)?;
            let index = integer(store, &data[2])?;
            let is_array = matches!(class, Class::ArrayNodeSeq);
            if index > nodes.len() || (is_array && nodes.len() != 32)
                || (!is_array && (nodes.len() % 2 != 0 || index % 2 != 0)) {
                return Err(error("Invalid hash key cursor range"));
            }
            if !absent(store, &data[3])? {
                let current = map_sequence_pairs(store, &data[3], classes, cursor_depth + 1, budget)?;
                if current.is_empty() { return Err(error("Hash key cursor retains an empty child")); }
                pairs.extend(current);
            } else if is_array || index == nodes.len() || absent(store, &nodes[index])? {
                return Err(error("Hash key cursor has no current entry"));
            }
            if is_array {
                for node in &nodes[index..] {
                    if !absent(store, node)? { hash_node(store, node, classes, 0, budget, &mut pairs)?; }
                }
            } else {
                for pair in nodes[index..].chunks_exact(2) {
                    if absent(store, &pair[0])? {
                        if !absent(store, &pair[1])? { hash_node(store, &pair[1], classes, 0, budget, &mut pairs)?; }
                    } else { pairs.extend_from_slice(pair); }
                    if pairs.len() > budget.nodes { return Err(error("Key cursor exceeds bounded pair storage")); }
                }
            }
        }
        Class::List | Class::Cons => {
            let expected = if matches!(class, Class::List) { 5 } else { 4 };
            if data.len() != expected { return Err(error("Invalid key cursor list layout")); }
            let (entry_class, entry) = object(store, &data[1], classes)?;
            if !matches!(entry_class, Class::MapEntry) || entry.len() != 3 {
                return Err(error("Key cursor list needs canonical map entries"));
            }
            pairs.extend_from_slice(&entry[..2]);
            pairs.extend(map_sequence_pairs(store, &data[2], classes, cursor_depth + 1, budget)?);
        }
        Class::EmptyList => {
            if data.len() != 1 { return Err(error("Invalid empty key cursor layout")); }
        }
        _ => return Err(error("Key sequence needs a canonical map cursor")),
    }
    if pairs.len() > budget.nodes { return Err(error("Key cursor exceeds bounded pair storage")); }
    Ok(pairs)
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
    classes: &BTreeMap<i64, (Class, SessionValue)>,
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
