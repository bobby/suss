//! Independent ABI2 observations. Never call the guest printer or equality.
//! Inspect canonical storage directly; unknown objects and malformed layouts fail.
use suss_compile::portable_session::{Session, SessionError, SessionValue};
use wasmtime::{AnyRef, OwnedRooted, Rooted, StorageType, StoreContextMut, Val};

#[path = "probe_eval.rs"]
mod probe_eval;
#[allow(unused_imports)]
pub use probe_eval::ProbeEval;

#[derive(Debug, PartialEq)]
pub enum Observation {
    Nil,
    Bool(bool),
    Number(u64),
    String(Vec<u16>),
    Keyword(Option<Vec<u16>>, Vec<u16>),
    Symbol(Option<Vec<u16>>, Vec<u16>),
    Vector(Vec<Observation>),
    List(Vec<Observation>),
    Map(Vec<(Observation, Observation)>),
    Set(Vec<Observation>),
    ExceptionInfo {
        message: Box<Observation>,
        data: Box<Observation>,
        cause: Box<Observation>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Keyword,
    Symbol,
    Vector,
    Subvec,
    RSeq,
    ValSeq,
    Node,
    SourceArray,
    List,
    EmptyList,
    Cons,
    IndexedSeq,
    ChunkedSeq,
    MapEntry,
    ArrayMap,
    HashMap,
    HashSet,
    BitmapNode,
    ArrayNode,
    CollisionNode,
    ExceptionInfo,
    LazySeq,
    ChunkedCons,
    ArrayChunk,
    IntegerRange,
    Range,
    ArrayMapSeq,
    KeySeq,
    NodeSeq,
    ArrayNodeSeq,
    TreeSeq,
    BlackNode,
    RedNode,
}

struct Canonical {
    anchor: SessionValue,
    descriptor: OwnedRooted<AnyRef>,
    owner_type: wasmtime::StructType,
    class: Class,
}

pub struct Decoder {
    remaining: usize,
    canonical: Vec<Canonical>,
}

impl Decoder {
    /// Reset the node budget, e.g. per independently decoded test run.
    #[allow(dead_code)]
    pub fn refill(&mut self, remaining: usize) {
        self.remaining = remaining;
    }
    pub fn new(remaining: usize) -> Self {
        Self {
            remaining,
            canonical: Vec::new(),
        }
    }

    /// Capture descriptors from known core instances before user fixtures.
    /// Class closures can wrap their environment for function properties; a
    /// nominal instance carries its descriptor directly in object field0.
    pub fn capture(session: &mut Session, remaining: usize) -> Result<Self, SessionError> {
        let mut decoder = Self::new(remaining);
        for (source, class) in [
            (
                r#"(new suss.core/Keyword nil "decoder" "decoder" nil)"#,
                Class::Keyword,
            ),
            (
                r#"(new suss.core/Symbol nil "decoder" "decoder" 0 nil)"#,
                Class::Symbol,
            ),
            ("[]", Class::Vector),
            (
                "(new suss.core/VectorNode nil (suss.core/make-array 32))",
                Class::Node,
            ),
            ("(suss.core/array)", Class::SourceArray),
            ("(new suss.core/List nil 1 nil 1 nil)", Class::List),
            ("(new suss.core/EmptyList nil)", Class::EmptyList),
            ("(new suss.core/Cons nil 1 nil nil)", Class::Cons),
            (
                "(new suss.core/IndexedSeq (suss.core/array) 0 nil)",
                Class::IndexedSeq,
            ),
            (
                "(new suss.core/ChunkedSeq [] (suss.core/array) 0 0 nil nil)",
                Class::ChunkedSeq,
            ),
            ("(new suss.core/MapEntry nil nil nil)", Class::MapEntry),
            (
                "(new suss.core/PersistentArrayMap nil 0 (suss.core/array) nil)",
                Class::ArrayMap,
            ),
            (
                "(new suss.core/PersistentHashMap nil 0 nil false nil nil)",
                Class::HashMap,
            ),
            (
                "(new suss.core/PersistentHashSet nil (suss.core/hash-map) nil)",
                Class::HashSet,
            ),
            (
                "(new suss.core/BitmapIndexedNode nil 0 (suss.core/array))",
                Class::BitmapNode,
            ),
            (
                "(new suss.core/ArrayNode nil 0 (suss.core/make-array 32))",
                Class::ArrayNode,
            ),
            (
                "(new suss.core/HashCollisionNode nil 0 0 (suss.core/array))",
                Class::CollisionNode,
            ),
            (
                "(new suss.core/ExceptionInfo \"decoder\" nil nil)",
                Class::ExceptionInfo,
            ),
        ] {
            decoder.capture_class(session, source, class)?;
        }
        Ok(decoder)
    }

    /// Optional conformance sequence layouts. Reading their stored state must
    /// never call a lazy thunk, a protocol method, or the guest's seq/equality.
    #[allow(dead_code)]
    pub fn capture_sequences(&mut self, session: &mut Session) -> Result<(), SessionError> {
        for (source, class) in [
            ("(new suss.core/LazySeq nil nil nil nil)", Class::LazySeq),
            (
                "(new suss.core/ChunkedCons nil nil nil nil)",
                Class::ChunkedCons,
            ),
            (
                "(new suss.core/ArrayChunk (suss.core/array) 0 0)",
                Class::ArrayChunk,
            ),
            (
                "(new suss.core/IntegerRange nil 0 1 1 1 nil)",
                Class::IntegerRange,
            ),
            ("(new suss.core/Range nil 0 1 1 nil nil nil)", Class::Range),
            (
                "(new suss.core/PersistentArrayMapSeq (suss.core/array) 0 nil)",
                Class::ArrayMapSeq,
            ),
            ("(new suss.core/KeySeq nil nil)", Class::KeySeq),
            ("(new suss.core/ValSeq nil nil)", Class::ValSeq),
            ("(new suss.core/RSeq [] 0 nil)", Class::RSeq),
            ("(new suss.core/Subvec nil [] 0 0 nil)", Class::Subvec),
            (
                "(new suss.core/NodeSeq nil (suss.core/array) 0 nil nil)",
                Class::NodeSeq,
            ),
            (
                "(new suss.core/ArrayNodeSeq nil (suss.core/array) 0 nil nil)",
                Class::ArrayNodeSeq,
            ),
            (
                "(new suss.core/PersistentTreeMapSeq nil nil true 0 nil)",
                Class::TreeSeq,
            ),
            (
                "(new suss.core/BlackNode nil nil nil nil nil)",
                Class::BlackNode,
            ),
            (
                "(new suss.core/RedNode nil nil nil nil nil)",
                Class::RedNode,
            ),
        ] {
            self.capture_class(session, source, class)?;
        }
        Ok(())
    }

