//! Bounded native transport between reader forms and real compiled macro values.
//! Macro bodies execute in Wasm; this module constructs and reads nominal data.
use crate::portable::Diagnostic;
use crate::portable_session::{Session, SessionError, SessionValue};
use std::{collections::BTreeMap, ops::Range, sync::Arc};
use suss_reader::forms::{Form, Kind};
use wasmtime::{AnyRef, AsContextMut, Rooted, StoreContextMut, Val};

pub(crate) fn normalized_reader_metadata(
    session: &mut Session,
    form: &Form,
) -> Result<Vec<Form>, SessionError> {
    crate::portable::hir::reader_metadata_pairs_metered(form, &mut |work| {
        session.charge_host_work(work).map_err(|error| Diagnostic {
            span: form.span.clone(), message: error.to_string(),
        })
    }).map_err(SessionError::Compile)
}

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
// One enumeration for bridge construction and initial bootstrap authority.
const SOURCE_CLASSES: [(&str, Class); 23] = [
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
];
pub(crate) fn source_class_names() -> impl ExactSizeIterator<Item = &'static str> {
    SOURCE_CLASSES.iter().map(|(name, _)| *name)
}
fn source_class_name(class: Class) -> &'static str {
    if class == Class::SourceArray {
        return "SourceArray";
    }
    SOURCE_CLASSES
        .iter()
        .find(|(_, candidate)| *candidate == class)
        .expect("complete bridge class enumeration")
        .0
}

#[derive(Clone, Copy)]
pub(crate) enum MetadataShape {
    Symbol,
    List { empty: bool },
    Vector,
    Map { large: bool },
    Set,
}

/// Display-only data, never fabricated reader/macro syntax.
pub(crate) enum DisplayDatum {
    Plain(Form),
    Collection(DisplayCollection, Vec<DisplayDatum>),
    Opaque(DisplayLeaf),
}
#[derive(Clone, Copy)]
pub(crate) enum DisplayCollection {
    List,
    Vector,
    Map,
    Set,
}
#[derive(Clone, Copy)]
pub(crate) enum DisplayLeaf {
    Future(crate::portable_session::FutureStatus),
    StreamReader,
    StreamWriter,
    StreamEof,
}
impl DisplayLeaf {
    pub(crate) fn text(self) -> &'static str {
        use crate::portable_session::FutureStatus;
        match self {
            Self::Future(FutureStatus::Pending) => "#<future pending>",
            Self::Future(FutureStatus::Ready) => "#<future ready>",
            Self::Future(FutureStatus::Failed) => "#<future failed>",
            Self::Future(FutureStatus::Cancelled) => "#<future cancelled>",
            Self::StreamReader => "#<stream reader>",
            Self::StreamWriter => "#<stream writer>",
            Self::StreamEof => "#<stream eof>",
        }
    }
}
#[derive(Clone, Copy)]
struct DisplayCalls {
    future_is: wasmtime::Func,
    future_status: wasmtime::Func,
    stream_kind: wasmtime::Func,
}
impl DisplayCalls {
    fn read(
        self,
        store: &mut StoreContextMut<'_, ()>,
        value: Val,
    ) -> wasmtime::Result<Option<DisplayLeaf>> {
        let mut result = [Val::I32(0)];
        self.future_is.call(&mut *store, &[value], &mut result)?;
        if result[0].unwrap_i32() != 0 {
            self.future_status
                .call(&mut *store, &[value], &mut result)?;
            use crate::portable_session::FutureStatus;
            return Ok(Some(DisplayLeaf::Future(match result[0].unwrap_i32() {
                0 => FutureStatus::Pending,
                1 => FutureStatus::Ready,
                2 => FutureStatus::Failed,
                3 => FutureStatus::Cancelled,
                _ => return Err(error("Invalid nominal future status in display")),
            })));
        }
        self.stream_kind.call(&mut *store, &[value], &mut result)?;
        Ok(match result[0].unwrap_i32() {
            0 => None,
            1 => Some(DisplayLeaf::StreamReader),
            2 => Some(DisplayLeaf::StreamWriter),
            3 => Some(DisplayLeaf::StreamEof),
            _ => return Err(error("Invalid nominal stream kind in display")),
        })
    }
}
fn display_calls(session: &mut Session) -> Result<DisplayCalls, SessionError> {
    let (future_is, future_status, stream_kind) = session.display_runtime_exports()?;
    Ok(DisplayCalls {
        future_is,
        future_status,
        stream_kind,
    })
}
pub(crate) fn display_leaf(
    session: &mut Session,
    value: &SessionValue,
) -> Result<Option<DisplayLeaf>, SessionError> {
    let calls = display_calls(session)?;
    session.inspect(value, |mut store, value| calls.read(&mut store, value))
}
trait DatumTarget: Sized {
    fn plain(form: Form) -> Self;
    fn collection(
        kind: DisplayCollection,
        items: Vec<Self>,
        span: Range<usize>,
        metadata: Vec<Form>,
    ) -> Self;
    fn opaque(leaf: DisplayLeaf) -> wasmtime::Result<Self>;
}
impl DatumTarget for Form {
    fn plain(form: Form) -> Self {
        form
    }
    fn collection(
        kind: DisplayCollection,
        items: Vec<Self>,
        span: Range<usize>,
        metadata: Vec<Form>,
    ) -> Self {
        Self {
            span,
            metadata,
            kind: match kind {
                DisplayCollection::List => Kind::List(items),
                DisplayCollection::Vector => Kind::Vector(items),
                DisplayCollection::Map => Kind::Map(items),
                DisplayCollection::Set => Kind::Set(items),
            },
        }
    }
    fn opaque(_: DisplayLeaf) -> wasmtime::Result<Self> {
        Err(error("Opaque runtime values are not macro syntax"))
    }
}
impl DatumTarget for DisplayDatum {
    fn plain(form: Form) -> Self {
        Self::Plain(form)
    }
    fn collection(
        kind: DisplayCollection,
        items: Vec<Self>,
        _: Range<usize>,
        _: Vec<Form>,
    ) -> Self {
        Self::Collection(kind, items)
    }
    fn opaque(leaf: DisplayLeaf) -> wasmtime::Result<Self> {
        Ok(Self::Opaque(leaf))
    }
}
enum DecodedKind<T> {
    Plain(Kind),
    List(Vec<T>),
    Vector(Vec<T>),
    Map(Vec<T>),
    Set(Vec<T>),
}

