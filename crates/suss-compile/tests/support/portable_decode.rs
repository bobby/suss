//! Independent ABI2 observations. Never call the guest printer or equality.
//! Inspect canonical storage directly; unknown objects and malformed layouts fail.
use suss_compile::portable_session::{Session, SessionError, SessionValue};
use wasmtime::{AnyRef, OwnedRooted, Rooted, StorageType, StoreContextMut, Val};

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
            let anchor = session.eval(source)?;
            let (descriptor, owner_type) = session.inspect(&anchor, |mut store, value| {
                let owner_type = non_null_ref(&value)?
                    .as_struct(&store)?
                    .ok_or_else(|| wasmtime::Error::msg("Invalid canonical object storage"))?
                    .ty(&store)?;
                let object = fields(&mut store, &value, 4)?;
                let descriptor = &object[0];
                let storage = fields(&mut store, descriptor, 5)?;
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
            decoder.canonical.push(Canonical {
                anchor,
                descriptor,
                owner_type,
                class,
            });
        }
        Ok(decoder)
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
                    Class::IndexedSeq => return self.indexed(store, &args, depth),
                    Class::ChunkedSeq => return self.chunked(store, &args, depth),
                    Class::MapEntry => {
                        return Ok(Observation::Vector(vec![
                            self.value(store, &args[0], depth + 1)?,
                            self.value(store, &args[1], depth + 1)?,
                        ]));
                    }
                    Class::List | Class::EmptyList | Class::Cons => {
                        return self.list(store, value, depth);
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
                    Class::Keyword | Class::Cons | Class::ArrayMap | Class::CollisionNode => 4,
                    Class::Symbol | Class::List => 5,
                    Class::Vector | Class::HashMap | Class::ChunkedSeq => 6,
                    Class::Node => 2,
                    Class::IndexedSeq
                    | Class::MapEntry
                    | Class::HashSet
                    | Class::BitmapNode
                    | Class::ArrayNode
                    | Class::ExceptionInfo => 3,
                    Class::SourceArray | Class::EmptyList => 1,
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
    ) -> wasmtime::Result<wasmtime::Rooted<wasmtime::ArrayRef>> {
        let (class, data) = self.object(store, value)?;
        if class != Class::SourceArray {
            return Err(wasmtime::Error::msg("Expected canonical source array"));
        }
        let array = non_null_ref(&data[0])?
            .as_array(&*store)?
            .ok_or_else(|| wasmtime::Error::msg("Malformed source array storage"))?;
        if !matches!(
            array.ty(&*store)?.element_type(),
            StorageType::ValType(wasmtime::ValType::Ref(_))
        ) || array.len(&*store)? as usize != expected
        {
            return Err(wasmtime::Error::msg(
                "Malformed source array length or element type",
            ));
        }
        Ok(array)
    }

    fn node(
        &mut self,
        store: &mut StoreContextMut<'_, ()>,
        value: &Val,
    ) -> wasmtime::Result<wasmtime::Rooted<wasmtime::ArrayRef>> {
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
        let count = count(store, &data[1])?;
        if start > count {
            return Err(wasmtime::Error::msg("Vector sequence offset exceeds count"));
        }
        let shift = count_field(store, &data[2])?;
        if !(5..=30).contains(&shift) || shift % 5 != 0 {
            return Err(wasmtime::Error::msg("Invalid vector trie shift"));
        }
        if count - start > self.remaining {
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
        let mut result = Vec::with_capacity(count - start);
        for index in start..count {
            let item = if index >= tail_start {
                tail.get(&mut *store, (index - tail_start) as u32)?
            } else {
                let mut current = data[3].clone();
                let mut level = shift;
                loop {
                    let array = self.node(store, &current)?;
                    if level == 0 {
                        break array.get(&mut *store, (index & 31) as u32)?;
                    }
                    current = array.get(&mut *store, ((index >> level) & 31) as u32)?;
                    level -= 5;
                }
            };
            result.push(self.value(store, &item, depth + 1)?);
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
        let length = node.len(&*store)? as usize;
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
            let value = node.get(&mut *store, item as u32)?;
            result.push(self.value(store, &value, depth + 1)?);
        }
        result.extend(self.vector_range(store, &vector, index + length, depth)?);
        Ok(Observation::List(result))
    }

    fn raw_array(
        &self,
        store: &mut StoreContextMut<'_, ()>,
        value: &Val,
    ) -> wasmtime::Result<Rooted<wasmtime::ArrayRef>> {
        let (class, data) = self.object(store, value)?;
        if class != Class::SourceArray {
            return Err(wasmtime::Error::msg("Expected canonical source array"));
        }
        let array = non_null_ref(&data[0])?
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
                let key = array.get(&mut *store, (index * 2) as u32)?;
                let value = array.get(&mut *store, (index * 2 + 1) as u32)?;
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
                    let child = array.get(&mut *store, index)?;
                    if !internal_absent(store, &child)? {
                        occupied += 1;
                        self.hash_node(store, &child, level + 1, depth, declared, pairs)?;
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
                let length = array.len(&*store)? as usize;
                if length % 2 != 0 || count * 2 > length || count > self.remaining {
                    return Err(wasmtime::Error::msg(
                        "Hash node population exceeds storage or traversal bound",
                    ));
                }
                for index in 0..count {
                    let key = array.get(&mut *store, (index * 2) as u32)?;
                    let value = array.get(&mut *store, (index * 2 + 1) as u32)?;
                    if internal_absent(store, &key)? {
                        if collision {
                            return Err(wasmtime::Error::msg("Absent key in hash collision node"));
                        }
                        self.hash_node(store, &value, level + 1, depth, declared, pairs)?;
                    } else {
                        if pairs.len() >= declared {
                            return Err(wasmtime::Error::msg(
                                "Hash trie exceeds declared map count",
                            ));
                        }
                        pairs.push((
                            self.value(store, &key, depth + 1)?,
                            self.value(store, &value, depth + 1)?,
                        ));
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
            (array, true)
        } else {
            let (class, owner) = self.object(store, &data[0])?;
            if class != Class::SourceArray {
                return Err(wasmtime::Error::msg(
                    "Indexed sequence requires canonical source array",
                ));
            }
            let array = non_null_ref(&owner[0])?
                .as_array(&*store)?
                .ok_or_else(|| wasmtime::Error::msg("Malformed indexed sequence backing"))?;
            if !matches!(
                array.ty(&*store)?.element_type(),
                StorageType::ValType(wasmtime::ValType::Ref(_))
            ) {
                return Err(wasmtime::Error::msg(
                    "Malformed indexed sequence element type",
                ));
            }
            (array, false)
        };
        let length = array.len(&*store)? as usize;
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
            let item = array.get(&mut *store, offset as u32)?;
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
    fn indexed_string_rejects_immutable_utf16_backing() {
        let mut session = Session::new_repl().unwrap();
        let mut decoder = Decoder::capture(&mut session, 4096).unwrap();
        let anchor = session
            .eval("(new suss.core/IndexedSeq \"A\" 0 nil)")
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