    fn capture_class(
        &mut self,
        session: &mut Session,
        source: &str,
        class: Class,
    ) -> Result<(), SessionError> {
        let anchor = session.eval_probe(source)?;
        let (descriptor, owner_type) = session.inspect(&anchor, |mut store, value| {
            let owner_type = non_null_ref(&value)?
                .as_struct(&store)?
                .ok_or_else(|| wasmtime::Error::msg("Invalid canonical object storage"))?
                .ty(&store)?;
            let object = fields(&mut store, &value, 4)?;
            let descriptor = &object[0];
            let storage = fields(&mut store, descriptor, 6)?;
            if !matches!(storage[0], Val::I64(_)) {
                return Err(wasmtime::Error::msg(
                    "Invalid canonical descriptor identity",
                ));
            }
            Ok((
                non_null_ref(descriptor)?.to_owned_rooted(&mut store)?,
                owner_type,
            ))
        })?;
        self.canonical.push(Canonical {
            anchor,
            descriptor,
            owner_type,
            class,
        });
        Ok(())
    }

    pub fn decode_session(
        &mut self,
        session: &mut Session,
        value: &SessionValue,
    ) -> Result<Observation, SessionError> {
        // Check ownership/reset through the retained instance roots before using
        // descriptor handles. Numeric nominal IDs alone do not identify a Store.
        for canonical in &self.canonical {
            session.inspect(&canonical.anchor, |_, _| Ok(()))?;
        }
        session.inspect(value, |mut store, value| self.value(&mut store, &value, 0))
    }

    pub fn decode(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        value: &Val,
    ) -> wasmtime::Result<Observation> {
        if !self.canonical.is_empty() {
            return Err(wasmtime::Error::msg(
                "Captured decoder requires decode_session ownership check",
            ));
        }
        self.value(store, value, 0)
    }

    /// Assertion-time observer path: the hook and canonical anchors belong to
    /// the same live Store. No Session reset, GC, guest call or lazy forcing here.
    pub fn snapshot_payload(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        value: &Val,
    ) -> wasmtime::Result<Vec<Result<Observation, String>>> {
        let (class, data) = self.object(store, value)?;
        if class != Class::Vector {
            return Err(wasmtime::Error::msg(
                "Observation payload must be a canonical vector",
            ));
        }
        let count = count(store, &data[1])?;
        let values = self.vector_values(store, &data, 0, count, 0)?;
        Ok(values
            .iter()
            .map(|value| {
                self.value(store, value, 0)
                    .map_err(|error| format!("Host error: {error}"))
            })
            .collect())
    }

    fn value(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        value: &Val,
        depth: usize,
    ) -> wasmtime::Result<Observation> {
        if depth >= 128 {
            return Err(wasmtime::Error::msg("ABI2 decoder nesting limit"));
        }
        self.spend(1)?;
        let Val::AnyRef(Some(reference)) = value else {
            return Err(wasmtime::Error::msg(
                "Expected non-null ABI2 language value",
            ));
        };
        if let Some(sentinel) = reference.as_i31(&*store)? {
            return match sentinel.get_u32() {
                0 => Ok(Observation::Nil),
                2 => Ok(Observation::Bool(false)),
                4 => Ok(Observation::Bool(true)),
                _ => Err(wasmtime::Error::msg("Unknown ABI2 language sentinel")),
            };
        }
        if let Some(array) = reference.as_array(&*store)? {
            let ty = array.ty(&*store)?;
            if !matches!(ty.element_type(), StorageType::I16)
                || ty.mutability() != wasmtime::Mutability::Var
                || ty.finality() != wasmtime::Finality::Final
            {
                return Err(wasmtime::Error::msg("Expected UTF-16 language string"));
            }
            self.spend(array.len(&*store)? as usize)?;
            let units = array
                .elems(&mut *store)?
                .map(|value| {
                    value
                        .i32()
                        .map(|unit| unit as u16)
                        .ok_or_else(|| wasmtime::Error::msg("Malformed UTF-16 unit"))
                })
                .collect::<wasmtime::Result<Vec<_>>>()?;
            return Ok(Observation::String(units));
        }
        if let Some(structure) = reference.as_struct(&*store)? {
            let fields = structure.fields(&mut *store)?.collect::<Vec<_>>();
            if let [Val::F64(_)] = fields.as_slice() {
                return Ok(Observation::Number(number_bits(store, value)?));
            }
            if fields.len() == 4 {
                let (class, args) = self.object(store, value)?;
                match class {
                    Class::ExceptionInfo => {
                        return Ok(Observation::ExceptionInfo {
                            message: Box::new(self.value(store, &args[0], depth + 1)?),
                            data: Box::new(self.value(store, &args[1], depth + 1)?),
                            cause: Box::new(self.value(store, &args[2], depth + 1)?),
                        });
                    }
                    Class::Keyword | Class::Symbol => {
                        let keyword = class == Class::Keyword;
                        if args.len() != if keyword { 4 } else { 5 } {
                            return Err(wasmtime::Error::msg("Malformed identifier field count"));
                        }
                        let namespace = match self.value(store, &args[0], depth + 1)? {
                            Observation::Nil => None,
                            Observation::String(units) => Some(units),
                            _ => {
                                return Err(wasmtime::Error::msg("Malformed identifier namespace"));
                            }
                        };
                        let Observation::String(name) = self.value(store, &args[1], depth + 1)?
                        else {
                            return Err(wasmtime::Error::msg("Malformed identifier name"));
                        };
                        return Ok(if keyword {
                            Observation::Keyword(namespace, name)
                        } else {
                            Observation::Symbol(namespace, name)
                        });
                    }
                    Class::ArrayMap | Class::HashMap => {
                        return self.map(store, class, &args, depth);
                    }
                    Class::HashSet => {
                        let (map_class, map_data) = self.object(store, &args[1])?;
                        if !matches!(map_class, Class::ArrayMap | Class::HashMap) {
                            return Err(wasmtime::Error::msg(
                                "Hash set needs canonical backing map",
                            ));
                        }
                        let Observation::Map(pairs) =
                            self.map(store, map_class, &map_data, depth)?
                        else {
                            unreachable!()
                        };
                        return Ok(Observation::Set(
                            pairs.into_iter().map(|(key, _)| key).collect(),
                        ));
                    }
                    Class::Vector => return self.vector(store, &args, depth),
                    Class::Subvec => {
                        let start = count_field(store, &args[2])?;
                        let end = count_field(store, &args[3])?;
                        return Ok(Observation::Vector(self.indexed_slice(
                            store,
                            &args[1],
                            start,
                            end,
                            depth + 1,
                        )?));
                    }
                    Class::RSeq => {
                        let last = count_field(store, &args[1])?;
                        let end = last.checked_add(1).ok_or_else(|| {
                            wasmtime::Error::msg("Reverse sequence index overflow")
                        })?;
                        let mut items = self.indexed_slice(store, &args[0], 0, end, depth + 1)?;
                        items.reverse();
                        return Ok(Observation::List(items));
                    }
                    Class::IndexedSeq => return self.indexed(store, &args, depth),
                    Class::ChunkedSeq => return self.chunked(store, &args, depth),
                    Class::MapEntry | Class::BlackNode | Class::RedNode => {
                        return Ok(Observation::Vector(vec![
                            self.value(store, &args[0], depth + 1)?,
                            self.value(store, &args[1], depth + 1)?,
                        ]));
                    }
                    Class::List
                    | Class::EmptyList
                    | Class::Cons
                    | Class::LazySeq
                    | Class::ChunkedCons => {
                        return self.list(store, value, depth);
                    }
                    Class::IntegerRange | Class::Range => return self.range(store, class, &args),
                    Class::ArrayMapSeq => return self.array_map_seq(store, &args, depth, None),
                    Class::NodeSeq | Class::ArrayNodeSeq => {
                        return self.hash_seq(store, class, &args, depth, None);
                    }
                    Class::TreeSeq => return self.tree_seq(store, &args, depth, None),
                    Class::KeySeq | Class::ValSeq => {
                        return self.entry_sequence(
                            store,
                            &args[0],
                            if class == Class::KeySeq { 0 } else { 1 },
                            depth + 1,
                        );
                    }
                    _ => (),
                }
            }
        }
        Err(wasmtime::Error::msg("Unsupported ABI2 value layout"))
    }