/// Captured canonical class roots remain valid through redefinition and GC.
/// A reset or another Store invalidates this bridge; construct a new one there.
// Immutable native containers; their Wasm referents still require live checks.
struct BridgeCore {
    roots: Vec<SessionValue>,
    classes: BTreeMap<i64, (Class, SessionValue)>,
    constructors: BTreeMap<Class, usize>,
    factories: BTreeMap<&'static str, SessionValue>,
}
pub struct FormBridge {
    core: Arc<BridgeCore>,
    recording: Option<std::sync::Mutex<crate::portable_session::construction_records::ConstructionRecords>>,
    private_arguments: bool,
}
impl FormBridge {
    /// Parents containing private analysis placeholders must use only the
    /// immediate native constructor route. The caller must separately exclude
    /// private values from hash/equality arguments. This does not certify data.
    pub(crate) fn private_parent_bridge(&self, session: &mut Session) -> Result<Self, SessionError> {
        self.check(session)?;
        if self.recording.is_some() {
            return Err(SessionError::Host(error("Private parents cannot be source recordings")));
        }
        session.charge_host_work(2)?; // Arc increment and pointer copy.
        Ok(Self {
            core: self.core.clone(),
            recording: None, private_arguments: true,
        })
    }
    pub(crate) fn recording(&self, session: &mut Session) -> Result<Self, SessionError> {
        if self.private_arguments {
            return Err(SessionError::Host(error("Private parents cannot be source recordings")));
        }
        self.check(session)?;
        session.charge_host_work(2)?; // No container copies or factory recapture.
        // These are captured bridge inputs, not an authority minted here.
        let mut inputs = Vec::new();
        let count = SOURCE_CLASSES.len() + 1;
        session.charge_host_work((count * std::mem::size_of::<crate::portable_session::construction_records::BridgeClassInput>()) as u64 + 1)?;
        inputs.try_reserve_exact(count).map_err(|_| SessionError::Host(error("Bridge class input allocation failed")))?;
        for (name, class) in SOURCE_CLASSES {
            inputs.push(crate::portable_session::construction_records::BridgeClassInput {
                name,
                constructor: Some(self.core.roots[self.core.constructors[&class]].clone()),
                descriptor: self.class_descriptor(session, class)?.clone(),
            });
        }
        inputs.push(crate::portable_session::construction_records::BridgeClassInput {
            name: "SourceArray", constructor: None,
            descriptor: self.class_descriptor(session, Class::SourceArray)?.clone(),
        });
        let mut records = crate::portable_session::construction_records::ConstructionRecords::with_bridge_classes(inputs);
        let valid = records.matches_class_schemas(session)?;
        records.note_class_precheck(valid);
        // Preserve the already-captured factory values and their capture time.
        Ok(Self {
            core: self.core.clone(),
            recording: Some(std::sync::Mutex::new(records)),
            private_arguments: false,
        })
    }
    fn class_descriptor(&self, session: &mut Session, class: Class) -> Result<&SessionValue, SessionError> {
        session.charge_host_work(self.core.classes.len() as u64 + 1)?;
        self.core.classes.values().find(|(candidate, _)| *candidate == class)
            .map(|(_, descriptor)| descriptor)
            .ok_or_else(|| SessionError::Host(error("Missing captured bridge descriptor")))
    }
    fn precheck_class(&self, session: &mut Session, class: Class) -> Result<(), SessionError> {
        let Some(records) = &self.recording else { return Ok(()); };
        session.charge_host_work(SOURCE_CLASSES.len() as u64 + 1)?;
        let constructor = if class == Class::SourceArray { None } else {
            Some(&self.core.roots[self.core.constructors[&class]])
        };
        let descriptor = self.class_descriptor(session, class)?;
        // This runtime adapter only reads native constructor identity. Never hold
        // the recording lock across it, validation, or ordinary construction.
        let valid = session.initial_source_class_matches(source_class_name(class), constructor, descriptor)?;
        records.lock().map_err(|_| SessionError::Host(error("Poisoned construction records")))?
            .note_class_precheck(valid);
        // A false audit does not substitute a pristine constructor or prevent
        // ordinary transport. The final recorded-source check declines instead.
        Ok(())
    }
    fn record(
        &self,
        session: &mut Session,
        value: &SessionValue,
        fact: crate::portable_session::construction_records::Fact,
    ) -> Result<(), SessionError> {
        if let Some(records) = &self.recording {
            // Clone the domain under the lock, then perform every factory and
            // runtime check without it. The lock protects publication only.
            let domain=records.lock().map_err(|_|SessionError::Host(error("Poisoned construction records")))?.write_domain();
            domain.record(session,value,&fact)?;
            records.lock().map_err(|_|SessionError::Host(error("Poisoned construction records")))?
                .push_prepared(session,value.clone(),fact)?;
        }
        Ok(())
    }
    fn construct(
        &self,
        session: &mut Session,
        class: Class,
        arguments: &[&SessionValue],
    ) -> Result<SessionValue, SessionError> {
        self.precheck_class(session, class)?;
        let class = &self.core.roots[self.core.constructors[&class]];
        if self.private_arguments {
            return session.data_construct_without_guest_callbacks(class, arguments);
        }
        if self.recording.is_some() {
            let (value, fact) = session.data_construct_with_inputs(class, arguments)?;
            let fact = match fact {
                Some(fact) => {
                    crate::portable_session::construction_records::Fact::Constructor(fact)
                }
                None => crate::portable_session::construction_records::Fact::ExternalSeed(
                    "special constructor",
                ),
            };
            self.record(session, &value, fact)?;
            Ok(value)
        } else {
            session.data_construct(class, arguments)
        }
    }
    fn array(
        &self,
        session: &mut Session,
        values: &[&SessionValue],
    ) -> Result<SessionValue, SessionError> {
        if self.recording.is_some() {
            self.precheck_class(session, Class::SourceArray)?;
            let (value, fact) = session.data_array_with_inputs(values)?;
            self.record(
                session,
                &value,
                crate::portable_session::construction_records::Fact::Array(fact),
            )?;
            Ok(value)
        } else {
            session.data_array(values)
        }
    }
    fn seed(
        &self,
        session: &mut Session,
        name: &'static str,
    ) -> Result<SessionValue, SessionError> {
        let value = self.core.factories[name].clone();
        let fact = if self.recording.is_some() {
            match session.canonical_seed(name)? {
                Some(seed) => crate::portable_session::construction_records::Fact::CanonicalSeed(seed),
                None => crate::portable_session::construction_records::Fact::ExternalSeed(name),
            }
        } else {
            crate::portable_session::construction_records::Fact::ExternalSeed(name)
        };
        self.record(
            session,
            &value,
            fact,
        )?;
        Ok(value)
    }
    pub(crate) fn into_records(
        self,
    ) -> Result<crate::portable_session::construction_records::ConstructionRecords, SessionError>
    {
        self.recording
            .ok_or_else(|| SessionError::Host(error("Bridge is not recording")))?
            .into_inner()
            .map_err(|_| SessionError::Host(error("Poisoned construction records")))
    }
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
        self.construct(session,
            class,
            &fields.iter().collect::<Vec<_>>(),
        )
    }
    pub fn new(session: &mut Session) -> Result<Self, SessionError> {
        let mut roots = Vec::new();
        let mut classes = BTreeMap::new();
        let mut constructors = BTreeMap::new();
        for (name, class) in SOURCE_CLASSES {
            let value = session.eval(&format!("suss.core/{name}"))?;
            let id = session.inspect(&value, |mut store, value| {
                let closure = fields(&mut store, &value, 5)?;
                let owner = fields(&mut store, &closure[0], 4)?;
                let storage = array(&mut store, &owner[1])?;
                if storage.len() != 3 {
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
        if classes
            .insert(id, (Class::SourceArray, descriptor))
            .is_some()
        {
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
        session.charge_host_work((std::mem::size_of::<BridgeCore>()
            + 2 * std::mem::size_of::<usize>()) as u64)?;
        Ok(Self {
            core: Arc::new(BridgeCore { roots, classes, constructors, factories }),
            recording: None,
            private_arguments: false,
        })
    }
    fn check(&self, session: &mut Session) -> Result<(), SessionError> {
        for root in &self.core.roots {
            session.inspect(root, |_, _| Ok(()))?;
        }
        Ok(())
    }
    /// Native canonical data construction preserves sharing in analysis graphs.
    /// Inputs and factories are rooted in this Store; no source is interpreted.
    pub fn scalar(
        &self,
        session: &mut Session,
        literal: &crate::portable::hir::Literal,
    ) -> Result<SessionValue, SessionError> {
        self.check(session)?;
        if self.recording.is_some() {
            let (value,fact)=session.data_scalar_with_inputs(literal)?;
            self.record(session,&value,crate::portable_session::construction_records::Fact::Scalar(fact))?;
            Ok(value)
        } else { session.data_scalar(literal) }
    }
    pub fn map_values(
        &self,
        session: &mut Session,
        entries: &[(SessionValue, SessionValue)],
    ) -> Result<SessionValue, SessionError> {
        self.check(session)?;
        session.with_operation_budget(|session| self.map_values_inner(session, entries))
    }
    fn map_values_inner(
        &self,
        session: &mut Session,
        entries: &[(SessionValue, SessionValue)],
    ) -> Result<SessionValue, SessionError> {
        if entries.is_empty() {
            return self.seed(session,"empty-map");
        }
        // Bulk construction is original host code. Retained factories resolve
        // public class cells dynamically, so capturing their closures alone is
        // insufficient to keep compiler data canonical after core redefinition.
        let nil = self.scalar(session, &crate::portable::hir::Literal::Nil)?;
        let mut pairs: Vec<(u32, SessionValue, SessionValue)> = Vec::new();
        let mut hashes: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
        let mut nil_value = None;
        let mut nil_index = 0;
        for (key, value) in entries {
            session.charge_host_work(1)?;
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
            let hash = if entries.len() <= 8 {
                0
            } else {
                let hash = session.invoke(&self.core.factories["hash"], &[key])?;
                session.inspect(&hash, |mut store, hash| {
                    let data = fields(&mut store, &hash, 1)?;
                    let [Val::F64(bits)] = data.as_slice() else {
                        return Err(error("Compiler map hash requires a numeric scalar"));
                    };
                    let value = f64::from_bits(*bits);
                    // ECMAScript ToUint32: truncation followed by modulo 2^32,
                    // with nonfinite numbers and signed zero mapped to zero.
                    Ok(if value.is_finite() {
                        value.trunc().rem_euclid(4_294_967_296.0) as u32
                    } else {
                        0
                    })
                })?
            };
            let mut existing = None;
            for index in hashes.entry(hash).or_default().iter().copied() {
                session.charge_host_work(1)?;
                let equal = if let Some(identifier) = self.identifier_key(session, key)? {
                    self.identifier_key(session, &pairs[index].1)?
                        .is_some_and(|other| other == identifier)
                } else {
                    let equal =
                        session.invoke(&self.core.factories["key-test"], &[key, &pairs[index].1])?;
                    session.inspect(&equal, |store, value| {
                        Ok(sentinel(&store, &value)? == Some(4))
                    })?
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
            &crate::portable::hir::Literal::Number(count as f64),
        )?;
        if entries.len() <= 8 {
            let mut data = pairs
                .iter()
                .flat_map(|(_, key, value)| [key, value])
                .collect::<Vec<_>>();
            if let Some(value) = &nil_value {
                data.splice(nil_index * 2..nil_index * 2, [&nil, value]);
            }
            let array = self.array(session, &data)?;
            self.construct(session,
                Class::PersistentArrayMap,
                &[&nil, &count, &array, &nil],
            )
        } else {
            let root = self.map_node(session, &pairs, 0, &nil)?;
            let has_nil = self.scalar(
                session,
                &crate::portable::hir::Literal::Bool(nil_value.is_some()),
            )?;
            self.construct(session,
                Class::PersistentHashMap,
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
    fn identifier_key(
        &self,
        session: &mut Session,
        value: &SessionValue,
    ) -> Result<Option<(Class, Vec<u16>)>, SessionError> {
        let interrupt = session.host_work_interrupt();
        session.inspect(value, |mut store, value| {
            let Some(structure) = reference(&value)?.as_struct(&store)? else {
                return Ok(None);
            };
            let storage = structure.fields(&mut store)?.collect::<Vec<_>>();
            if storage.len() != 4 {
                return Ok(None);
            }
            let descriptor = fields(&mut store, &storage[0], 5)?;
            let Val::I64(id) = descriptor[0] else {
                return Ok(None);
            };
            if !self.core
                .classes
                .get(&id)
                .is_some_and(|(class, _)| matches!(class, Class::Symbol | Class::Keyword))
            {
                return Ok(None);
            }
            let (class, data) = object(&mut store, &value, &self.core.classes)?;
            if data.len() != if class == Class::Symbol { 5 } else { 4 } {
                return Err(error("Invalid compiler identifier key layout"));
            }
            let mut budget = Budget {
                display: None,
                source_valid: None,
                include_metadata: true,
                interrupt,
                calls: None,
                active: Vec::new(),
                        profile: std::env::var_os("SUSS_MACRO_FUEL_TRACE").is_some(),
                        host_fuel: std::cell::Cell::new(0),
                        lazy_calls: std::cell::Cell::new(0),
            };
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
        use crate::portable::hir::Literal;
        session.charge_host_work(pairs.len() as u64)?;
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
            let array = self.array(session, &items)?;
            return self.construct(session,
                Class::HashCollisionNode,
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
        let array = self.array(session, &items.iter().collect::<Vec<_>>())?;
        self.construct(session,
            Class::BitmapIndexedNode,
            &[nil, &bitmap, &array],
        )
    }

    pub fn set_values(
        &self,
        session: &mut Session,
        items: &[SessionValue],
    ) -> Result<SessionValue, SessionError> {
        self.check(session)?;
        session.with_operation_budget(|session| self.set_values_inner(session, items))
    }
    fn set_values_inner(
        &self,
        session: &mut Session,
        items: &[SessionValue],
    ) -> Result<SessionValue, SessionError> {
        session.charge_host_work(items.len() as u64)?;
        if items.is_empty() {
            return self.seed(session,"empty-set");
        }
        let nil = self.scalar(session, &crate::portable::hir::Literal::Nil)?;
        self.set_with_metadata(session, items, &nil)
    }
    fn set_with_metadata(
        &self,
        session: &mut Session,
        items: &[SessionValue],
        metadata: &SessionValue,
    ) -> Result<SessionValue, SessionError> {
        self.check(session)?;
        let nil = self.scalar(session, &crate::portable::hir::Literal::Nil)?;
        let entries = items
            .iter()
            .map(|item| (item.clone(), nil.clone()))
            .collect::<Vec<_>>();
        let map = self.map_values(session, &entries)?;
        self.construct(session,
            Class::PersistentHashSet,
            &[metadata, &map, &nil],
        )
    }
    pub fn vector_values(
        &self,
        session: &mut Session,
        items: &[SessionValue],
    ) -> Result<SessionValue, SessionError> {
        self.check(session)?;
        session.with_operation_budget(|session| self.vector_values_inner(session, items))
    }
    fn vector_values_inner(
        &self,
        session: &mut Session,
        items: &[SessionValue],
    ) -> Result<SessionValue, SessionError> {
        session.charge_host_work(items.len() as u64)?;
        use crate::portable::hir::Literal;
        let nil = self.scalar(session, &Literal::Nil)?;
        let tail_offset = if items.is_empty() {
            0
        } else {
            ((items.len() - 1) / 32) * 32
        };
        let tail = self.array(session, &items[tail_offset..].iter().collect::<Vec<_>>())?;
        let mut shift = 5u32;
        while (tail_offset / 32) > (1usize << shift) {
            shift += 5;
        }
        let root = self.vector_node(session, &items[..tail_offset], shift, &nil)?;
        let count = self.scalar(session, &Literal::Number(items.len() as f64))?;
        let shift = self.scalar(session, &Literal::Number(shift as f64))?;
        self.construct(session,
            Class::PersistentVector,
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
        session.charge_host_work(32)?;
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
        let array = self.array(session, &fields.iter().collect::<Vec<_>>())?;
        self.construct(session,
            Class::VectorNode,
            &[nil, &array],
        )
    }

    pub fn list_values(
        &self,
        session: &mut Session,
        items: &[SessionValue],
    ) -> Result<SessionValue, SessionError> {
        self.check(session)?;
        session.with_operation_budget(|session| self.list_values_inner(session, items))
    }
    fn list_values_inner(
        &self,
        session: &mut Session,
        items: &[SessionValue],
    ) -> Result<SessionValue, SessionError> {
        session.charge_host_work(items.len() as u64)?;
        let nil = self.scalar(session, &crate::portable::hir::Literal::Nil)?;
        let mut tail = self.seed(session,"empty-list")?;
        for (index, value) in items.iter().enumerate().rev() {
            let count = self.scalar(
                session,
                &crate::portable::hir::Literal::Number((items.len() - index) as f64),
            )?;
            tail = self.construct(session,
                Class::List,
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
        let nil = self.scalar(session, &crate::portable::hir::Literal::Nil)?;
        let ns = if let Some(namespace) = namespace {
            self.scalar(
                session,
                &crate::portable::hir::Literal::String(namespace.encode_utf16().collect()),
            )?
        } else {
            nil.clone()
        };
        let name_value = self.scalar(
            session,
            &crate::portable::hir::Literal::String(name.encode_utf16().collect()),
        )?;
        let fqn = namespace.map_or_else(
            || name.to_owned(),
            |namespace| format!("{namespace}/{name}"),
        );
        let fqn = self.scalar(
            session,
            &crate::portable::hir::Literal::String(fqn.encode_utf16().collect()),
        )?;
        // Reader-backed analyzer metadata starts with a cold memo in pinned
        // CLJS. Let the live IHash method perform any warming; precomputing here
        // changes nil-aware callbacks and prevents witnessing their first write.
        // Compiled constant lowering has a separate initialization contract.
        if keyword {
            if metadata.is_some() {
                return Err(SessionError::Host(error(
                    "Keyword compiler data cannot carry metadata",
                )));
            }
            self.construct(session,
                Class::Keyword,
                &[&ns, &name_value, &fqn, &nil],
            )
        } else {
            self.construct(session,
                Class::Symbol,
                &[&ns, &name_value, &fqn, &nil, metadata.unwrap_or(&nil)],
            )
        }
    }
    pub fn quote(&self, session: &mut Session, form: Form) -> Result<SessionValue, SessionError> {
        self.quote_ref(session, &form)
    }
    pub(crate) fn quote_ref(
        &self,
        session: &mut Session,
        form: &Form,
    ) -> Result<SessionValue, SessionError> {
        self.check(session)?;
        session.with_operation_budget(|session| self.form_value(session, &form))
    }
    fn form_value(&self, session: &mut Session, form: &Form) -> Result<SessionValue, SessionError> {
        use crate::portable::hir::Literal;
        enum Task<'a> {
            Visit(&'a Form),
            Finish {
                form: &'a Form,
                children: usize,
                has_metadata: bool,
            },
        }
        let failure = |form: &Form, message: &str| {
            SessionError::Compile(Diagnostic {
                span: form.span.clone(),
                message: message.into(),
            })
        };
        let mut tasks = vec![Task::Visit(form)];
        let mut values = Vec::<SessionValue>::new();
        while let Some(task) = tasks.pop() {
            match task {
                Task::Visit(form) => {
                    let work = 1u64.saturating_add(match &form.kind {
                        Kind::String(text) => text.len() as u64,
                        Kind::Symbol(name) => {
                            name.name.encode_utf16().count() as u64
                                + name
                                    .namespace
                                    .as_ref()
                                    .map_or(0, |part| part.encode_utf16().count() as u64 + 1)
                        }
                        Kind::Keyword(name) => {
                            name.name.encode_utf16().count() as u64
                                + name
                                    .namespace
                                    .as_ref()
                                    .map_or(0, |part| part.encode_utf16().count() as u64 + 1)
                        }
                        _ => 0,
                    });
                    session.charge_host_work(work)?;
                    let has_metadata = !form.metadata.is_empty();
                    let children: Vec<&Form> = match &form.kind {
                        Kind::List(items) | Kind::Vector(items) | Kind::Set(items) => {
                            items.iter().collect()
                        }
                        Kind::Map(items) => {
                            if items.len() % 2 != 0 {
                                return Err(failure(
                                    form,
                                    "Macro form map requires paired entries",
                                ));
                            }
                            items.iter().collect()
                        }
                        Kind::Conditional(_)
                        | Kind::SplicingConditional(_)
                        | Kind::Discard(_)
                        | Kind::Prefix { .. } => {
                            return Err(failure(
                                form,
                                "Native macro data requires resolved reader prefixes",
                            ));
                        }
                        _ => Vec::new(),
                    };
                    let is_collection = matches!(
                        form.kind,
                        Kind::Symbol(_)
                            | Kind::List(_)
                            | Kind::Vector(_)
                            | Kind::Map(_)
                            | Kind::Set(_)
                    );
                    if has_metadata && !is_collection {
                        return Err(failure(form, "Metadata requires a symbol or collection"));
                    }
                    tasks.push(Task::Finish {
                        form,
                        children: children.len(),
                        has_metadata,
                    });
                    for child in children.into_iter().rev() {
                        tasks.push(Task::Visit(child));
                    }
                }
                Task::Finish {
                    form,
                    children,
                    has_metadata,
                } => {
                    let take = children;
                    if values.len() < take {
                        return Err(failure(form, "Invalid macro form construction stack"));
                    }
                    let start = values.len() - take;
                    let built = values.split_off(start);
                    let mut result = match &form.kind {
                        Kind::Nil => self.scalar(session, &Literal::Nil)?,
                        Kind::Bool(value) => self.scalar(session, &Literal::Bool(*value))?,
                        Kind::Number(value) => self.scalar(session, &Literal::Number(*value))?,
                        Kind::String(value) => {
                            self.scalar(session, &Literal::String(value.clone()))?
                        }
                        Kind::Symbol(value) => self.identifier(
                            session,
                            value.namespace.as_deref(),
                            &value.name,
                            false,
                            None,
                        )?,
                        Kind::Keyword(value) => self.identifier(
                            session,
                            value.namespace.as_deref(),
                            &value.name,
                            true,
                            None,
                        )?,
                        Kind::List(_) => self.list_values(session, &built)?,
                        Kind::Vector(_) => self.vector_values(session, &built)?,
                        Kind::Map(_) => {
                            let entries = built
                                .chunks_exact(2)
                                .map(|pair| (pair[0].clone(), pair[1].clone()))
                                .collect::<Vec<_>>();
                            self.map_values(session, &entries)?
                        }
                        Kind::Set(_) if has_metadata => self.scalar(session, &Literal::Nil)?,
                        Kind::Set(_) => self.set_values(session, &built)?,
                        _ => {
                            return Err(failure(
                                form,
                                "Native macro data requires resolved reader prefixes",
                            ));
                        }
                    };
                    if has_metadata {
                        // Preserve quotation order: realize children and the
                        // base collection before evaluating its metadata map.
                        let pairs = normalized_reader_metadata(session, form)?;
                        let metadata_form = Form {
                            span: form.span.clone(),
                            metadata: vec![],
                            kind: Kind::Map(pairs),
                        };
                        let metadata = stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
                            self.form_value(session, &metadata_form)
                        })?;
                        if matches!(form.kind, Kind::Set(_)) {
                            result = self.set_with_metadata(session, &built, &metadata)?;
                        } else {
                            let class = match form.kind {
                                Kind::Symbol(_) => Class::Symbol,
                                Kind::List(ref items) if items.is_empty() => Class::EmptyList,
                                Kind::List(_) => Class::List,
                                Kind::Vector(_) => Class::PersistentVector,
                                Kind::Map(ref items) if items.len() <= 16 => {
                                    Class::PersistentArrayMap
                                }
                                Kind::Map(_) => Class::PersistentHashMap,
                                _ => unreachable!("metadata-bearing datum validated"),
                            };
                            let mut fields = session.data_fields(&result)?;
                            fields[if class == Class::Symbol { 4 } else { 0 }] = metadata;
                            result = self.construct(session,
                                class,
                                &fields.iter().collect::<Vec<_>>(),
                            )?;
                        }
                    }
                    values.push(result);
                }
            }
        }
        if values.len() != 1 {
            return Err(failure(form, "Invalid macro form construction result"));
        }
        Ok(values.pop().unwrap())
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
    /// Callback-free finite-data observation for logical source validation.
    /// Hash/UID/property storage and alias identity need separate integrity
    /// checks; a successful read is not a memo or source-storage certificate.
    pub(crate) fn read_finite(
        &self,
        session: &mut Session,
        value: &SessionValue,
    ) -> Result<(Form, bool), SessionError> {
        let _request = session.operation_budget_scope()?;
        self.check(session)?;
        let interrupt = session.host_work_interrupt();
        session.inspect(value, |mut store, value| {
            let mut budget = Budget {
                display: None,
                source_valid: Some(true),
                include_metadata: true,
                calls: None,
                interrupt,
                active: Vec::new(),
                profile: false,
                host_fuel: std::cell::Cell::new(0),
                lazy_calls: std::cell::Cell::new(0),
            };
            let form = decode(&mut store, &value, &self.core.classes, &(0..0), 0, &mut budget)?;
            Ok((form, budget.source_valid.unwrap()))
        })
    }
    pub(crate) fn read_for_display(
        &self,
        session: &mut Session,
        value: &SessionValue,
    ) -> Result<DisplayDatum, SessionError> {
        self.check(session)?;
        session.with_operation_budget(|session| {
            let display = display_calls(session)?;
            let interrupt = session.host_work_interrupt();
            session.data_inspect_calls(
                value,
                &self.core.factories["lazy-sval"],
                |mut store, value, args_new, invoke, function| {
                    let mut budget = Budget {
                        include_metadata: false,
                        display: Some(display),
                        source_valid: None,
                        interrupt,
                        active: Vec::new(),
                        profile: std::env::var_os("SUSS_MACRO_FUEL_TRACE").is_some(),
                        host_fuel: std::cell::Cell::new(0),
                        lazy_calls: std::cell::Cell::new(0),
                        calls: Some(ReadCalls {
                            args_new,
                            invoke,
                            function,
                        }),
                    };
                    decode_datum::<DisplayDatum>(
                        &mut store,
                        &value,
                        &self.core.classes,
                        &(0..0),
                        0,
                        &mut budget,
                    )
                },
            )
        })
    }
    fn read_with_metadata(
        &self,
        session: &mut Session,
        value: &SessionValue,
        span: Range<usize>,
        include_metadata: bool,
    ) -> Result<Form, SessionError> {
        self.check(session)?;
        session.with_operation_budget(|session| {
            let interrupt = session.host_work_interrupt();
            session
                .data_inspect_calls(
                    value,
                    &self.core.factories["lazy-sval"],
                    |mut store, value, args_new, invoke, function| {
                        let mut budget = Budget {
                            display: None,
                            source_valid: None,
                            include_metadata,
                            interrupt,
                            active: Vec::new(),
                        profile: std::env::var_os("SUSS_MACRO_FUEL_TRACE").is_some(),
                        host_fuel: std::cell::Cell::new(0),
                        lazy_calls: std::cell::Cell::new(0),
                            calls: Some(ReadCalls {
                                args_new,
                                invoke,
                                function,
                            }),
                        };
                        let result = decode(&mut store, &value, &self.core.classes, &span, 0, &mut budget);
                        if budget.profile {
                            eprintln!("macro-decode host-fuel={} lazy-calls={} success={}", budget.host_fuel.get(), budget.lazy_calls.get(), result.is_ok());
                        }
                        result
                    },
                )
                .map_err(|failure| match failure {
                    SessionError::Host(failure) => SessionError::Compile(Diagnostic {
                        span,
                        message: if include_metadata {
                            format!("Invalid or unsupported compiled macro data: {failure}")
                        } else {
                            format!("Invalid or unsupported value display: {failure}")
                        },
                    }),
                    failure => failure,
                })
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
    fn shared_bridge_core_preserves_captured_callbacks_recording_isolation_and_owners() {
        use crate::portable::hir::Literal;
        use std::sync::Arc;

        let mut session = Session::new_macro().unwrap();
        session.eval(r#"
          (def bridge-old-hash-calls 0)
          (def bridge-new-hash-calls 0)
          (def bridge-original-hash suss.core/hash)
        "#).unwrap();
        session.enter_namespace("suss.core").unwrap();
        session.eval(r#"
          (def hash (fn [key]
            (do (set! user/bridge-old-hash-calls (+ user/bridge-old-hash-calls 1))
                (user/bridge-original-hash key))))
        "#).unwrap();
        session.enter_namespace("user").unwrap();

        let original = FormBridge::new(&mut session).unwrap();
        let first = original.recording(&mut session).unwrap();
        let second = original.recording(&mut session).unwrap();
        let private = original.private_parent_bridge(&mut session).unwrap();
        assert!(Arc::ptr_eq(&original.core, &first.core));
        assert!(Arc::ptr_eq(&original.core, &second.core));
        assert!(Arc::ptr_eq(&original.core, &private.core));
        let first_domain = first.recording.as_ref().unwrap().lock().unwrap().write_domain();
        let second_domain = second.recording.as_ref().unwrap().lock().unwrap().write_domain();
        assert!(!Arc::ptr_eq(&first_domain, &second_domain));
        assert!(!first.private_arguments && !second.private_arguments);
        assert!(private.private_arguments && private.recording.is_none());
        assert!(first.private_parent_bridge(&mut session).is_err());
        assert!(second.private_parent_bridge(&mut session).is_err());
        assert!(private.recording(&mut session).is_err());
        drop(original);
        session.collect().unwrap();

        // Redefine the PUBLIC binding only after all captures/forks. No fork may
        // recapture it. Counter reset excludes setup/compiler activity explicitly.
        session.enter_namespace("suss.core").unwrap();
        session.eval(r#"
          (def hash (fn [key]
            (do (set! user/bridge-new-hash-calls (+ user/bridge-new-hash-calls 1))
                (user/bridge-original-hash key))))
        "#).unwrap();
        session.enter_namespace("user").unwrap();
        session.eval("(set! bridge-old-hash-calls 0) (set! bridge-new-hash-calls 0)").unwrap();

        // Every input is recorded by its own bridge. Nine elements force the
        // hash-map construction path used by ReaderSet materialization.
        fn set(bridge: &FormBridge, session: &mut Session) -> SessionValue {
            let items = (101..110).map(|number| {
                bridge.scalar(session, &Literal::Number(f64::from(number))).unwrap()
            }).collect::<Vec<_>>();
            bridge.set_values(session, &items).unwrap()
        }
        let first_set = set(&first, &mut session);
        let second_set = set(&second, &mut session);
        // Read counters before any further semantic operations can affect them.
        let counters = session.eval("[bridge-old-hash-calls bridge-new-hash-calls]").unwrap();
        assert_eq!(crate::portable_repl::display(&mut session, &counters).unwrap(), "[18 0]");

        let first_records = first.into_records().unwrap();
        let second_records = second.into_records().unwrap();
        assert!(!Arc::ptr_eq(&first_records.write_domain(), &second_records.write_domain()));
        session.collect().unwrap();
        assert!(first_records.matches_direct_facts(&mut session).unwrap());
        assert!(second_records.matches_direct_facts(&mut session).unwrap());
        assert!(first_records.matches_recorded_inputs(&mut session, &first_set).unwrap());
        assert!(second_records.matches_recorded_inputs(&mut session, &second_set).unwrap());
        assert!(first_records.matches_class_schemas(&mut session).unwrap());
        assert!(second_records.matches_class_schemas(&mut session).unwrap());
        assert!(first_records.matches_prototype_origins(&mut session).unwrap());
        assert!(second_records.matches_prototype_origins(&mut session).unwrap());

        let forge = session.eval("(fn [s] (set! (.-__hash s) 991))").unwrap();
        drop(session.invoke(&forge, &[&first_set]).unwrap());
        session.collect().unwrap();
        assert!(!first_records.matches_direct_facts(&mut session).unwrap(),
            "generic first-set memo mutation must remain unauthenticated");
        assert!(second_records.matches_direct_facts(&mut session).unwrap(),
            "sharing capture containers must not share recording facts or domains");
        let check = session.eval(r#"
          (fn [a b] [(not (identical? a b)) (= (.-__hash a) 991)
                      (nil? (.-__hash b)) (= (count a) 9) (= (count b) 9)
                      (contains? a 101) (contains? b 109)])
        "#).unwrap();
        let result = session.invoke(&check, &[&first_set, &second_set]).unwrap();
        assert_eq!(crate::portable_repl::display(&mut session, &result).unwrap(),
            "[true true true true true true true]");

        // The surviving private fork owns the captured core after every other
        // bridge wrapper has been consumed/dropped. Ownership checks stay live.
        assert!(private.scalar(&mut session, &Literal::Number(17.0)).is_ok());
        let mut foreign = Session::new_macro().unwrap();
        assert!(matches!(private.scalar(&mut foreign, &Literal::Number(17.0)),
            Err(SessionError::ForeignValue)));
        session.reset().unwrap();
        assert!(matches!(private.scalar(&mut session, &Literal::Number(17.0)),
            Err(SessionError::ForeignValue)));
    }
    #[test]
    fn private_bridge_fork_fits_finite_fuel() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.set_operation_fuel(1_000);
        let operation = session.operation_budget_scope().unwrap();
        let before = session.operation_fuel_remaining().unwrap();
        let fork = bridge.private_parent_bridge(&mut session).unwrap();
        eprintln!("private bridge fork fuel: {}", before - session.operation_fuel_remaining().unwrap());
        assert!(fork.private_arguments && fork.recording.is_none());
        drop(operation);
    }
    #[test]
    fn two_recording_bridge_forks_fit_finite_fuel() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.set_operation_fuel(30_000);
        let operation = session.operation_budget_scope().unwrap();
        let before = session.operation_fuel_remaining().unwrap();
        let first = bridge.recording(&mut session).unwrap();
        let second = bridge.recording(&mut session).unwrap();
        eprintln!("two recording bridge forks fuel: {}", before - session.operation_fuel_remaining().unwrap());
        let first = first.into_records().unwrap();
        let second = second.into_records().unwrap();
        assert!(!std::sync::Arc::ptr_eq(&first.write_domain(), &second.write_domain()));
        drop(operation);
    }
    #[test]
    fn realized_lazy_data_retains_guest_dispatch_and_metadata_checks() {
        for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
            let bridge = FormBridge::new(&mut session).unwrap();
            let cached = session.eval("(new cljs.core/LazySeq nil nil (list 42) nil)").unwrap();
            session.collect().unwrap();
            let actual = bridge.read(&mut session, &cached, 11..19).unwrap();
            assert!(matches!(&actual.kind, Kind::List(items)
                if items.len() == 1 && items[0].kind == Kind::Number(42.0)));

            session.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
            let malformed = session.eval("(new cljs.core/LazySeq nil nil (new cljs.core/List 7 42 nil 1 nil) nil)").unwrap();
            let error = bridge.read(&mut session, &malformed, 21..29).unwrap_err();
            assert!(error.to_string().contains("metadata"), "{error:?}");
            let cycle = session.eval("(new cljs.core/LazySeq nil nil nil nil)").unwrap();
            session.inspect(&cycle, |mut store, value| {
                let storage = fields(&mut store, &value, 4)?;
                let data = reference(&storage[1])?.as_array(&store)?.unwrap();
                data.set(&mut store, 2, value)?;
                Ok(())
            }).unwrap();
            assert!(bridge.read(&mut session, &cycle, 31..39).unwrap_err()
                .to_string().contains("Cyclic runtime sequence"));
        }
    }

    #[test]
    fn macro_form_quote_uses_an_explicit_stack_and_accepts_large_forms() {
        for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
            let bridge = FormBridge::new(&mut session).unwrap();
            let leaf = Form {
                span: 0..1,
                metadata: vec![],
                kind: Kind::Number(7.0),
            };
            let wide = Form {
                span: 0..1,
                metadata: vec![],
                kind: Kind::Vector(vec![leaf.clone(); 5_000]),
            };
            let mut deep = leaf.clone();
            for _ in 0..256 {
                deep = Form {
                    span: 0..1,
                    metadata: vec![],
                    kind: Kind::Vector(vec![deep]),
                };
            }
            let large_text = Form {
                span: 0..1,
                metadata: vec![],
                kind: Kind::String(vec![b'x' as u16; 1_100_000]),
            };
            let metadata = Form {
                span: 0..1,
                metadata: vec![Form {
                    span: 0..1,
                    metadata: vec![],
                    kind: Kind::Map(vec![
                        Form {
                            span: 0..1,
                            metadata: vec![],
                            kind: Kind::Keyword(suss_reader::Keyword {
                                namespace: None,
                                name: "payload".into(),
                            }),
                        },
                        large_text,
                    ]),
                }],
                kind: Kind::Vector(vec![leaf]),
            };
            let wide_value = bridge.quote_ref(&mut session, &wide).unwrap();
            let wide_result = bridge.read(&mut session, &wide_value, 0..1).unwrap();
            assert!(matches!(&wide_result.kind, Kind::Vector(items) if items.len() == 5_000));
            let deep_value = bridge.quote_ref(&mut session, &deep).unwrap();
            let mut deep_result = bridge.read(&mut session, &deep_value, 0..1).unwrap();
            for _ in 0..256 {
                let Kind::Vector(mut items) = deep_result.take_kind() else {
                    panic!("deep quoted vector did not decode as a vector");
                };
                assert_eq!(items.len(), 1);
                deep_result = items.pop().unwrap();
            }
            let metadata_value = bridge.quote_ref(&mut session, &metadata).unwrap();
            let observed = bridge.read(&mut session, &metadata_value, 0..1).unwrap();
            assert_eq!(observed.metadata.len(), 1);
            let Kind::Map(metadata_pairs) = &observed.metadata[0].kind else {
                panic!("metadata payload did not remain a map");
            };
            assert!(
                matches!(&metadata_pairs[1].kind, Kind::String(text) if text.len() == 1_100_000)
            );
            let unobserved = bridge
                .read_with_metadata(&mut session, &metadata_value, 0..1, false)
                .unwrap();
            assert!(unobserved.metadata.is_empty());
        }
    }

    #[test]
    fn macro_data_reads_sparse_array_abi_without_inventing_hole_syntax() {
        for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
            let bridge = FormBridge::new(&mut session).unwrap();
            for (source, expected) in [
                (
                    "(seq (array 1 nil 2))",
                    vec![Kind::Number(1.0), Kind::Nil, Kind::Number(2.0)],
                ),
                (
                    "[1 nil 2]",
                    vec![Kind::Number(1.0), Kind::Nil, Kind::Number(2.0)],
                ),
                (
                    "(let [ctor (type (array)) a (ctor 1000001)] (aset a 1000000 7) (IndexedSeq. a 1000000 nil))",
                    vec![Kind::Number(7.0)],
                ),
            ] {
                let value = session.eval(source).unwrap();
                session.collect().unwrap();
                let form = bridge.read(&mut session, &value, 0..1).unwrap();
                match form.into_parts().2 {
                    Kind::List(items) | Kind::Vector(items) => assert_eq!(
                        items.into_iter().map(|item| item.into_parts().2).collect::<Vec<_>>(),
                        expected
                    ),
                    _ => panic!("Expected collection for {source}"),
                }
            }
            // Forge an otherwise-valid sparse node into a dense-prefix hole.
            let owner = session.eval("(def overlap-array (array 1 nil 2))").unwrap();
            let seq = session.eval("(IndexedSeq. overlap-array 0 nil)").unwrap();
            let sparse = session
                .eval(
                    "(let [ctor (type (array)) a (ctor 33)] (set! (.-length a) 3) (aset a 0 9) a)",
                )
                .unwrap();
            let head = session
                .inspect(&sparse, |mut store, value| {
                    let object = fields(&mut store, &value, 4)?;
                    let owner = array(&mut store, &object[1])?;
                    let backing = array(&mut store, &owner[0])?;
                    reference(&backing[1])?.to_owned_rooted(&mut store)
                })
                .unwrap();
            session
                .inspect(&owner, |mut store, value| {
                    let object = fields(&mut store, &value, 4)?;
                    let owner = array(&mut store, &object[1])?;
                    let backing = array(&mut store, &owner[0])?;
                    let mask = reference(&backing[3])?.as_array(&store)?.unwrap();
                    mask.set(&mut store, 0, Val::I32(0))?;
                    let fields = reference(&owner[0])?.as_array(&store)?.unwrap();
                    let head = Val::AnyRef(Some(head.to_rooted(&mut store)));
                    fields.set(&mut store, 1, head)?;
                    Ok(())
                })
                .unwrap();
            session.collect().unwrap();
            let error = bridge
                .read(&mut session, &seq, 0..1)
                .err()
                .expect("Dense-hole sparse overlap must be rejected");
            assert!(error.to_string().contains("dense/sparse"), "{error}");
            for source in [
                "(let [ctor (type (array)) a (ctor 2)] (aset a 1 7) (IndexedSeq. a 0 nil))",
                "(let [ctor (type (array)) a (ctor 4097)] (IndexedSeq. a 0 nil))",
            ] {
                let value = session.eval(source).unwrap();
                session.collect().unwrap();
                assert!(bridge.read(&mut session, &value, 0..1).is_err(), "{source}");
            }
            let owner = session.eval("(def decoder-array (let [ctor (type (array)) a (ctor 33)] (set! (.-length a) 2) (aset a 0 1) (aset a 1 2) a))").unwrap();
            let seq = session.eval("(IndexedSeq. decoder-array 0 nil)").unwrap();
            let (node, saved_tail) = session
                .inspect(&owner, |mut store, value| {
                    let object = fields(&mut store, &value, 4)?;
                    let owner = array(&mut store, &object[1])?;
                    let backing = array(&mut store, &owner[0])?;
                    let node = reference(&backing[1])?.as_array(&store)?.unwrap();
                    let tail = node.get(&mut store, 2)?;
                    let node_value = backing[1].clone();
                    node.set(&mut store, 2, node_value.clone())?;
                    Ok((
                        reference(&node_value)?.to_owned_rooted(&mut store)?,
                        reference(&tail)?.to_owned_rooted(&mut store)?,
                    ))
                })
                .unwrap();
            session.collect().unwrap();
            assert!(bridge.read(&mut session, &seq, 0..1).is_err());
            session
                .inspect(&owner, |mut store, _| {
                    let node = node.to_rooted(&mut store).as_array(&store)?.unwrap();
                    let tail = Val::AnyRef(Some(saved_tail.to_rooted(&mut store)));
                    node.set(&mut store, 2, tail)?;
                    Ok(())
                })
                .unwrap();
            session.collect().unwrap();
            assert!(
                matches!(&bridge.read(&mut session, &seq, 0..1).unwrap().kind, Kind::List(items) if items.iter().map(|item| item.kind.clone()).collect::<Vec<_>>() == vec![Kind::Number(1.0), Kind::Number(2.0)])
            );
        }
    }

    #[test]
    fn macro_data_rejects_copied_descriptor_ids_after_gc() {
        for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
            let bridge = FormBridge::new(&mut session).unwrap();
            for source in [
                "#{1 2}",
                "(keys {1 10 2 20})",
                "'symbol",
                "[1 2]",
                "{:a 1}",
                "'(1 2)",
            ] {
                let original = session.eval(source).unwrap();
                let fake = session
                    .inspect(&original, |mut store, value| {
                        let object = reference(&value)?.as_struct(&store)?.unwrap();
                        let mut object_fields = object.fields(&mut store)?.collect::<Vec<_>>();
                        let descriptor = reference(&object_fields[0])?.as_struct(&store)?.unwrap();
                        let descriptor_fields = descriptor.fields(&mut store)?.collect::<Vec<_>>();
                        let ty = descriptor.ty(&store)?;
                        let allocator = wasmtime::StructRefPre::new(&mut store, ty);
                        let copied =
                            wasmtime::StructRef::new(&mut store, &allocator, &descriptor_fields)?;
                        object_fields[0] = Val::AnyRef(Some(copied.to_anyref()));
                        let ty = object.ty(&store)?;
                        let allocator = wasmtime::StructRefPre::new(&mut store, ty);
                        let fake =
                            wasmtime::StructRef::new(&mut store, &allocator, &object_fields)?;
                        fake.to_anyref().to_owned_rooted(&mut store)
                    })
                    .unwrap();
                session.collect().unwrap();
                bridge.read(&mut session, &original, 0..1).unwrap();
                let rejected = session
                    .inspect(&original, |mut store, _| {
                        let value = Val::AnyRef(Some(fake.to_rooted(&mut store)));
                        Ok(decode(
                            &mut store,
                            &value,
                            &bridge.core.classes,
                            &(0..1),
                            0,
                            &mut Budget {
                                display: None,
                                source_valid: None,
                                include_metadata: true,
                                interrupt: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                                    false,
                                )),
                                active: Vec::new(),
                        profile: std::env::var_os("SUSS_MACRO_FUEL_TRACE").is_some(),
                        host_fuel: std::cell::Cell::new(0),
                        lazy_calls: std::cell::Cell::new(0),
                                calls: None,
                            },
                        )
                        .is_err())
                    })
                    .unwrap();
                assert!(rejected, "copied descriptor identity accepted for {source}");
            }
        }
    }

    #[test]
    fn macro_data_rejects_recursive_collection_cycles() {
        for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
            let bridge = FormBridge::new(&mut session).unwrap();
            let vector = session.eval("[1]").unwrap();
            session
                .inspect(&vector, |mut store, root| {
                    let (_, data) = object(&mut store, &root, &bridge.core.classes)?;
                    let (_, owner) = object(&mut store, &data[4], &bridge.core.classes)?;
                    let backing = array(&mut store, &owner[0])?;
                    let dense = reference(&backing[2])?
                        .as_array(&store)?
                        .ok_or_else(|| error("Expected vector tail dense storage"))?;
                    dense.set(&mut store, 0, root.clone())?;
                    Ok(())
                })
                .unwrap();
            let error = bridge
                .read(&mut session, &vector, 0..1)
                .expect_err("recursive runtime collection must be rejected");
            assert!(
                error.to_string().contains("Cyclic runtime value"),
                "{error}"
            );
        }
    }

    #[test]
    fn macro_data_host_traversal_exhausts_the_operation_fuel() {
        let mut session = Session::new_repl().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let value = session.eval("[1]").unwrap();
        session.set_operation_fuel(1);
        let error = bridge
            .read(&mut session, &value, 0..1)
            .expect_err("decoder host work must consume operation fuel");
        assert!(error.to_string().to_lowercase().contains("fuel"), "{error}");
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
    // Reject foreign/wide layouts before rooting/copying their fields.
    if object.ty(&*store)?.fields().len() != count {
        return Err(error("Unexpected structure layout"));
    }
    Ok(object.fields(&mut *store)?.collect())
}
fn array(store: &mut StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<Vec<Val>> {
    let array = reference(value)?
        .as_array(&*store)?
        .ok_or_else(|| error("Expected field array"))?;
    // Every array read here is a descriptor owner, nominal field vector, or
    // sparse-array node. Their pinned ABI widths are at most six; source
    // elements are traversed through source_array_suffix instead.
    if array.len(&*store)? > 6 {
        return Err(error("Nominal field array exceeds ABI layout"));
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
    if !budget.include_metadata {
        return Ok(vec![]);
    }
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
    let len = array.len(&*store)?;
    charge_read_work(store, len as u64, budget)?;
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
    let (class, canonical) = classes
        .get(&id)
        .ok_or_else(|| error("Unrecognized nominal macro data type"))?;
    let expected = canonical.rooted(store);
    if !Rooted::ref_eq(&*store, reference(&storage[0])?, &expected)? {
        return Err(error(
            "Macro data descriptor is not the captured canonical identity",
        ));
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
    display: Option<DisplayCalls>,
    include_metadata: bool,
    // None for ordinary macro/display reads. Compiler source checks accumulate
    // invariant mismatches without treating them as a new source certificate.
    source_valid: Option<bool>,
    calls: Option<ReadCalls>,
    interrupt: std::sync::Arc<std::sync::atomic::AtomicBool>,
    active: Vec<Val>,
    profile: bool,
    host_fuel: std::cell::Cell<u64>,
    lazy_calls: std::cell::Cell<u64>,
}
fn unsupported_datum(budget: &Budget, name: &str) -> wasmtime::Error {
    if budget.display.is_some() {
        error(&format!("Value display does not yet support {name}"))
    } else {
        error(&format!("{name} are not macro syntax"))
    }
}
fn charge_read_work(store: &mut StoreContextMut<'_, ()>, units: u64, budget: &Budget) -> wasmtime::Result<()> {
    if !budget.profile {
        return Session::consume_host_work(store.as_context_mut(), units, &budget.interrupt);
    }
    let before = store.get_fuel()?;
    let result = Session::consume_host_work(store.as_context_mut(), units, &budget.interrupt);
    let after = store.get_fuel()?;
    budget.host_fuel.set(budget.host_fuel.get().saturating_add(before.saturating_sub(after)));
    result
}
fn spend(store: &mut StoreContextMut<'_, ()>, budget: &mut Budget) -> wasmtime::Result<()> {
    charge_read_work(store, 1, budget)
}
fn enter_active(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    budget: &mut Budget,
    cycle_message: &str,
    allow_top_alias: bool,
) -> wasmtime::Result<bool> {
    if value.anyref().flatten().is_none() {
        return Ok(false);
    }
    for (index, prior) in budget.active.iter().enumerate() {
        charge_read_work(store, 1, budget)?;
        if Rooted::ref_eq(&*store, reference(prior)?, reference(value)?)? {
            if allow_top_alias && index + 1 == budget.active.len() {
                // `decode_datum` marks a sequence root active before calling
                // `sequence` for that same value. Record this one permitted
                // wrapper alias as its own stack frame: otherwise a nested
                // self-link (for example NodeSeq.s = NodeSeq) is repeatedly
                // mistaken for the same wrapper and recurses until fuel runs
                // out instead of reporting the cycle.
                budget.active.push(value.clone());
                return Ok(true);
            }
            return Err(error(cycle_message));
        }
    }
    budget.active.push(value.clone());
    Ok(true)
}
fn leave_active(budget: &mut Budget, pushed: bool) {
    if pushed {
        budget.active.pop();
    }
}
fn decode(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    classes: &BTreeMap<i64, (Class, SessionValue)>,
    span: &Range<usize>,
    depth: usize,
    budget: &mut Budget,
) -> wasmtime::Result<Form> {
    decode_datum::<Form>(store, value, classes, span, depth, budget)
}
fn decode_datum<T: DatumTarget>(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    classes: &BTreeMap<i64, (Class, SessionValue)>,
    span: &Range<usize>,
    depth: usize,
    budget: &mut Budget,
) -> wasmtime::Result<T> {
    let pushed = enter_active(
        store,
        value,
        budget,
        "Cyclic runtime value cannot be transported as macro data",
        false,
    )?;
    let result = stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
        decode_datum_inner(store, value, classes, span, depth, budget)
    });
    leave_active(budget, pushed);
    result
}
fn decode_datum_inner<T: DatumTarget>(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    classes: &BTreeMap<i64, (Class, SessionValue)>,
    span: &Range<usize>,
    depth: usize,
    budget: &mut Budget,
) -> wasmtime::Result<T> {
    spend(store, budget)?;
    if let Some(calls) = budget.display {
        if let Some(leaf) = calls.read(store, *value)? {
            return T::opaque(leaf);
        }
    }
    let reference = reference(value)?;
    let mut reader_metadata = vec![];
    let kind = if let Some(sentinel) = sentinel(store, value)? {
        match sentinel {
            0 => DecodedKind::Plain(Kind::Nil),
            2 => DecodedKind::Plain(Kind::Bool(false)),
            4 => DecodedKind::Plain(Kind::Bool(true)),
            _ => return Err(error("Unknown language sentinel")),
        }
    } else if reference.as_array(&*store)?.is_some() {
        DecodedKind::Plain(Kind::String(text(store, value, budget)?))
    } else {
        let structure = reference
            .as_struct(&*store)?
            .ok_or_else(|| error("Unsupported macro value"))?;
        // Scalar boxes and nominal objects have fixed ABI widths. Checking
        // metadata first keeps hostile arbitrary-width structs off the copying
        // path; this logical decoder does not admit other representations.
        if !matches!(structure.ty(&*store)?.fields().len(), 1 | 4) {
            return Err(error("Unexpected structure layout"));
        }
        let values = structure.fields(&mut *store)?.collect::<Vec<_>>();
        if let [Val::F64(bits)] = values.as_slice() {
            DecodedKind::Plain(Kind::Number(f64::from_bits(*bits)))
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
                    if budget.source_valid.is_some() {
                        let fqn = text(store, &data[2], budget)?;
                        let expected = namespace.as_deref().into_iter()
                            .flat_map(|ns| ns.encode_utf16().chain(std::iter::once(b'/' as u16)))
                            .chain(name.encode_utf16());
                        let mut index = 0;
                        let mut matches = true;
                        for unit in expected {
                            charge_read_work(store, 1, budget)?;
                            if fqn.get(index) != Some(&unit) { matches = false; }
                            index += 1;
                        }
                        charge_read_work(store, 1, budget)?;
                        if index != fqn.len() { matches = false; }
                        if !matches { budget.source_valid = Some(false); }
                    }
                    if count == 5 {
                        DecodedKind::Plain(Kind::Symbol(suss_reader::Symbol { namespace, name }))
                    } else {
                        DecodedKind::Plain(Kind::Keyword(suss_reader::Keyword { namespace, name }))
                    }
                }
                Class::PersistentVector => {
                    DecodedKind::Vector(vector(store, &data, classes, span, depth, budget)?)
                }
                Class::PersistentArrayMap => {
                    if data.len() != 4 {
                        return Err(error("Invalid persistent array map field layout"));
                    }
                    let count = integer(store, &data[1])?;
                    let entries = source_elements(store, &data[2], classes, budget)?;
                    if count.checked_mul(2) != Some(entries.len()) {
                        return Err(error("Map count disagrees with pair storage"));
                    }
                    DecodedKind::Map(
                        entries
                            .iter()
                            .map(|entry| {
                                decode_datum(store, entry, classes, span, depth + 1, budget)
                            })
                            .collect::<wasmtime::Result<Vec<_>>>()?,
                    )
                }
                Class::PersistentHashMap => {
                    DecodedKind::Map(hash_map(store, &data, classes, span, depth, budget)?)
                }
                Class::PersistentHashSet => {
                    if data.len() != 3 {
                        return Err(error("Invalid persistent hash set field layout"));
                    }
                    let (map_class, map_data) = object(store, &data[1], classes)?;
                    let entries = match map_class {
                        Class::PersistentHashMap => {
                            hash_map_pairs(store, &map_data, classes, budget)?
                        }
                        Class::PersistentArrayMap => {
                            if map_data.len() != 4 {
                                return Err(error("Invalid set array map layout"));
                            }
                            let count = integer(store, &map_data[1])?;
                            let entries = source_elements(store, &map_data[2], classes, budget)?;
                            if count.checked_mul(2) != Some(entries.len()) {
                                return Err(error("Set count disagrees with array map storage"));
                            }
                            entries
                        }
                        _ => return Err(error("Set needs a canonical persistent backing map")),
                    };
                    if budget.source_valid.is_some() {
                        charge_read_work(store, 1, budget)?;
                        // The compiler's backing maps are constructed with nil
                        // metadata, independently of the enclosing set metadata.
                        if sentinel(store, &map_data[0])? != Some(0) {
                            budget.source_valid = Some(false);
                        }
                    }
                    DecodedKind::Set(
                        entries
                            .chunks_exact(2)
                            .map(|pair| {
                                if budget.source_valid.is_some() {
                                    charge_read_work(store, 1, budget)?;
                                    // Native source set construction stores nil
                                    // values; logical set decoding otherwise
                                    // visits only the keys of its backing map.
                                    if sentinel(store, &pair[1])? != Some(0) {
                                        budget.source_valid = Some(false);
                                    }
                                }
                                decode_datum(store, &pair[0], classes, span, depth + 1, budget)
                            })
                            .collect::<wasmtime::Result<Vec<_>>>()?,
                    )
                }
                Class::BitmapIndexedNode | Class::ArrayNode | Class::HashCollisionNode => {
                    return Err(unsupported_datum(budget, "Hash trie nodes"));
                }
                Class::MapEntry => {
                    if data.len() != 3 {
                        return Err(error("Invalid map entry field layout"));
                    }
                    DecodedKind::Vector(
                        data[..2]
                            .iter()
                            .map(|entry| {
                                decode_datum(store, entry, classes, span, depth + 1, budget)
                            })
                            .collect::<wasmtime::Result<Vec<_>>>()?,
                    )
                }
                Class::PersistentArrayMapSeq
                | Class::ChunkedSeq
                | Class::NodeSeq
                | Class::ArrayNodeSeq => {
                    DecodedKind::List(sequence(store, value, classes, span, depth, budget)?)
                }
                Class::KeySeq => {
                    if data.len() != 2 {
                        return Err(error("Invalid key sequence field layout"));
                    }
                    DecodedKind::List(
                        map_sequence_pairs(store, &data[0], classes, 0, budget)?
                            .chunks_exact(2)
                            .map(|pair| {
                                decode_datum(store, &pair[0], classes, span, depth + 1, budget)
                            })
                            .collect::<wasmtime::Result<Vec<_>>>()?,
                    )
                }
                Class::ArrayChunk => return Err(unsupported_datum(budget, "Array chunks")),
                Class::LazySeq | Class::ChunkedCons => {
                    DecodedKind::List(sequence(store, value, classes, span, depth, budget)?)
                }
                Class::VectorNode => return Err(unsupported_datum(budget, "Vector trie nodes")),
                Class::SourceArray => return Err(unsupported_datum(budget, "Raw source arrays")),
                Class::List | Class::Cons | Class::EmptyList | Class::IndexedSeq => {
                    DecodedKind::List(sequence(store, value, classes, span, depth, budget)?)
                }
            }
        }
    };
    Ok(match kind {
        DecodedKind::Plain(kind) => T::plain(Form {
            span: span.clone(),
            metadata: reader_metadata,
            kind,
        }),
        DecodedKind::List(items) => T::collection(
            DisplayCollection::List,
            items,
            span.clone(),
            reader_metadata,
        ),
        DecodedKind::Vector(items) => T::collection(
            DisplayCollection::Vector,
            items,
            span.clone(),
            reader_metadata,
        ),
        DecodedKind::Map(items) => {
            T::collection(DisplayCollection::Map, items, span.clone(), reader_metadata)
        }
        DecodedKind::Set(items) => {
            T::collection(DisplayCollection::Set, items, span.clone(), reader_metadata)
        }
    })
}
fn sequence<T: DatumTarget>(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    classes: &BTreeMap<i64, (Class, SessionValue)>,
    span: &Range<usize>,
    depth: usize,
    budget: &mut Budget,
) -> wasmtime::Result<Vec<T>> {
    let pushed = enter_active(
        store,
        value,
        budget,
        "Cyclic runtime sequence cannot be transported as macro data",
        true,
    )?;
    let result = stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
        sequence_inner(store, value, classes, span, depth, budget)
    });
    leave_active(budget, pushed);
    result
}
fn sequence_inner<T: DatumTarget>(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    classes: &BTreeMap<i64, (Class, SessionValue)>,
    span: &Range<usize>,
    depth: usize,
    budget: &mut Budget,
) -> wasmtime::Result<Vec<T>> {
    let mut cursor = value.clone();
    let mut items = Vec::new();
    let mut counts = Vec::new();
    let mut cursor_roots = Vec::<Val>::new();
    let mut first_segment = true;
    loop {
        if absent(store, &cursor)? {
            break;
        }
        for prior in &cursor_roots {
            charge_read_work(store, 1, budget)?;
            if Rooted::ref_eq(&*store, reference(prior)?, reference(&cursor)?)? {
                return Err(error(
                    "Cyclic runtime sequence cannot be transported as macro data",
                ));
            }
        }
        cursor_roots.push(cursor.clone());
        spend(store, budget)?;
        if reference(&cursor)?.as_array(&*store)?.is_some() {
            for unit in text(store, &cursor, budget)? {
                spend(store, budget)?;
                items.push(T::plain(Form {
                    span: span.clone(),
                    metadata: vec![],
                    kind: Kind::String(vec![unit]),
                }));
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
                budget.lazy_calls.set(budget.lazy_calls.get().saturating_add(1));
                // Observational reads never replace an invocation error. This
                // delta includes callback-triggered host charges, not just Wasm.
                let fuel_before = if budget.profile { store.get_fuel().ok() } else { None };
                if budget.profile {
                    eprintln!("macro-lazy-invoke call={} stage=before fuel={fuel_before:?}",
                        budget.lazy_calls.get());
                }
                let invoked = calls.invoke.call(
                    &mut *store,
                    &[calls.function.clone(), args[0].clone()],
                    &mut result,
                );
                if budget.profile {
                    let fuel_after = store.get_fuel().ok();
                    let delta = fuel_before.zip(fuel_after)
                        .and_then(|(before, after)| before.checked_sub(after));
                    eprintln!("macro-lazy-invoke call={} stage=after fuel={fuel_after:?} delta={delta:?} success={}",
                        budget.lazy_calls.get(), invoked.is_ok());
                }
                invoked?;
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
                let entries = source_elements(store, &chunk[0], classes, budget)?;
                let start = integer(store, &chunk[1])?;
                let end = integer(store, &chunk[2])?;
                if start >= end || end > entries.len() {
                    return Err(error("Invalid array chunk cursor"));
                }
                for entry in &entries[start..end] {
                    items.push(decode_datum(
                        store,
                        entry,
                        classes,
                        span,
                        depth + 1,
                        budget,
                    )?);
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
                items.push(decode_datum(
                    store,
                    &data[1],
                    classes,
                    span,
                    depth + 1,
                    budget,
                )?);
                cursor = data[2].clone();
            }
            Class::KeySeq => {
                if data.len() != 2 {
                    return Err(error("Invalid key sequence field layout"));
                }
                if !first_segment {
                    metadata(store, &data[1], classes, span, depth, budget)?;
                }
                for pair in map_sequence_pairs(store, &data[0], classes, 0, budget)?.chunks_exact(2)
                {
                    items.push(decode_datum(
                        store,
                        &pair[0],
                        classes,
                        span,
                        depth + 1,
                        budget,
                    )?);
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
                let nodes = source_elements(store, &data[1], classes, budget)?;
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
                    }
                }
                for pair in pairs.chunks_exact(2) {
                    spend(store, budget)?;
                    let pair = pair
                        .iter()
                        .map(|entry| decode_datum(store, entry, classes, span, depth + 2, budget))
                        .collect::<wasmtime::Result<Vec<_>>>()?;
                    items.push(T::collection(
                        DisplayCollection::Vector,
                        pair,
                        span.clone(),
                        vec![],
                    ));
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
                let entries = source_elements(store, &data[0], classes, budget)?;
                if entries.len() % 2 != 0 || index % 2 != 0 || index >= entries.len() {
                    return Err(error("Invalid array map sequence pair storage"));
                }
                for pair in entries[index..].chunks_exact(2) {
                    spend(store, budget)?;
                    let pair = pair
                        .iter()
                        // The generated entry vector is one syntax level below
                        // this sequence; its key/value children are another.
                        .map(|entry| decode_datum(store, entry, classes, span, depth + 2, budget))
                        .collect::<wasmtime::Result<Vec<_>>>()?;
                    items.push(T::collection(
                        DisplayCollection::Vector,
                        pair,
                        span.clone(),
                        vec![],
                    ));
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
                let mut node = source_elements(store, &data[1], classes, budget)?;
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
                        items.push(decode_datum(
                            store,
                            entry,
                            classes,
                            span,
                            depth + 1,
                            budget,
                        )?);
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

fn append_indexed<T: DatumTarget>(
    store: &mut StoreContextMut<'_, ()>,
    data: &[Val],
    classes: &BTreeMap<i64, (Class, SessionValue)>,
    span: &Range<usize>,
    depth: usize,
    budget: &mut Budget,
    items: &mut Vec<T>,
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
    if reference(&data[0])?.as_array(&*store)?.is_none() {
        if index > f64::from(u32::MAX) {
            return Err(error("IndexedSeq index exceeds source array storage"));
        }
        let values = source_array_suffix(store, &data[0], classes, index as u32, budget)?;
        for value in values {
            items.push(decode_datum(
                store,
                &value,
                classes,
                span,
                depth + 1,
                budget,
            )?);
        }
        return Ok(());
    }
    let backing = reference(&data[0])?.as_array(&*store)?.unwrap();
    if !matches!(
        backing.ty(&*store)?.element_type(),
        wasmtime::StorageType::I16
    ) {
        return Err(error("IndexedSeq string needs UTF16 storage"));
    }
    let length = backing.len(&*store)?;
    if index > f64::from(length) {
        return Err(error("IndexedSeq index exceeds backing storage"));
    }
    let index = index as u32;
    charge_read_work(store, u64::from(length - index), budget)?;
    for offset in index..length {
        let value = backing.get(&mut *store, offset)?;
        spend(store, budget)?;
        let Val::I32(unit) = value else {
            return Err(error("Invalid UTF16 indexed unit"));
        };
        items.push(T::plain(Form {
            span: span.clone(),
            metadata: vec![],
            kind: Kind::String(vec![unit as u16]),
        }));
    }
    Ok(())
}

fn integer(store: &mut StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<usize> {
    let data = fields(store, value, 1)?;
    let [Val::F64(bits)] = data.as_slice() else {
        return Err(error("Invalid vector integer field"));
    };
    let n = f64::from_bits(*bits);
    if !n.is_finite() || n < 0.0 || n.fract() != 0.0 || n > f64::from(u32::MAX) {
        return Err(error("Vector integer field exceeds ABI range"));
    }
    Ok(n as usize)
}
fn source_elements(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    classes: &BTreeMap<i64, (Class, SessionValue)>,
    budget: &mut Budget,
) -> wasmtime::Result<Vec<Val>> {
    source_array_suffix(store, value, classes, 0, budget)
}

// Read the current sparse source-array ABI directly. Never execute guest code or
// invent nil for an absent index; undefined remains invalid macro syntax.
fn source_array_suffix(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    classes: &BTreeMap<i64, (Class, SessionValue)>,
    start: u32,
    budget: &mut Budget,
) -> wasmtime::Result<Vec<Val>> {
    fn uint32(store: &mut StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<u32> {
        let data = fields(store, value, 1)?;
        let [Val::F64(bits)] = data.as_slice() else {
            return Err(error("Invalid source array uint32"));
        };
        let n = f64::from_bits(*bits);
        if !n.is_finite() || n < 0.0 || n > f64::from(u32::MAX) || n.fract() != 0.0 {
            return Err(error("Invalid source array uint32"));
        }
        Ok(n as u32)
    }
    let (class, owner) = object(store, value, classes)?;
    if !matches!(class, Class::SourceArray) || owner.len() != 2 {
        return Err(error("Expected canonical sparse source array owner"));
    }
    let backing = array(store, &owner[0])?;
    if backing.len() != 4 {
        return Err(error("Invalid source array backing layout"));
    }
    let length = uint32(store, &backing[0])?;
    if start > length {
        return Err(error("Source array index exceeds logical length"));
    }
    let count = (length - start) as usize;
    charge_read_work(store, count as u64, budget)?;
    let dense = reference(&backing[2])?
        .as_array(&*store)?
        .ok_or_else(|| error("Invalid dense source array storage"))?;
    let mask = reference(&backing[3])?
        .as_array(&*store)?
        .ok_or_else(|| error("Invalid source array presence mask"))?;
    if !matches!(
        dense.ty(&*store)?.element_type(),
        wasmtime::StorageType::ValType(wasmtime::ValType::Ref(_))
    ) || !matches!(mask.ty(&*store)?.element_type(), wasmtime::StorageType::I16)
        || dense.len(&*store)? != mask.len(&*store)?
        || dense.len(&*store)? > length
    {
        return Err(error("Invalid source array dense/presence layout"));
    }
    let undefined = Val::AnyRef(Some(AnyRef::from_i31(
        &mut *store,
        wasmtime::I31::wrapping_u32(6),
    )));
    let mask_len = mask.len(&*store)?;
    charge_read_work(store, mask_len as u64, budget)?;
    let mut result = vec![undefined; count];
    for index in 0..mask_len {
        let flag = mask.get(&mut *store, index)?.unwrap_i32();
        if !(0..=1).contains(&flag) {
            return Err(error("Invalid source array presence bit"));
        }
        if flag == 1 && index >= start {
            result[(index - start) as usize] = dense.get(&mut *store, index)?;
        }
    }
    let mut next = backing[1].clone();
    let mut seen: Vec<Val> = Vec::new();
    let mut previous = None;
    while !nil(store, &next)? {
        spend(store, budget)?;
        for prior in &seen {
            charge_read_work(store, 1, budget)?;
            if Rooted::ref_eq(&*store, reference(prior)?, reference(&next)?)? {
                return Err(error("Cyclic source array sparse chain"));
            }
        }
        seen.push(next.clone());
        let node = array(store, &next)?;
        if node.len() != 3 {
            return Err(error("Invalid source array sparse node"));
        }
        let key = uint32(store, &node[0])?;
        if previous.is_some_and(|previous| key <= previous) || (key != u32::MAX && key >= length) {
            return Err(error("Invalid source array sparse key order/range"));
        }
        if key < mask.len(&*store)? {
            return Err(error("Duplicate dense/sparse source array key"));
        }
        if key != u32::MAX && key >= start {
            result[(key - start) as usize] = node[1].clone();
        }
        previous = Some(key);
        next = node[2].clone();
    }
    Ok(result)
}

fn node_elements(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    classes: &BTreeMap<i64, (Class, SessionValue)>,
    budget: &mut Budget,
) -> wasmtime::Result<Vec<Val>> {
    spend(store, budget)?;
    let (class, data) = object(store, value, classes)?;
    if !matches!(class, Class::VectorNode) || data.len() != 2 {
        return Err(error("Invalid persistent vector trie node"));
    }
    let items = source_elements(store, &data[1], classes, budget)?;
    if items.len() != 32 {
        return Err(error("Invalid vector trie array width"));
    }
    Ok(items)
}
fn vector<T: DatumTarget>(
    store: &mut StoreContextMut<'_, ()>,
    data: &[Val],
    classes: &BTreeMap<i64, (Class, SessionValue)>,
    span: &Range<usize>,
    depth: usize,
    budget: &mut Budget,
) -> wasmtime::Result<Vec<T>> {
    if data.len() != 6 {
        return Err(error("Invalid persistent vector layout"));
    }
    let count = integer(store, &data[1])?;
    let shift = integer(store, &data[2])?;
    if shift < 5 || shift > 30 || shift % 5 != 0 {
        return Err(error("Invalid vector trie shift"));
    }
    let tail = source_elements(store, &data[4], classes, budget)?;
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
            items.push(decode_datum(
                store,
                &value,
                classes,
                span,
                depth + 1,
                budget,
            )?);
        }
    }
    for value in tail {
        items.push(decode_datum(
            store,
            &value,
            classes,
            span,
            depth + 1,
            budget,
        )?);
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
        let tail = source_elements(store, &data[4], classes, budget)?;
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
fn hash_map<T: DatumTarget>(
    store: &mut StoreContextMut<'_, ()>,
    data: &[Val],
    classes: &BTreeMap<i64, (Class, SessionValue)>,
    span: &Range<usize>,
    depth: usize,
    budget: &mut Budget,
) -> wasmtime::Result<Vec<T>> {
    if data.len() != 6 {
        return Err(error("Invalid persistent hash map field layout"));
    }
    hash_map_pairs(store, data, classes, budget)?
        .iter()
        .map(|entry| decode_datum(store, entry, classes, span, depth + 1, budget))
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
    if count.checked_mul(2) != Some(pairs.len()) {
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
    let pushed = enter_active(
        store,
        value,
        budget,
        "Cyclic map key cursor cannot be transported as macro data",
        false,
    )?;
    let result = stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
        map_sequence_pairs_inner(store, value, classes, cursor_depth, budget)
    });
    leave_active(budget, pushed);
    result
}
fn map_sequence_pairs_inner(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    classes: &BTreeMap<i64, (Class, SessionValue)>,
    cursor_depth: usize,
    budget: &mut Budget,
) -> wasmtime::Result<Vec<Val>> {
    if nil(store, value)? {
        return Ok(Vec::new());
    }
    spend(store, budget)?;
    let (class, data) = object(store, value, classes)?;
    let mut pairs = Vec::new();
    match class {
        Class::PersistentArrayMapSeq => {
            if data.len() != 3 {
                return Err(error("Invalid array map key cursor layout"));
            }
            let index = integer(store, &data[1])?;
            let entries = source_elements(store, &data[0], classes, budget)?;
            if entries.len() % 2 != 0 || index % 2 != 0 || index >= entries.len() {
                return Err(error("Invalid array map key cursor range"));
            }
            pairs.extend_from_slice(&entries[index..]);
        }
        Class::NodeSeq | Class::ArrayNodeSeq => {
            if data.len() != 5 {
                return Err(error("Invalid hash key cursor layout"));
            }
            let nodes = source_elements(store, &data[1], classes, budget)?;
            let index = integer(store, &data[2])?;
            let is_array = matches!(class, Class::ArrayNodeSeq);
            if index > nodes.len()
                || (is_array && nodes.len() != 32)
                || (!is_array && (nodes.len() % 2 != 0 || index % 2 != 0))
            {
                return Err(error("Invalid hash key cursor range"));
            }
            if !absent(store, &data[3])? {
                let current =
                    map_sequence_pairs(store, &data[3], classes, cursor_depth + 1, budget)?;
                if current.is_empty() {
                    return Err(error("Hash key cursor retains an empty child"));
                }
                pairs.extend(current);
            } else if is_array || index == nodes.len() || absent(store, &nodes[index])? {
                return Err(error("Hash key cursor has no current entry"));
            }
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
                }
            }
        }
        Class::List | Class::Cons => {
            let expected = if matches!(class, Class::List) { 5 } else { 4 };
            if data.len() != expected {
                return Err(error("Invalid key cursor list layout"));
            }
            let (entry_class, entry) = object(store, &data[1], classes)?;
            if !matches!(entry_class, Class::MapEntry) || entry.len() != 3 {
                return Err(error("Key cursor list needs canonical map entries"));
            }
            pairs.extend_from_slice(&entry[..2]);
            pairs.extend(map_sequence_pairs(
                store,
                &data[2],
                classes,
                cursor_depth + 1,
                budget,
            )?);
        }
        Class::EmptyList => {
            if data.len() != 1 {
                return Err(error("Invalid empty key cursor layout"));
            }
        }
        _ => return Err(error("Key sequence needs a canonical map cursor")),
    }
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
    spend(store, budget)?;
    let (class, data) = object(store, value, classes)?;
    match class {
        Class::BitmapIndexedNode => {
            if data.len() != 3 {
                return Err(error("Invalid bitmap node field layout"));
            }
            let count = bitmap(store, &data[1])?.count_ones() as usize;
            let entries = source_elements(store, &data[2], classes, budget)?;
            let Some(occupied_len) = count.checked_mul(2) else {
                return Err(error("Bitmap population overflows pair storage"));
            };
            if entries.len() < occupied_len || entries.len() % 2 != 0 {
                return Err(error("Bitmap population exceeds pair storage"));
            }
            for pair in entries[..occupied_len].chunks_exact(2) {
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
            let entries = source_elements(store, &data[2], classes, budget)?;
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
            let entries = source_elements(store, &data[3], classes, budget)?;
            let Some(occupied_len) = count.checked_mul(2) else {
                return Err(error("Collision count overflows pair storage"));
            };
            if count == 0 || entries.len() < occupied_len || entries.len() % 2 != 0 {
                return Err(error("Collision count exceeds pair storage"));
            }
            for pair in entries[..occupied_len].chunks_exact(2) {
                if absent(store, &pair[0])? {
                    return Err(error("Nil key in hash collision node"));
                }
                pairs.extend_from_slice(pair);
            }
        }
        _ => return Err(error("Hash map needs canonical trie nodes")),
    }
    Ok(())
}