    fn object(
        &self,
        store: &mut StoreContextMut<'_, ()>,
        value: &Val,
    ) -> wasmtime::Result<(Class, Vec<Val>)> {
        let owner = fields(store, value, 4)?;
        let descriptor = non_null_ref(&owner[0])?;
        for canonical in &self.canonical {
            let captured = canonical.descriptor.to_rooted(&mut *store);
            if Rooted::ref_eq(&*store, descriptor, &captured)? {
                let ty = non_null_ref(value)?
                    .as_struct(&*store)?
                    .unwrap()
                    .ty(&*store)?;
                if !wasmtime::StructType::eq(&ty, &canonical.owner_type) {
                    return Err(wasmtime::Error::msg("Malformed nominal owner layout"));
                }
                let args = non_null_ref(&owner[1])?
                    .as_array(&*store)?
                    .ok_or_else(|| wasmtime::Error::msg("Malformed nominal field storage"))?;
                let expected = match canonical.class {
                    Class::Keyword
                    | Class::Cons
                    | Class::ArrayMap
                    | Class::CollisionNode
                    | Class::LazySeq
                    | Class::ChunkedCons => 4,
                    Class::Symbol
                    | Class::List
                    | Class::NodeSeq
                    | Class::ArrayNodeSeq
                    | Class::TreeSeq
                    | Class::BlackNode
                    | Class::RedNode
                    | Class::Subvec => 5,
                    Class::Vector | Class::HashMap | Class::ChunkedSeq | Class::IntegerRange => 6,
                    Class::Range => 7,
                    Class::Node | Class::KeySeq | Class::ValSeq => 2,
                    Class::IndexedSeq
                    | Class::MapEntry
                    | Class::HashSet
                    | Class::BitmapNode
                    | Class::ArrayNode
                    | Class::ExceptionInfo
                    | Class::ArrayChunk
                    | Class::ArrayMapSeq
                    | Class::RSeq => 3,
                    Class::SourceArray => 2,
                    Class::EmptyList => 1,
                };
                if args.len(&*store)? != expected {
                    return Err(wasmtime::Error::msg("Malformed nominal field count"));
                }
                return Ok((canonical.class, args.elems(&mut *store)?.collect()));
            }
        }
        Err(wasmtime::Error::msg("Unsupported ABI2 value layout"))
    }

    fn source_array(
        &self,
        store: &mut StoreContextMut<'_, ()>,
        value: &Val,
        expected: usize,
    ) -> wasmtime::Result<Vec<Val>> {
        let array = self.raw_array(store, value)?;
        if array.len() != expected {
            return Err(wasmtime::Error::msg("Malformed source array length"));
        }
        Ok(array)
    }

    fn node(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        value: &Val,
    ) -> wasmtime::Result<Vec<Val>> {
        self.spend(1)?;
        let (class, data) = self.object(store, value)?;
        if class != Class::Node {
            return Err(wasmtime::Error::msg("Expected canonical vector node"));
        }
        self.source_array(store, &data[1], 32)
    }

    fn vector(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        data: &[Val],
        depth: usize,
    ) -> wasmtime::Result<Observation> {
        Ok(Observation::Vector(
            self.vector_range(store, data, 0, depth)?,
        ))
    }

    fn vector_range(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        data: &[Val],
        start: usize,
        depth: usize,
    ) -> wasmtime::Result<Vec<Observation>> {
        let end = count(store, &data[1])?;
        self.vector_slice(store, data, start, end, depth)
    }

    fn indexed_count(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        value: &Val,
        depth: usize,
    ) -> wasmtime::Result<usize> {
        if depth >= 128 {
            return Err(wasmtime::Error::msg("Indexed backing depth limit"));
        }
        self.spend(1)?;
        let (class, data) = self.object(store, value)?;
        match class {
            Class::Vector => Ok(count(store, &data[1])?),
            Class::Subvec => {
                let start = count_field(store, &data[2])?;
                let end = count_field(store, &data[3])?;
                let backing = self.indexed_count(store, &data[1], depth + 1)?;
                if start > end || end > backing {
                    return Err(wasmtime::Error::msg(
                        "Subvector declared bounds exceed backing",
                    ));
                }
                Ok(end - start)
            }
            _ => Err(wasmtime::Error::msg(
                "Indexed backing needs canonical vector/subvector",
            )),
        }
    }

    fn indexed_slice(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        value: &Val,
        start: usize,
        end: usize,
        depth: usize,
    ) -> wasmtime::Result<Vec<Observation>> {
        if depth >= 128 || start > end {
            return Err(wasmtime::Error::msg("Indexed slice depth/bounds"));
        }
        self.spend(1)?;
        let (class, data) = self.object(store, value)?;
        match class {
            Class::Vector => self.vector_slice(store, &data, start, end, depth),
            Class::Subvec => {
                let from = count_field(store, &data[2])?;
                let to = count_field(store, &data[3])?;
                let backing = self.indexed_count(store, &data[1], depth + 1)?;
                if from > to || to > backing || end > to - from {
                    return Err(wasmtime::Error::msg("Subvector slice exceeds bounds"));
                }
                self.indexed_slice(store, &data[1], from + start, from + end, depth + 1)
            }
            _ => Err(wasmtime::Error::msg(
                "Indexed slice needs canonical vector/subvector",
            )),
        }
    }

    fn vector_slice(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        data: &[Val],
        start: usize,
        end: usize,
        depth: usize,
    ) -> wasmtime::Result<Vec<Observation>> {
        let values = self.vector_values(store, data, start, end, depth)?;
        values
            .iter()
            .map(|item| self.value(store, item, depth + 1))
            .collect()
    }

    fn vector_values(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        data: &[Val],
        start: usize,
        end: usize,
        _depth: usize,
    ) -> wasmtime::Result<Vec<Val>> {
        let count = count(store, &data[1])?;
        if start > end || end > count {
            return Err(wasmtime::Error::msg("Vector sequence offset exceeds count"));
        }
        let shift = count_field(store, &data[2])?;
        if !(5..=30).contains(&shift) || shift % 5 != 0 {
            return Err(wasmtime::Error::msg("Invalid vector trie shift"));
        }
        if end - start > self.remaining {
            return Err(wasmtime::Error::msg("ABI2 decoder traversal limit"));
        }
        let tail_start = if count <= 32 {
            0
        } else {
            ((count - 1) >> 5) << 5
        };
        if tail_start as u64 > (1_u64 << (shift + 5)) {
            return Err(wasmtime::Error::msg("Vector count exceeds trie capacity"));
        }
        let tail = self.source_array(store, &data[4], count - tail_start)?;
        let (root_class, root_data) = self.object(store, &data[3])?;
        if root_class != Class::Node
            || !matches!(non_null_ref(&root_data[0])?.as_i31(&*store)?, Some(nil) if nil.get_u32() == 0)
        {
            return Err(wasmtime::Error::msg(
                "Persistent vector root has an active edit token",
            ));
        }
        self.node(store, &data[3])?;
        let mut result = Vec::with_capacity(end - start);
        for index in start..end {
            let item = if index >= tail_start {
                tail[index - tail_start].clone()
            } else {
                let mut current = data[3].clone();
                let mut level = shift;
                loop {
                    let array = self.node(store, &current)?;
                    if level == 0 {
                        break array[index & 31].clone();
                    }
                    current = array[(index >> level) & 31].clone();
                    level -= 5;
                }
            };
            result.push(item);
        }
        Ok(result)
    }

    fn chunked(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        data: &[Val],
        depth: usize,
    ) -> wasmtime::Result<Observation> {
        let (class, vector) = self.object(store, &data[0])?;
        if class != Class::Vector {
            return Err(wasmtime::Error::msg(
                "Chunked sequence needs canonical vector",
            ));
        }
        let count = count(store, &vector[1])?;
        let index = count_field(store, &data[2])?;
        let offset = count_field(store, &data[3])?;
        let node = self.raw_array(store, &data[1])?;
        let length = node.len();
        if index >= count
            || index % 32 != 0
            || length != (count - index).min(32)
            || offset >= length
        {
            return Err(wasmtime::Error::msg("Malformed chunked sequence bounds"));
        }
        if count - index - offset > self.remaining {
            return Err(wasmtime::Error::msg("ABI2 decoder traversal limit"));
        }
        let mut result = Vec::with_capacity(count - index - offset);
        for item in offset..length {
            let value = node[item].clone();
            result.push(self.value(store, &value, depth + 1)?);
        }
        result.extend(self.vector_range(store, &vector, index + length, depth)?);
        Ok(Observation::List(result))
    }

    fn raw_array(
        &self,
        store: &mut StoreContextMut<'_, ()>,
        value: &Val,
    ) -> wasmtime::Result<Vec<Val>> {
        fn uint32(store: &mut StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<u32> {
            let n = f64::from_bits(number_bits(store, value)?);
            if !n.is_finite() || n < 0.0 || n > f64::from(u32::MAX) || n.fract() != 0.0 {
                return Err(wasmtime::Error::msg("Invalid source array uint32"));
            }
            Ok(n as u32)
        }
        fn args(
            store: &mut StoreContextMut<'_, ()>,
            value: &Val,
        ) -> wasmtime::Result<Rooted<wasmtime::ArrayRef>> {
            let array = non_null_ref(value)?
                .as_array(&*store)?
                .ok_or_else(|| wasmtime::Error::msg("Malformed source array storage"))?;
            if !matches!(
                array.ty(&*store)?.element_type(),
                StorageType::ValType(wasmtime::ValType::Ref(_))
            ) {
                return Err(wasmtime::Error::msg("Malformed source array element type"));
            }
            Ok(array)
        }
        let (class, data) = self.object(store, value)?;
        if class != Class::SourceArray {
            return Err(wasmtime::Error::msg("Expected canonical source array"));
        }
        let backing = args(store, &data[0])?;
        if backing.len(&*store)? != 4 {
            return Err(wasmtime::Error::msg(
                "Malformed source array backing layout",
            ));
        }
        let backing = backing.elems(&mut *store)?.collect::<Vec<_>>();
        let length = uint32(store, &backing[0])?;
        // Host observation is bounded before allocation, independently of the
        // sparse logical length. No guest operations or fabricated nil values.
        if length > 1_000_000 {
            return Err(wasmtime::Error::msg("ABI2 decoder source array limit"));
        }
        let dense = args(store, &backing[2])?;
        let mask = non_null_ref(&backing[3])?
            .as_array(&*store)?
            .ok_or_else(|| wasmtime::Error::msg("Malformed source array presence mask"))?;
        if !matches!(mask.ty(&*store)?.element_type(), StorageType::I16)
            || mask.len(&*store)? != dense.len(&*store)?
            || dense.len(&*store)? > length
        {
            return Err(wasmtime::Error::msg(
                "Malformed source array dense/presence layout",
            ));
        }
        let undefined = Val::AnyRef(Some(AnyRef::from_i31(
            &mut *store,
            wasmtime::I31::wrapping_u32(6),
        )));
        let mut result = vec![undefined; length as usize];
        let dense_length = dense.len(&*store)?;
        for index in 0..dense_length {
            match mask.get(&mut *store, index)?.unwrap_i32() {
                0 => (),
                1 => result[index as usize] = dense.get(&mut *store, index)?,
                _ => return Err(wasmtime::Error::msg("Malformed source array presence bit")),
            }
        }
        let mut next = backing[1].clone();
        let mut previous = None;
        let mut nodes = 0;
        while sentinel(store, &next)? != Some(0) {
            nodes += 1;
            if nodes > 1_000_001 {
                return Err(wasmtime::Error::msg("ABI2 decoder sparse chain limit"));
            }
            let node = args(store, &next)?;
            if node.len(&*store)? != 3 {
                return Err(wasmtime::Error::msg("Malformed source array sparse node"));
            }
            let node = node.elems(&mut *store)?.collect::<Vec<_>>();
            let key = uint32(store, &node[0])?;
            // Strict ordering also rejects every cycle when its key repeats.
            if previous.is_some_and(|prior| key <= prior)
                || key < dense_length
                || (key != u32::MAX && key >= length)
            {
                return Err(wasmtime::Error::msg(
                    "Malformed source array sparse key order/range",
                ));
            }
            if key != u32::MAX {
                result[key as usize] = node[1].clone();
            }
            previous = Some(key);
            next = node[2].clone();
        }
        Ok(result)
    }

    fn map(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        class: Class,
        data: &[Val],
        depth: usize,
    ) -> wasmtime::Result<Observation> {
        let declared = count_field(store, &data[1])?;
        if declared > self.remaining / 2 {
            return Err(wasmtime::Error::msg("ABI2 decoder traversal limit"));
        }
        let mut pairs = Vec::with_capacity(declared);
        if class == Class::ArrayMap {
            let array = self.source_array(store, &data[2], declared * 2)?;
            for index in 0..declared {
                let key = array[index * 2].clone();
                let value = array[index * 2 + 1].clone();
                pairs.push((
                    self.value(store, &key, depth + 1)?,
                    self.value(store, &value, depth + 1)?,
                ));
            }
        } else {
            match sentinel(store, &data[3])? {
                Some(2) => (),
                Some(4) => {
                    self.spend(1)?;
                    pairs.push((Observation::Nil, self.value(store, &data[4], depth + 1)?));
                }
                _ => return Err(wasmtime::Error::msg("Malformed hash map nil-key flag")),
            }
            if sentinel(store, &data[2])? != Some(0) {
                self.hash_node(store, &data[2], 0, depth, declared, &mut pairs)?;
            }
            if pairs.len() != declared {
                return Err(wasmtime::Error::msg(
                    "Hash map count disagrees with storage",
                ));
            }
        }
        Ok(Observation::Map(pairs))
    }

    fn hash_node(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        value: &Val,
        level: usize,
        depth: usize,
        declared: usize,
        pairs: &mut Vec<(Observation, Observation)>,
    ) -> wasmtime::Result<()> {
        let mut raw = Vec::new();
        self.hash_node_values(
            store,
            value,
            level,
            depth,
            declared.saturating_sub(pairs.len()),
            &mut raw,
        )?;
        for (key, value) in raw {
            pairs.push((
                self.value(store, &key, depth + 1)?,
                self.value(store, &value, depth + 1)?,
            ));
        }
        Ok(())
    }

    fn hash_node_values(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        value: &Val,
        level: usize,
        depth: usize,
        declared: usize,
        pairs: &mut Vec<(Val, Val)>,
    ) -> wasmtime::Result<()> {
        if level > 7 {
            return Err(wasmtime::Error::msg("Hash trie exceeds 32-bit path depth"));
        }
        self.spend(1)?;
        let (class, data) = self.object(store, value)?;
        match class {
            Class::ArrayNode => {
                let expected = count_field(store, &data[1])?;
                let array = self.source_array(store, &data[2], 32)?;
                let mut occupied = 0;
                for index in 0..32 {
                    let child = array[index].clone();
                    if !internal_absent(store, &child)? {
                        occupied += 1;
                        self.hash_node_values(store, &child, level + 1, depth, declared, pairs)?;
                    }
                }
                if occupied != expected {
                    return Err(wasmtime::Error::msg(
                        "Hash array node count disagrees with storage",
                    ));
                }
            }
            Class::BitmapNode | Class::CollisionNode => {
                let collision = class == Class::CollisionNode;
                let bitmap = signed_word(store, &data[1])?;
                let count = if collision {
                    count_field(store, &data[2])?
                } else {
                    bitmap.count_ones() as usize
                };
                if collision && count == 0 {
                    return Err(wasmtime::Error::msg("Empty hash collision node"));
                }
                let array = self.raw_array(store, &data[if collision { 3 } else { 2 }])?;
                let length = array.len();
                if length % 2 != 0 || count * 2 > length || count > self.remaining {
                    return Err(wasmtime::Error::msg(
                        "Hash node population exceeds storage or traversal bound",
                    ));
                }
                for index in 0..count {
                    let key = array[index * 2].clone();
                    let value = array[index * 2 + 1].clone();
                    if internal_absent(store, &key)? {
                        if collision {
                            return Err(wasmtime::Error::msg("Absent key in hash collision node"));
                        }
                        self.hash_node_values(store, &value, level + 1, depth, declared, pairs)?;
                    } else {
                        if pairs.len() >= declared {
                            return Err(wasmtime::Error::msg(
                                "Hash trie exceeds declared map count",
                            ));
                        }
                        pairs.push((key, value));
                    }
                }
            }
            _ => return Err(wasmtime::Error::msg("Expected canonical hash trie node")),
        }
        Ok(())
    }

    fn indexed(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        data: &[Val],
        depth: usize,
    ) -> wasmtime::Result<Observation> {
        let index = count_field(store, &data[1])?;
        let backing = non_null_ref(&data[0])?;
        let (array, string) = if let Some(array) = backing.as_array(&*store)? {
            let ty = array.ty(&*store)?;
            if !matches!(ty.element_type(), StorageType::I16)
                || ty.mutability() != wasmtime::Mutability::Var
                || ty.finality() != wasmtime::Finality::Final
            {
                return Err(wasmtime::Error::msg(
                    "Indexed string requires ABI2 UTF16 storage",
                ));
            }
            let length = array.len(&*store)? as usize;
            if index > length || length - index > self.remaining {
                return Err(wasmtime::Error::msg(
                    "ABI2 decoder indexed bounds/traversal limit",
                ));
            }
            (array.elems(&mut *store)?.collect::<Vec<_>>(), true)
        } else {
            (self.raw_array(store, &data[0])?, false)
        };
        let length = array.len();
        if index > length {
            return Err(wasmtime::Error::msg(
                "Indexed offset exceeds backing storage",
            ));
        }
        if length - index > self.remaining {
            return Err(wasmtime::Error::msg("ABI2 decoder traversal limit"));
        }
        let mut result = Vec::with_capacity(length - index);
        for offset in index..length {
            let item = array[offset].clone();
            result.push(if string {
                self.spend(1)?;
                let Val::I32(unit) = item else {
                    return Err(wasmtime::Error::msg("Malformed UTF16 indexed unit"));
                };
                Observation::String(vec![unit as u16])
            } else {
                self.value(store, &item, depth + 1)?
            });
        }
        Ok(Observation::List(result))
    }

    fn range(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        class: Class,
        data: &[Val],
    ) -> wasmtime::Result<Observation> {
        let mut current = f64::from_bits(number_bits(store, &data[1])?);
        let end = f64::from_bits(number_bits(store, &data[2])?);
        let step = f64::from_bits(number_bits(store, &data[3])?);
        let estimated = ((end - current) / step).ceil();
        if !current.is_finite()
            || !end.is_finite()
            || !step.is_finite()
            || step == 0.0
            || !estimated.is_finite()
            || estimated <= 0.0
            || estimated > 100_000.0
            || estimated > self.remaining as f64
        {
            return Err(wasmtime::Error::msg(
                "ABI2 decoder range bounds/traversal limit",
            ));
        }
        let declared = if class == Class::IntegerRange {
            Some(count_field(store, &data[4])?)
        } else {
            None
        };
        let mut result = Vec::new();
        while if step > 0.0 {
            current < end
        } else {
            current > end
        } {
            if result.len() >= 100_000 {
                return Err(wasmtime::Error::msg("ABI2 decoder range traversal limit"));
            }
            self.spend(1)?;
            result.push(Observation::Number(current.to_bits()));
            let next = current + step;
            if next == current {
                return Err(wasmtime::Error::msg("ABI2 decoder range does not advance"));
            }
            current = next;
        }
        if declared.is_some_and(|count| count != result.len()) {
            return Err(wasmtime::Error::msg(
                "Integer range count disagrees with storage",
            ));
        }
        Ok(Observation::List(result))
    }

    fn entry_fields(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        key: &Val,
        value: &Val,
        depth: usize,
        projection: Option<usize>,
    ) -> wasmtime::Result<Observation> {
        match projection {
            Some(0) => self.value(store, key, depth + 1),
            Some(1) => self.value(store, value, depth + 1),
            None => Ok(Observation::Vector(vec![
                self.value(store, key, depth + 1)?,
                self.value(store, value, depth + 1)?,
            ])),
            _ => Err(wasmtime::Error::msg("Invalid entry projection")),
        }
    }

    fn entry_sequence(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        value: &Val,
        projection: usize,
        depth: usize,
    ) -> wasmtime::Result<Observation> {
        if depth >= 128 || projection > 1 {
            return Err(wasmtime::Error::msg("Entry projection depth/index"));
        }
        let mut current = value.clone();
        let mut result = Vec::new();
        let mut counts = Vec::new();
        while sentinel(store, &current)? != Some(0) {
            self.spend(1)?;
            let (class, fields) = self.object(store, &current)?;
            let tail = match class {
                Class::Cons | Class::List => {
                    if class == Class::List {
                        counts.push((result.len(), count_field(store, &fields[3])?));
                    }
                    let (entry_class, entry) = self.object(store, &fields[1])?;
                    let head = match entry_class {
                        Class::MapEntry | Class::BlackNode | Class::RedNode => {
                            self.value(store, &entry[projection], depth + 1)?
                        }
                        Class::Vector => {
                            if count(store, &entry[1])? != 2 {
                                return Err(wasmtime::Error::msg("Projected vector entry width"));
                            }
                            self.vector_slice(store, &entry, projection, projection + 1, depth + 1)?
                                .remove(0)
                        }
                        _ => {
                            return Err(wasmtime::Error::msg(
                                "Projection needs canonical stored entry",
                            ));
                        }
                    };
                    result.push(head);
                    current = fields[2].clone();
                    continue;
                }
                Class::EmptyList => break,
                Class::ArrayMapSeq => {
                    self.array_map_seq(store, &fields, depth + 1, Some(projection))?
                }
                Class::NodeSeq | Class::ArrayNodeSeq => {
                    self.hash_seq(store, class, &fields, depth + 1, Some(projection))?
                }
                Class::TreeSeq => self.tree_seq(store, &fields, depth + 1, Some(projection))?,
                _ => return Err(wasmtime::Error::msg("Unsupported stored entry sequence")),
            };
            let Observation::List(tail) = tail else {
                unreachable!()
            };
            result.extend(tail);
            break;
        }
        for (start, declared) in counts {
            if result.len() - start != declared {
                return Err(wasmtime::Error::msg(
                    "Projected list count disagrees with storage",
                ));
            }
        }
        Ok(Observation::List(result))
    }

    fn array_map_seq(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        data: &[Val],
        depth: usize,
        projection: Option<usize>,
    ) -> wasmtime::Result<Observation> {
        let array = self.raw_array(store, &data[0])?;
        let index = count_field(store, &data[1])?;
        if index >= array.len() || index % 2 != 0 || array.len() % 2 != 0 {
            return Err(wasmtime::Error::msg("Malformed array-map sequence bounds"));
        }
        let mut result = Vec::new();
        for pair in array[index..].chunks_exact(2) {
            result.push(self.entry_fields(store, &pair[0], &pair[1], depth, projection)?);
        }
        Ok(Observation::List(result))
    }

    fn hash_seq(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        class: Class,
        data: &[Val],
        depth: usize,
        projection: Option<usize>,
    ) -> wasmtime::Result<Observation> {
        let nodes = self.raw_array(store, &data[1])?;
        let index = count_field(store, &data[2])?;
        let array_node = class == Class::ArrayNodeSeq;
        if index > nodes.len() || (!array_node && (index % 2 != 0 || nodes.len() % 2 != 0)) {
            return Err(wasmtime::Error::msg("Malformed hash sequence bounds"));
        }
        let has_tail = sentinel(store, &data[3])? != Some(0);
        if array_node {
            if nodes.len() != 32 || index == 0 || !has_tail {
                return Err(wasmtime::Error::msg("Malformed array-node sequence cursor"));
            }
        } else if !has_tail && (index == nodes.len() || internal_absent(store, &nodes[index])?) {
            return Err(wasmtime::Error::msg(
                "Malformed node sequence current entry",
            ));
        }
        let mut result = Vec::new();
        if has_tail {
            let Observation::List(tail) = (match projection {
                Some(index) => self.entry_sequence(store, &data[3], index, depth + 1)?,
                None => self.value(store, &data[3], depth + 1)?,
            }) else {
                return Err(wasmtime::Error::msg("Malformed hash sequence current tail"));
            };
            result.extend(tail);
        }
        let mut pairs = Vec::new();
        let bound = self.remaining / 2;
        if array_node {
            for child in &nodes[index..] {
                if !internal_absent(store, child)? {
                    self.hash_node_values(store, child, 0, depth, bound, &mut pairs)?;
                }
            }
        } else {
            for pair in nodes[index..].chunks_exact(2) {
                if internal_absent(store, &pair[0])? {
                    if !internal_absent(store, &pair[1])? {
                        self.hash_node_values(store, &pair[1], 0, depth, bound, &mut pairs)?;
                    }
                } else {
                    pairs.push((pair[0].clone(), pair[1].clone()));
                }
            }
        }
        for (key, value) in pairs {
            result.push(self.entry_fields(store, &key, &value, depth, projection)?);
        }
        Ok(Observation::List(result))
    }

    fn tree_seq(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        data: &[Val],
        depth: usize,
        projection: Option<usize>,
    ) -> wasmtime::Result<Observation> {
        let ascending = match sentinel(store, &data[2])? {
            Some(4) => true,
            Some(2) => false,
            _ => return Err(wasmtime::Error::msg("Malformed tree sequence direction")),
        };
        let count = f64::from_bits(number_bits(store, &data[3])?);
        // sorted-seq-from starts at -1; rest/next decrement even an unknown
        // count. Every negative integer is therefore a valid unknown count.
        if !count.is_finite() || count.fract() != 0.0 || count > self.remaining as f64 {
            return Err(wasmtime::Error::msg("Malformed tree sequence count"));
        }
        let mut stack = Vec::new();
        let mut current = data[1].clone();
        let mut counts = Vec::new();
        while sentinel(store, &current)? != Some(0) {
            self.spend(1)?;
            let (class, fields) = self.object(store, &current)?;
            match class {
                Class::EmptyList => break,
                Class::List | Class::Cons => {
                    if class == Class::List {
                        counts.push((stack.len(), count_field(store, &fields[3])?));
                    }
                    stack.push(fields[1].clone());
                    current = fields[2].clone();
                }
                _ => return Err(wasmtime::Error::msg("Malformed tree sequence stack")),
            }
        }
        for (start, declared) in counts {
            if stack.len() - start != declared {
                return Err(wasmtime::Error::msg(
                    "Tree stack count disagrees with storage",
                ));
            }
        }
        if stack.is_empty() {
            return Err(wasmtime::Error::msg(
                "Tree sequence needs nonempty node stack",
            ));
        }
        stack.reverse();
        let mut result = Vec::new();
        while let Some(node) = stack.pop() {
            self.spend(1)?;
            let (class, fields) = self.object(store, &node)?;
            if !matches!(class, Class::BlackNode | Class::RedNode) {
                return Err(wasmtime::Error::msg("Tree stack needs canonical tree node"));
            }
            result.push(self.entry_fields(store, &fields[0], &fields[1], depth, projection)?);
            let mut child = fields[if ascending { 3 } else { 2 }].clone();
            let mut path = 0;
            while sentinel(store, &child)? != Some(0) {
                self.spend(1)?;
                path += 1;
                if path >= 128 {
                    return Err(wasmtime::Error::msg("ABI2 decoder tree path limit"));
                }
                let (class, fields) = self.object(store, &child)?;
                if !matches!(class, Class::BlackNode | Class::RedNode) {
                    return Err(wasmtime::Error::msg("Malformed tree sequence child"));
                }
                stack.push(child);
                child = fields[if ascending { 2 } else { 3 }].clone();
            }
        }
        if count >= 0.0 && count as usize != result.len() {
            return Err(wasmtime::Error::msg(
                "Tree sequence count disagrees with storage",
            ));
        }
        Ok(Observation::List(result))
    }

    fn list(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        value: &Val,
        depth: usize,
    ) -> wasmtime::Result<Observation> {
        let mut result = Vec::new();
        let mut counts = Vec::new();
        let mut current = value.clone();
        loop {
            self.spend(1)?;
            if let Some(sentinel) = non_null_ref(&current)?.as_i31(&*store)? {
                if sentinel.get_u32() == 0 {
                    break;
                }
                return Err(wasmtime::Error::msg("Invalid list tail"));
            }
            let (class, data) = self.object(store, &current)?;
            match class {
                Class::EmptyList => break,
                Class::LazySeq => {
                    if sentinel(store, &data[1])? != Some(0) {
                        return Err(wasmtime::Error::msg(
                            "ABI2 decoder cannot force unrealized lazy sequence",
                        ));
                    }
                    current = data[2].clone();
                }
                Class::ChunkedCons => {
                    let (class, chunk) = self.object(store, &data[0])?;
                    if class != Class::ArrayChunk {
                        return Err(wasmtime::Error::msg(
                            "Chunked cons needs canonical array chunk",
                        ));
                    }
                    let array = self.raw_array(store, &chunk[0])?;
                    let start = count_field(store, &chunk[1])?;
                    let end = count_field(store, &chunk[2])?;
                    if start >= end || end > array.len() {
                        return Err(wasmtime::Error::msg("Malformed chunked cons bounds"));
                    }
                    for item in &array[start..end] {
                        result.push(self.value(store, item, depth + 1)?);
                    }
                    current = data[1].clone();
                }
                Class::Vector => {
                    let Observation::Vector(tail) = self.vector(store, &data, depth)? else {
                        unreachable!()
                    };
                    result.extend(tail);
                    break;
                }
                Class::IndexedSeq => {
                    let Observation::List(tail) = self.indexed(store, &data, depth)? else {
                        unreachable!()
                    };
                    result.extend(tail);
                    break;
                }
                Class::ChunkedSeq => {
                    let Observation::List(tail) = self.chunked(store, &data, depth)? else {
                        unreachable!()
                    };
                    result.extend(tail);
                    break;
                }
                Class::List | Class::Cons => {
                    if class == Class::List {
                        let count = count(store, &data[3])?;
                        if count == 0 || count > self.remaining {
                            return Err(wasmtime::Error::msg(
                                "Invalid list count or traversal limit",
                            ));
                        }
                        counts.push((result.len(), count));
                    }
                    result.push(self.value(store, &data[1], depth + 1)?);
                    current = data[2].clone();
                }
                Class::IntegerRange
                | Class::Range
                | Class::ArrayMapSeq
                | Class::KeySeq
                | Class::RSeq
                | Class::ValSeq
                | Class::NodeSeq
                | Class::ArrayNodeSeq
                | Class::TreeSeq => {
                    let Observation::List(tail) = self.value(store, &current, depth + 1)? else {
                        return Err(wasmtime::Error::msg("Invalid list sequence tail"));
                    };
                    result.extend(tail);
                    break;
                }
                _ => return Err(wasmtime::Error::msg("Invalid list tail")),
            }
        }
        for (start, count) in counts {
            if result.len() - start != count {
                return Err(wasmtime::Error::msg("List count disagrees with storage"));
            }
        }
        Ok(Observation::List(result))
    }

    fn spend(&mut self, count: usize) -> wasmtime::Result<()> {
        self.remaining = self
            .remaining
            .checked_sub(count)
            .ok_or_else(|| wasmtime::Error::msg("ABI2 decoder traversal limit"))?;
        Ok(())
    }
}

fn non_null_ref(value: &Val) -> wasmtime::Result<&Rooted<AnyRef>> {
    match value {
        Val::AnyRef(Some(reference)) => Ok(reference),
        _ => Err(wasmtime::Error::msg("Expected non-null ABI2 reference")),
    }
}

fn fields(
    store: &mut StoreContextMut<'_, ()>,
    value: &Val,
    count: usize,
) -> wasmtime::Result<Vec<Val>> {
    let structure = non_null_ref(value)?
        .as_struct(&*store)?
        .ok_or_else(|| wasmtime::Error::msg("Expected ABI2 structure"))?;
    let fields = structure.fields(&mut *store)?.collect::<Vec<_>>();
    if fields.len() != count {
        return Err(wasmtime::Error::msg(
            "Unexpected ABI2 structure field count",
        ));
    }
    Ok(fields)
}

fn number_bits(store: &mut StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<u64> {
    let structure = non_null_ref(value)?
        .as_struct(&*store)?
        .ok_or_else(|| wasmtime::Error::msg("Expected ABI2 number box"))?;
    let ty = structure.ty(&*store)?;
    let field = ty
        .field(0)
        .ok_or_else(|| wasmtime::Error::msg("Malformed ABI2 number box"))?;
    if ty.fields().len() != 1
        || ty.finality() != wasmtime::Finality::Final
        || field.mutability() != wasmtime::Mutability::Const
        || !matches!(
            field.element_type(),
            StorageType::ValType(wasmtime::ValType::F64)
        )
    {
        return Err(wasmtime::Error::msg("Malformed ABI2 number box"));
    }
    let Val::F64(bits) = structure.field(&mut *store, 0)? else {
        return Err(wasmtime::Error::msg("Malformed ABI2 number box"));
    };
    Ok(bits)
}

fn count_field(store: &mut StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<usize> {
    let number = f64::from_bits(number_bits(store, value)?);
    if !number.is_finite() || number < 0.0 || number.fract() != 0.0 || number > 1_000_000.0 {
        return Err(wasmtime::Error::msg("Invalid bounded collection integer"));
    }
    Ok(number as usize)
}
fn count(store: &mut StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<usize> {
    count_field(store, value)
}

fn sentinel(store: &StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<Option<u32>> {
    Ok(non_null_ref(value)?
        .as_i31(&*store)?
        .map(|value| value.get_u32()))
}
fn internal_absent(store: &StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<bool> {
    // Undefined (6) is allowed only for private unused trie storage, never as an observed language value.
    Ok(matches!(sentinel(store, value)?, Some(0 | 6)))
}
fn signed_word(store: &mut StoreContextMut<'_, ()>, value: &Val) -> wasmtime::Result<u32> {
    let number = f64::from_bits(number_bits(store, value)?);
    if !number.is_finite()
        || number.fract() != 0.0
        || number < i32::MIN as f64
        || number > i32::MAX as f64
    {
        return Err(wasmtime::Error::msg("Invalid signed hash word"));
    }
    Ok(number as i32 as u32)
}

#[cfg(test)]
mod adversarial_owner {
    use super::*;
    #[test]
    fn sparse_array_decoder_rejects_corrupt_mask_cycles_and_range_then_recovers() {
        let mut session = Session::new_repl().unwrap();
        let decoder = Decoder::capture(&mut session, 4096).unwrap();
        let dense = session.eval("(array 7)").unwrap();
        let sparse = session.eval("(let [a (array)] (aset a 1 7) a)").unwrap();
        session.collect().unwrap();
        session
            .inspect(&dense, |mut store, value| {
                let (_, owner) = decoder.object(&mut store, &value)?;
                let backing = non_null_ref(&owner[0])?.as_array(&store)?.unwrap();
                let mask = non_null_ref(&backing.get(&mut store, 3)?)?
                    .as_array(&store)?
                    .unwrap();
                mask.set(&mut store, 0, Val::I32(2))?;
                assert!(decoder.raw_array(&mut store, &value).is_err());
                mask.set(&mut store, 0, Val::I32(1))?;
                assert_eq!(decoder.raw_array(&mut store, &value)?.len(), 1);
                Ok(())
            })
            .unwrap();
        session
            .inspect(&sparse, |mut store, value| {
                let (_, owner) = decoder.object(&mut store, &value)?;
                let backing = non_null_ref(&owner[0])?.as_array(&store)?.unwrap();
                let head = backing.get(&mut store, 1)?;
                let node = non_null_ref(&head)?.as_array(&store)?.unwrap();
                let old_next = node.get(&mut store, 2)?;
                node.set(&mut store, 2, head.clone())?;
                assert!(decoder.raw_array(&mut store, &value).is_err());
                node.set(&mut store, 2, old_next)?;
                let old_key = node.get(&mut store, 0)?;
                let length = backing.get(&mut store, 0)?;
                node.set(&mut store, 0, length)?;
                assert!(decoder.raw_array(&mut store, &value).is_err());
                node.set(&mut store, 0, old_key)?;
                let slots = decoder.raw_array(&mut store, &value)?;
                assert_eq!(slots.len(), 2);
                assert_eq!(sentinel(&store, &slots[0])?, Some(6));
                assert_eq!(number_bits(&mut store, &slots[1])?, 7.0_f64.to_bits());
                Ok(())
            })
            .unwrap();
    }
    #[test]
    fn indexed_string_rejects_immutable_utf16_backing() {
        let mut session = Session::new_repl().unwrap();
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        let anchor = session
            .eval_probe("(new suss.core/IndexedSeq \"A\" 0 nil)")
            .unwrap();
        session
            .inspect(&anchor, |mut store, canonical| {
                let owner = fields(&mut store, &canonical, 4)?;
                let data = non_null_ref(&owner[1])?.as_array(&store)?.unwrap();
                let ty = wasmtime::ArrayType::new(
                    store.engine(),
                    wasmtime::FieldType::new(wasmtime::Mutability::Const, StorageType::I16),
                );
                let allocator = wasmtime::ArrayRefPre::new(&mut store, ty);
                let backing = wasmtime::ArrayRef::new(&mut store, &allocator, &Val::I32(65), 1)?;
                data.set(&mut store, 0, Val::AnyRef(Some(backing.to_anyref())))?;
                assert!(
                    decoder.value(&mut store, &canonical, 0).is_err(),
                    "an indexed language string needs mutable ABI2 UTF16 backing"
                );
                Ok(())
            })
            .unwrap();
    }
    #[test]
    fn canonical_descriptor_does_not_validate_foreign_owner_layout() {
        let mut session = Session::new_repl().unwrap();
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        let anchor = session.eval(":ready").unwrap();
        session
            .inspect(&anchor, |mut store, canonical| {
                let data = fields(&mut store, &canonical, 4)?;
                let canonical_type = canonical
                    .unwrap_anyref()
                    .unwrap()
                    .as_struct(&store)?
                    .unwrap()
                    .ty(&store)?;
                let ty = wasmtime::StructType::new(
                    store.engine(),
                    (0..4).map(|_| {
                        wasmtime::FieldType::new(
                            wasmtime::Mutability::Const,
                            wasmtime::StorageType::ValType(wasmtime::ValType::Ref(
                                wasmtime::RefType::EQREF,
                            )),
                        )
                    }),
                )?;
                assert!(!wasmtime::StructType::eq(&canonical_type, &ty));
                let allocator = wasmtime::StructRefPre::new(&mut store, ty);
                let owner = wasmtime::StructRef::new(&mut store, &allocator, &data)?;
                // This internal raw fixture shares the inspected Store and retains
                // the genuine descriptor and fields. Only the owner Wasm layout is
                // foreign; no ownership bypass is exposed by the decoder API.
                let value = Val::AnyRef(Some(owner.to_anyref()));
                assert!(
                    decoder.value(&mut store, &value, 0).is_err(),
                    "a canonical descriptor must not validate a foreign owner layout"
                );
                Ok(())
            })
            .unwrap();
    }
}
