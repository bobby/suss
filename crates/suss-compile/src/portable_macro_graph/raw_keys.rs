//! Graph-local exact raw reader-data identities. No guest objects or callbacks.
use std::cmp::Ordering;
use suss_reader::forms::{Form, Kind};
mod raw_heads;
use raw_heads::RawHeads;
mod canonical_entries;
use canonical_entries::Entries;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct KeyId(pub(super) usize);

/// Native canonical-entry index encoded as index+1. Production owners come ONLY
/// from Entries::prepare after checked next_len and native extent validation.
/// This is a storage domain, not source/guest identity or provenance authority.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct HeadIndex(std::num::NonZeroUsize);
impl HeadIndex {
    // Caller must prepay the decode before using the resulting entry index.
    fn index(self) -> usize { self.0.get() - 1 }
}
pub(super) struct Node {
    pub(super) key: KeyId,
    pub(super) metadata_count: usize,
    pub(super) children: Vec<usize>,
}
#[derive(Default)]
pub(super) struct Plan {
    pub(super) nodes: Vec<Node>,
}
impl Plan {
    /// Drop all live nodes/child buffers, retaining only empty node capacity.
    /// Poll before each bounded drop. On error the caller discards this Plan.
    pub(super) fn clear_metered<E>(
        &mut self, charge: &mut impl FnMut(u64) -> Result<(), E>,
    ) -> Result<(), E> {
        while !self.nodes.is_empty() {
            // Length check, pop, child-buffer drop and loop bookkeeping.
            charge(4)?;
            self.nodes.pop();
        }
        charge(0)
    }
}
#[derive(Default)]
pub(super) struct Interner {
    heads: RawHeads,
    keys: Entries,
    entry_stats: EntryStats,
    profile_calls: u64,
    profile_work: u64,
    profile_comparison_work: u64,
    profile_head_lookup_work: u64,
    profile_head_insert_work: u64,
    #[cfg(test)]
    key_bytes: usize,
}
// Hashes select buckets only. Immutable exact bytes remain the identity proof.
struct Entry {
    key: Vec<u8>,
    next: Option<usize>,
}
// Observational subtotals of accepted existing charges, not new budget work.
#[derive(Default, Debug, Clone, Copy)]
struct EntryStats {
    allocation: u64, relocation: u64, directory_allocation: u64,
    directory_relocation: u64, access: u64, staging: u64, publication: u64,
}
fn bytes<T>(count: usize) -> u64 {
    (count as u64).saturating_mul(std::mem::size_of::<T>() as u64)
}
// Precharge capacity and possible relocation before allocation, then publication.
fn reserve<T, E>(
    v: &mut Vec<T>,
    extra: usize,
    charge: &mut impl FnMut(u64) -> Result<(), E>,
) -> Result<(), E> {
    charge(0)?;
    if extra > v.capacity() - v.len() {
        let capacity = v
            .len()
            .saturating_add(extra)
            .max(v.capacity().saturating_mul(2))
            .max(4);
        charge(bytes::<T>(capacity).saturating_add(bytes::<T>(v.len())))?;
        v.reserve_exact(capacity - v.len());
    }
    Ok(())
}
fn push<T, E>(
    v: &mut Vec<T>,
    value: T,
    charge: &mut impl FnMut(u64) -> Result<(), E>,
) -> Result<(), E> {
    charge(1)?;
    reserve(v, 1, charge)?;
    v.push(value);
    Ok(())
}
fn append<E>(
    out: &mut Vec<u8>,
    value: &[u8],
    charge: &mut impl FnMut(u64) -> Result<(), E>,
) -> Result<(), E> {
    // Bounded chunks poll cancellation without a single unbounded native copy.
    for chunk in value.chunks(256) {
        charge(chunk.len() as u64)?;
        reserve(out, chunk.len(), charge)?;
        out.extend_from_slice(chunk);
    }
    charge(0)
}
fn size<E>(
    out: &mut Vec<u8>,
    value: usize,
    charge: &mut impl FnMut(u64) -> Result<(), E>,
) -> Result<(), E> {
    // Canonical unsigned LEB128 keeps small lengths and child IDs compact.
    // Keys are graph-local; no persisted format or source fact changes. Each
    // integer is self-delimiting, so adjacent IDs retain exact boundaries.
    let mut remaining = value as u64;
    let mut encoded = [0u8; 10];
    let mut length = 0;
    loop {
        // Prepay the mask, shift, continuation test, byte write and cursor work.
        charge(6)?;
        let byte = (remaining & 0x7f) as u8;
        remaining >>= 7;
        encoded[length] = byte | if remaining == 0 { 0 } else { 0x80 };
        length += 1;
        if remaining == 0 { break; }
    }
    append(out, &encoded[..length], charge)
}
fn text<E>(
    out: &mut Vec<u8>,
    value: &str,
    charge: &mut impl FnMut(u64) -> Result<(), E>,
) -> Result<(), E> {
    size(out, value.len(), charge)?;
    append(out, value.as_bytes(), charge)
}
fn hash_bytes<E>(key: &[u8], charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<u64, E> {
    charge(1)?;
    let mut hash = 0xcbf29ce484222325u64;
    for chunk in key.chunks(256) {
        // Each byte read, xor and wrapping multiply is prepaid.
        charge((chunk.len() as u64).saturating_mul(3))?;
        for byte in chunk {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
    }
    Ok(hash)
}
// Every inspected byte of both inputs is billed before reading it. Lengths are
// immutable and compared only after the common prefix. No native slice cmp.
fn compare_bytes<E>(
    left: &[u8],
    right: &[u8],
    charge: &mut impl FnMut(u64) -> Result<(), E>,
) -> Result<Ordering, E> {
    charge(1)?;
    for index in 0..left.len().min(right.len()) {
        charge(2)?;
        match left[index].cmp(&right[index]) {
            Ordering::Equal => {}
            order => return Ok(order),
        }
    }
    Ok(left.len().cmp(&right.len()))
}
// Fallible bottom-up merge sort: charge comparisons and copies before doing
// them. No uninterruptible std sort or quadratic insertion walk on wide maps.
fn sort_groups<E>(
    groups: &mut Vec<(usize, usize)>,
    charge: &mut impl FnMut(u64) -> Result<(), E>,
) -> Result<(), E> {
    let count = groups.len();
    let mut scratch = Vec::new();
    reserve(&mut scratch, count, charge)?;
    let mut width = 1usize;
    while width < count {
        scratch.clear();
        let mut start = 0;
        while start < count {
            charge(1)?;
            let middle = start.saturating_add(width).min(count);
            let end = middle.saturating_add(width).min(count);
            let (mut left, mut right) = (start, middle);
            while left < middle || right < end {
                charge(3)?;
                let take_left = right == end || (left < middle && groups[left] <= groups[right]);
                let next = if take_left { let i = left; left += 1; i }
                    else { let i = right; right += 1; i };
                scratch.push(groups[next]);
            }
            start = end;
        }
        std::mem::swap(groups, &mut scratch);
        width = width.saturating_mul(2);
    }
    charge(0)
}

impl Interner {
    #[cfg(test)]
    fn intern<E>(
        &mut self,
        mut key: Vec<u8>,
        charge: &mut impl FnMut(u64) -> Result<(), E>,
    ) -> Result<KeyId, E> {
        self.intern_scratch(&mut key, charge)
    }
    fn intern_scratch<E>(
        &mut self,
        key: &mut Vec<u8>,
        charge: &mut impl FnMut(u64) -> Result<(), E>,
    ) -> Result<KeyId, E> {
        let mut work = 0u64;
        let mut comparison_work = 0u64;
        let mut counted = |units| {
            charge(units)?;
            work = work.saturating_add(units);
            Ok(())
        };
        let result = (|| {
            let hash = hash_bytes(&key, &mut counted)?;
            self.intern_bucket_scratch(key, hash, &mut counted, &mut comparison_work)
        })();
        self.profile_calls = self.profile_calls.saturating_add(1);
        self.profile_work = self.profile_work.saturating_add(work);
        self.profile_comparison_work = self.profile_comparison_work.saturating_add(comparison_work);
        if (self.profile_calls.is_multiple_of(128) || result.is_err())
            && std::env::var_os("SUSS_RAW_KEY_TRACE").is_some() {
            eprintln!("raw-key-profile calls={} entries={} intern={} comparison={} head_lookup={} head_insert={}", self.profile_calls, self.keys.len(), self.profile_work, self.profile_comparison_work, self.profile_head_lookup_work, self.profile_head_insert_work);
            let st=&self.entry_stats;
            eprintln!("raw-entry-storage calls={} entries={} capacity={} alloc={} reloc={} dir_alloc={} dir_reloc={} access={} staging={} publication={}",self.profile_calls,self.keys.len(),self.keys.capacity(),st.allocation,st.relocation,st.directory_allocation,st.directory_relocation,st.access,st.staging,st.publication);
        }
        result
    }
    #[cfg(test)]
    fn intern_bucket<E>(
        &mut self,
        mut key: Vec<u8>,
        hash: u64,
        charge: &mut impl FnMut(u64) -> Result<(), E>,
        comparison_work: &mut u64,
    ) -> Result<KeyId, E> {
        self.intern_bucket_scratch(&mut key, hash, charge, comparison_work)
    }
    fn intern_bucket_scratch<E>(
        &mut self,
        key: &mut Vec<u8>,
        hash: u64,
        charge: &mut impl FnMut(u64) -> Result<(), E>,
        comparison_work: &mut u64,
    ) -> Result<KeyId, E> {
        // Diagnostic subtotals count only charge calls returning Ok, including
        // successful charges before a later error. Rejected charge requests are
        // excluded; these are not a complete remaining-fuel delta on failure.
        let head = {
            let work = &mut self.profile_head_lookup_work;
            let mut head_charge = |units| {
                charge(units)?;
                *work = work.saturating_add(units);
                Ok(())
            };
            match self.heads.get(hash, &mut head_charge)? {
                Some(head) => {
                    head_charge(1)?; // decode canonical index before use; included in lookup subtotal
                    Some(head.index())
                }
                None => None,
            }
        };
        let mut cursor = head;
        let mut tail = None;
        while let Some(index) = cursor {
            charge(2)?;
            self.entry_stats.access=self.entry_stats.access.saturating_add(2);
            let entry = self.keys.get(index,charge,&mut self.entry_stats)?;
            if compare_bytes(&key, &entry.key, &mut |work| {
                charge(work)?;
                *comparison_work = comparison_work.saturating_add(work);
                Ok(())
            })? == Ordering::Equal {
                return Ok(KeyId(index));
            }
            tail = Some(index);
            cursor = entry.next;
        }
        let prepared=self.keys.prepare(charge,&mut self.entry_stats)?;
        charge(3)?;
        self.entry_stats.publication=self.entry_stats.publication.saturating_add(3);
        let index = self.keys.len();
        let tail_location=if let Some(tail)=tail {
            Some(self.keys.prepare_link(tail,charge,&mut self.entry_stats)?)
        } else {None};
        // Prepay transferring the buffer and resetting the private scratch.
        // This must precede head insertion: publication below cannot fail.
        charge(2)?;
        // All fallible work precedes publication. A failed bucket insertion
        // can change capacity, but cannot consume an ID or publish an entry.
        if head.is_none() {
            let head_index = prepared.head_index(charge, &mut self.entry_stats)?;
            let work = &mut self.profile_head_insert_work;
            let mut head_charge = |units| {
                charge(units)?;
                *work = work.saturating_add(units);
                Ok(())
            };
            self.heads.get_or_insert(hash, head_index, &mut head_charge)?;
        }
        #[cfg(test)]
        { self.key_bytes += key.len(); }
        self.keys.publish(prepared,Entry { key: std::mem::take(key), next: None });
        if let Some(location) = tail_location {
            self.keys.link(location,index);
        }
        Ok(KeyId(index))
    }
    pub(super) fn plan<E>(
        &mut self,
        form: &Form,
        plan: &mut Plan,
        charge: &mut impl FnMut(u64) -> Result<(), E>,
    ) -> Result<usize, E> {
        self.plan_with_order(form, plan, false, charge)
    }
    // Logical source comparison only: unlike raw reader occurrence identity,
    // map entries and set elements have no specified iteration order.
    pub(super) fn unordered_plan<E>(
        &mut self,
        form: &Form,
        plan: &mut Plan,
        charge: &mut impl FnMut(u64) -> Result<(), E>,
    ) -> Result<usize, E> {
        self.plan_with_order(form, plan, true, charge)
    }
    fn plan_with_order<E>(
        &mut self,
        form: &Form,
        plan: &mut Plan,
        unordered: bool,
        charge: &mut impl FnMut(u64) -> Result<(), E>,
    ) -> Result<usize, E> {
        enum Work<'a> {
            Enter(&'a Form, Option<usize>),
            Finish(&'a Form, usize),
        }
        let root = plan.nodes.len();
        let result = (|| {
            // One private encoding buffer per plan. Hits retain its allocation;
            // misses move it into immutable canonical storage.
            charge(std::mem::size_of::<Vec<u8>>() as u64 + 1)?;
            let mut key = Vec::new();
            let mut pending = Vec::new();
            push(&mut pending, Work::Enter(form, None), charge)?;
            while let Some(work) = pending.pop() {
                charge(1)?;
                match work {
                    Work::Enter(form, parent) => {
                        let id = plan.nodes.len();
                        push(
                            &mut plan.nodes,
                            Node {
                                key: KeyId(0),
                                metadata_count: form.metadata.len(),
                                children: Vec::new(),
                            },
                            charge,
                        )?;
                        if let Some(parent) = parent {
                            push(&mut plan.nodes[parent].children, id, charge)?;
                        }
                        push(&mut pending, Work::Finish(form, id), charge)?;
                        // Push in reverse: metadata visits precede kind children.
                        match &form.kind {
                            Kind::List(v)
                            | Kind::Vector(v)
                            | Kind::Map(v)
                            | Kind::Set(v)
                            | Kind::Conditional(v)
                            | Kind::SplicingConditional(v) => {
                                for child in v.iter().rev() {
                                    push(&mut pending, Work::Enter(child, Some(id)), charge)?;
                                }
                            }
                            Kind::Discard(child) => {
                                push(&mut pending, Work::Enter(child, Some(id)), charge)?
                            }
                            Kind::Prefix { operator, target } => {
                                push(&mut pending, Work::Enter(target, Some(id)), charge)?;
                                push(&mut pending, Work::Enter(operator, Some(id)), charge)?;
                            }
                            _ => {}
                        }
                        for child in form.metadata.iter().rev() {
                            push(&mut pending, Work::Enter(child, Some(id)), charge)?;
                        }
                    }
                    Work::Finish(form, id) => {
                        charge(1)?;
                        key.clear();
                        let node = &plan.nodes[id];
                        size(&mut key, node.metadata_count, charge)?;
                        for child in &node.children[..node.metadata_count] {
                            charge(1)?;
                            size(&mut key, plan.nodes[*child].key.0, charge)?;
                        }
                        let tag = match &form.kind {
                            Kind::Nil => 0,
                            Kind::Bool(_) => 1,
                            Kind::Number(_) => 2,
                            Kind::String(_) => 3,
                            Kind::Symbol(_) => 4,
                            Kind::Keyword(_) => 5,
                            Kind::List(_) => 6,
                            Kind::Vector(_) => 7,
                            Kind::Set(_) => 8,
                            Kind::Map(_) => 9,
                            Kind::Conditional(_) => 10,
                            Kind::Discard(_) => 11,
                            Kind::Prefix { .. } => 12,
                            Kind::SplicingConditional(_) => 13,
                        };
                        append(&mut key, &[tag], charge)?;
                        match &form.kind {
                            Kind::Bool(v) => append(&mut key, &[u8::from(*v)], charge)?,
                            Kind::Number(v) => {
                                append(&mut key, &v.to_bits().to_le_bytes(), charge)?
                            }
                            Kind::String(v) => {
                                size(&mut key, v.len(), charge)?;
                                // The UTF-16 payload length is known. Allocate
                                // once rather than repeatedly relocating bytes.
                                reserve(&mut key, v.len().saturating_mul(2), charge)?;
                                for unit in v {
                                    append(&mut key, &unit.to_le_bytes(), charge)?;
                                }
                            }
                            Kind::Symbol(v) => {
                                append(&mut key, &[u8::from(v.namespace.is_some())], charge)?;
                                if let Some(ns) = &v.namespace {
                                    text(&mut key, ns, charge)?;
                                }
                                text(&mut key, &v.name, charge)?;
                            }
                            Kind::Keyword(v) => {
                                append(&mut key, &[u8::from(v.namespace.is_some())], charge)?;
                                if let Some(ns) = &v.namespace {
                                    text(&mut key, ns, charge)?;
                                }
                                text(&mut key, &v.name, charge)?;
                            }
                            Kind::List(v)
                            | Kind::Vector(v)
                            | Kind::Map(v)
                            | Kind::Set(v)
                            | Kind::Conditional(v)
                            | Kind::SplicingConditional(v) => size(&mut key, v.len(), charge)?,
                            _ => {}
                        }
                        let children = &node.children[node.metadata_count..];
                        if unordered && matches!(form.kind, Kind::Map(_) | Kind::Set(_)) {
                            let paired = matches!(form.kind, Kind::Map(_));
                            let stride = if paired { 2 } else { 1 };
                            debug_assert!(children.len().is_multiple_of(stride));
                            let mut groups = Vec::new();
                            reserve(&mut groups, children.len() / stride, charge)?;
                            for group in children.chunks_exact(stride) {
                                charge(stride as u64)?;
                                groups.push((plan.nodes[group[0]].key.0,
                                    if paired { plan.nodes[group[1]].key.0 } else { 0 }));
                            }
                            sort_groups(&mut groups, charge)?;
                            for (first, second) in groups {
                                size(&mut key, first, charge)?;
                                if paired { size(&mut key, second, charge)?; }
                            }
                        } else {
                            for child in children {
                                charge(1)?;
                                size(&mut key, plan.nodes[*child].key.0, charge)?;
                            }
                        }
                        let canonical = self.intern_scratch(&mut key, charge)?;
                        plan.nodes[id].key = canonical;
                    }
                }
            }
            Ok(root)
        })();
        // Completed child keys remain valid canonical entries. Incomplete plan
        // nodes are private scratch and must not escape a failed build.
        if result.is_err() {
            plan.nodes.truncate(root);
        }
        result
    }
}
pub(super) fn bind<T: Copy, E>(
    values: &mut Vec<Option<T>>,
    key: KeyId,
    value: T,
    charge: &mut impl FnMut(u64) -> Result<(), E>,
) -> Result<(), E> {
    charge(1)?;
    if key.0 >= values.len() {
        let extra = key.0 - values.len() + 1;
        charge(extra as u64)?;
        reserve(values, extra, charge)?;
        values.resize(key.0 + 1, None);
    }
    values[key.0] = Some(value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Error {
        Fuel,
        Cancel,
    }
    fn form(kind: Kind) -> Form {
        Form {
            span: 0..0,
            metadata: vec![],
            kind,
        }
    }
    fn corpus() -> Vec<Form> {
        let mut values = vec![
            form(Kind::Nil),
            form(Kind::Bool(false)),
            form(Kind::Bool(true)),
        ];
        for bits in [
            0,
            (-0.0f64).to_bits(),
            42f64.to_bits(),
            0x7ff8_0000_0000_0001,
            0x7ff8_0000_0000_0002,
        ] {
            values.push(form(Kind::Number(f64::from_bits(bits))));
        }
        for units in [
            vec![],
            vec![0],
            vec![0xd800],
            vec![0xdc00],
            vec![0xd800, 0xdc00],
            vec![97],
        ] {
            values.push(form(Kind::String(units)));
        }
        for namespace in [None, Some(String::new()), Some("ns".into())] {
            values.push(form(Kind::Symbol(suss_reader::Symbol {
                namespace: namespace.clone(),
                name: "x".into(),
            })));
            values.push(form(Kind::Keyword(suss_reader::Keyword {
                namespace,
                name: "x".into(),
            })));
        }
        let leaves = values.clone();
        for items in [
            vec![],
            vec![leaves[0].clone()],
            vec![leaves[1].clone(), leaves[2].clone()],
            vec![leaves[2].clone(), leaves[1].clone()],
        ] {
            for constructor in [
                Kind::List,
                Kind::Vector,
                Kind::Map,
                Kind::Set,
                Kind::Conditional,
                Kind::SplicingConditional,
            ] {
                values.push(form(constructor(items.clone())));
            }
        }
        values.push(form(Kind::Discard(Box::new(leaves[0].clone()))));
        values.push(form(Kind::Prefix {
            operator: Box::new(leaves[1].clone()),
            target: Box::new(leaves[2].clone()),
        }));
        for metadata in [
            vec![leaves[1].clone(), leaves[2].clone()],
            vec![leaves[2].clone(), leaves[1].clone()],
            vec![leaves[1].clone(), leaves[1].clone()],
        ] {
            let mut value = form(Kind::Vector(vec![leaves[0].clone()]));
            value.metadata = metadata;
            values.push(value);
        }
        let mut moved = values.last().unwrap().clone();
        moved.span = 100..200;
        values.push(moved);
        let mut nested_metadata = form(Kind::Symbol(suss_reader::Symbol {
            namespace: None,
            name: "tag".into(),
        }));
        nested_metadata
            .metadata
            .push(values.last().unwrap().clone());
        values.push(nested_metadata);
        // Same normalized :x=true metadata, different raw representation.
        let keyword = form(Kind::Keyword(suss_reader::Keyword {
            namespace: None,
            name: "x".into(),
        }));
        let mut short = form(Kind::Vector(vec![]));
        short.metadata = vec![keyword.clone()];
        let mut expanded = short.clone();
        expanded.metadata = vec![form(Kind::Map(vec![keyword, form(Kind::Bool(true))]))];
        values.extend([short, expanded]);
        values
    }
    fn key(interner: &mut Interner, value: &Form) -> KeyId {
        let mut plan = Plan::default();
        let root = interner
            .plan(value, &mut plan, &mut |_| Ok::<_, Error>(()))
            .unwrap();
        plan.nodes[root].key
    }
    #[test]
    fn scratch_hits_retain_allocation_and_misses_transfer_exact_bytes() {
        let mut table = Interner::default();
        let mut scratch = vec![7; 300];
        let owned_pointer = scratch.as_ptr();
        assert_eq!(table.intern_bucket_scratch(&mut scratch, 7, &mut |_| Ok::<_, Error>(()), &mut 0), Ok(KeyId(0)));
        assert!(scratch.is_empty());
        assert_eq!(scratch.capacity(), 0);
        let entry = table.keys.get(0, &mut |_| Ok::<_, Error>(()), &mut table.entry_stats).unwrap();
        assert_eq!(entry.key.as_ptr(), owned_pointer);
        assert_eq!(entry.key, vec![7; 300]);
        scratch.extend_from_slice(&[7; 300]);
        let hit_pointer = scratch.as_ptr();
        let hit_capacity = scratch.capacity();
        for _ in 0..3 {
            assert_eq!(table.intern_bucket_scratch(&mut scratch, 7, &mut |_| Ok::<_, Error>(()), &mut 0), Ok(KeyId(0)));
            assert_eq!(scratch.as_ptr(), hit_pointer);
            assert_eq!(scratch.capacity(), hit_capacity);
            assert_eq!(scratch, vec![7; 300]);
            scratch.clear();
            scratch.extend_from_slice(&[7; 300]);
        }
        scratch[299] = 8;
        assert_eq!(table.intern_bucket_scratch(&mut scratch, 7, &mut |_| Ok::<_, Error>(()), &mut 0), Ok(KeyId(1)));
        assert_eq!(scratch.capacity(), 0);
        let entry = table.keys.get(1, &mut |_| Ok::<_, Error>(()), &mut table.entry_stats).unwrap();
        assert_eq!(entry.key.as_ptr(), hit_pointer);
        assert_eq!(entry.key[299], 8);
    }

    #[test]
    fn repeated_plans_preserve_every_canonical_byte_and_id() {
        for unordered in [false, true] {
            let mut table = Interner::default();
            let mut corpus = corpus();
            if unordered {
                // The raw-data corpus also includes odd maps; logical map
                // comparison requires complete key/value pairs.
                for value in &mut corpus {
                    if let Kind::Map(children) = &mut value.kind {
                        if !children.len().is_multiple_of(2) {
                            children.push(form(Kind::Nil));
                        }
                    }
                }
            }
            let mut expected = Vec::new();
            for value in &corpus {
                let mut plan = Plan::default();
                table.plan_with_order(value, &mut plan, unordered, &mut |_| Ok::<_, Error>(())).unwrap();
                expected.push(plan.nodes.iter().map(|n| n.key).collect::<Vec<_>>());
            }
            let count = table.keys.len();
            let bytes: Vec<_> = (0..count).map(|i| table.keys.get(i, &mut |_| Ok::<_, Error>(()), &mut table.entry_stats).unwrap().key.clone()).collect();
            // Every node is now a hit; sibling encodings share the scratch.
            for (value, expected_ids) in corpus.iter().zip(&expected).rev() {
                let mut plan = Plan::default();
                table.plan_with_order(value, &mut plan, unordered, &mut |_| Ok::<_, Error>(())).unwrap();
                assert_eq!(plan.nodes.iter().map(|n| n.key).collect::<Vec<_>>(), *expected_ids);
            }
            assert_eq!(table.keys.len(), count);
            for (i, expected) in bytes.iter().enumerate() {
                assert_eq!(&table.keys.get(i, &mut |_| Ok::<_, Error>(()), &mut table.entry_stats).unwrap().key, expected);
            }
        }
    }

    #[test]
    fn compact_sizes_keep_adjacent_integer_boundaries_exact() {
        for (value, expected) in [
            (0, vec![0]), (127, vec![127]), (128, vec![128, 1]),
            (16383, vec![255, 127]), (16384, vec![128, 128, 1]),
        ] {
            let mut actual = Vec::new();
            size(&mut actual, value, &mut |_| Ok::<_, ()>(())).unwrap();
            assert_eq!(actual, expected);
        }
        let mut pair = Vec::new();
        size(&mut pair, 1, &mut |_| Ok::<_, ()>(())).unwrap();
        size(&mut pair, 128, &mut |_| Ok::<_, ()>(())).unwrap();
        let mut other = Vec::new();
        size(&mut other, 129, &mut |_| Ok::<_, ()>(())).unwrap();
        size(&mut other, 0, &mut |_| Ok::<_, ()>(())).unwrap();
        assert_ne!(pair, other);
        let mut maximum = Vec::new();
        size(&mut maximum, usize::MAX, &mut |_| Ok::<_, ()>(())).unwrap();
        assert_eq!(maximum.len(), (usize::BITS as usize).div_ceil(7));
        assert!(maximum[..maximum.len()-1].iter().all(|byte| byte & 128 != 0));
        assert!(maximum[maximum.len()-1] < 128);
        let mut expected = vec![255; (usize::BITS as usize).div_ceil(7)];
        let last = expected.len() - 1;
        expected[last] = (usize::MAX >> (last * 7)) as u8;
        assert_eq!(maximum, expected);
    }

    #[test]
    fn raw_ids_match_legacy_exact_equivalence_pairwise() {
        let mut interner = Interner::default();
        let corpus = corpus();
        let ids: Vec<_> = corpus.iter().map(|form| key(&mut interner, form)).collect();
        let legacy: Vec<_> = corpus
            .iter()
            .map(|form| super::super::form_key(form).unwrap())
            .collect();
        for i in 0..corpus.len() {
            for j in 0..corpus.len() {
                assert_eq!(
                    ids[i] == ids[j],
                    legacy[i] == legacy[j],
                    "raw equivalence {i}/{j}"
                );
            }
        }
        // Interning order may change numeric IDs, never equivalence.
        let mut reversed = Interner::default();
        for value in corpus.iter().rev() {
            key(&mut reversed, value);
        }
        for i in 0..corpus.len() {
            for j in 0..corpus.len() {
                assert_eq!(
                    key(&mut reversed, &corpus[i]) == key(&mut reversed, &corpus[j]),
                    legacy[i] == legacy[j]
                );
            }
        }
    }
    #[test]
    fn comparator_bills_only_inspected_bytes_and_polls_empty_input() {
        for (left, right, expected, work) in [
            (&b""[..], &b""[..], Ordering::Equal, 1),
            (&b"abc"[..], &b"abd"[..], Ordering::Less, 7),
            (&b"abc"[..], &b"abcZ"[..], Ordering::Less, 7),
            (&b"aZZZZ"[..], &b"bZZZZ"[..], Ordering::Less, 3),
        ] {
            let mut charged = 0;
            assert_eq!(
                compare_bytes(left, right, &mut |n| {
                    charged += n;
                    Ok::<_, Error>(())
                })
                .unwrap(),
                expected
            );
            assert_eq!(charged, work);
        }
        assert_eq!(
            compare_bytes(b"", b"", &mut |_| Err(Error::Cancel)),
            Err(Error::Cancel)
        );
    }
    #[test]
    fn forced_collision_chain_is_exact_and_failure_atomic() {
        let seed = vec![97; 300];
        let mut candidate = seed.clone();
        candidate.push(98);
        let insert = |table: &mut Interner, key, charge: &mut dyn FnMut(u64) -> Result<(), Error>| {
            table.intern_bucket(key, 7, &mut |n| charge(n), &mut 0)
        };
        let mut baseline = Interner::default();
        assert_eq!(insert(&mut baseline, seed.clone(), &mut |_| Ok(())), Ok(KeyId(0)));
        let mut calls = 0;
        assert_eq!(insert(&mut baseline, candidate.clone(), &mut |_| { calls += 1; Ok(()) }), Ok(KeyId(1)));
        assert_eq!(insert(&mut baseline, seed.clone(), &mut |_| Ok(())), Ok(KeyId(0)));
        assert_eq!(insert(&mut baseline, candidate.clone(), &mut |_| Ok(())), Ok(KeyId(1)));
        for stop in 1..=calls {
            let mut table = Interner::default();
            insert(&mut table, seed.clone(), &mut |_| Ok(())).unwrap();
            let mut index = 0;
            assert_eq!(insert(&mut table, candidate.clone(), &mut |_| {
                index += 1;
                if index == stop { Err(Error::Cancel) } else { Ok(()) }
            }), Err(Error::Cancel), "collision charge point {stop}");
            assert_eq!(table.keys.len(), 1);
            assert_eq!(insert(&mut table, seed.clone(), &mut |_| Ok(())), Ok(KeyId(0)));
            assert_eq!(insert(&mut table, candidate.clone(), &mut |_| Ok(())), Ok(KeyId(1)));
            assert_eq!(insert(&mut table, vec![0], &mut |_| Ok(())), Ok(KeyId(2)));
        }
    }
    #[test]
    fn every_intern_failure_point_preserves_table_and_next_id() {
        let seed = vec![97; 300];
        let mut candidate = seed.clone();
        candidate.push(98);
        let mut baseline = Interner::default();
        baseline
            .intern(seed.clone(), &mut |_| Ok::<_, Error>(()))
            .unwrap();
        let mut calls = 0;
        baseline
            .intern(candidate.clone(), &mut |_| {
                calls += 1;
                Ok::<_, Error>(())
            })
            .unwrap();
        for stop in 1..=calls {
            let mut interner = Interner::default();
            let old = interner
                .intern(seed.clone(), &mut |_| Ok::<_, Error>(()))
                .unwrap();
            let mut index = 0;
            let failed = interner.intern(candidate.clone(), &mut |_| {
                index += 1;
                if index == stop {
                    Err(Error::Cancel)
                } else {
                    Ok(())
                }
            });
            assert_eq!(failed, Err(Error::Cancel), "charge point {stop}");
            assert_eq!(interner.keys.len(), 1);
            assert_eq!(
                interner
                    .intern(seed.clone(), &mut |_| Ok::<_, Error>(()))
                    .unwrap(),
                old
            );
            assert_eq!(
                interner
                    .intern(candidate.clone(), &mut |_| Ok::<_, Error>(()))
                    .unwrap(),
                KeyId(1)
            );
        }
    }
    #[test]
    fn collision_comparison_fuel_and_chain_growth_preserve_ids() {
        let mut table = Interner::default();
        for index in 0..4 {
            assert_eq!(table.intern_bucket(vec![index; 4096], 7, &mut |_| Ok::<_, Error>(()), &mut 0), Ok(KeyId(index as usize)));
        }
        let mut remaining = 100u64;
        let mut compared = 0;
        assert_eq!(table.intern_bucket(vec![0; 4097], 7, &mut |n| {
            remaining = remaining.checked_sub(n).ok_or(Error::Fuel)?;
            Ok(())
        }, &mut compared), Err(Error::Fuel));
        assert!(compared > 0, "must reach exact collision comparison");
        assert_eq!(table.keys.len(), 4);
        let mut calls = 0;
        table.intern_bucket(vec![4; 4096], 7, &mut |_| { calls += 1; Ok::<_, Error>(()) }, &mut 0).unwrap();
        for stop in 1..=calls {
            let mut table = Interner::default();
            for index in 0..4 {
                table.intern_bucket(vec![index; 4096], 7, &mut |_| Ok::<_, Error>(()), &mut 0).unwrap();
            }
            let mut call = 0;
            assert_eq!(table.intern_bucket(vec![4; 4096], 7, &mut |_| {
                call += 1;
                if call == stop { Err(Error::Cancel) } else { Ok(()) }
            }, &mut 0), Err(Error::Cancel));
            assert_eq!(table.keys.len(), 4);
            assert_eq!(table.intern_bucket(vec![4; 4096], 7, &mut |_| Ok::<_, Error>(()), &mut 0), Ok(KeyId(4)));
            for index in 0..5 {
                assert_eq!(table.intern_bucket(vec![index; 4096], 7, &mut |_| Ok::<_, Error>(()), &mut 0), Ok(KeyId(index as usize)));
            }
        }
    }
    #[test]
    fn fuel_exhaustion_in_hashing_does_not_publish() {
        let mut interner = Interner::default();
        interner
            .intern(vec![97; 4096], &mut |_| Ok::<_, Error>(()))
            .unwrap();
        let mut remaining = 100u64;
        let result = interner.intern(vec![97; 4097], &mut |n| {
            remaining = remaining.checked_sub(n).ok_or(Error::Fuel)?;
            Ok(())
        });
        assert_eq!(result, Err(Error::Fuel));
        assert_eq!(interner.keys.len(), 1);
        assert_eq!(
            interner
                .intern(vec![97; 4097], &mut |_| Ok::<_, Error>(()))
                .unwrap(),
            KeyId(1)
        );
    }
    #[test]
    fn deep_plan_has_linear_direct_edges_and_flat_key_storage() {
        let mut value = form(Kind::Nil);
        for _ in 0..256 {
            value = form(Kind::Vector(vec![value]));
        }
        let mut interner = Interner::default();
        let mut plan = Plan::default();
        interner
            .plan(&value, &mut plan, &mut |_| Ok::<_, Error>(()))
            .unwrap();
        assert_eq!(plan.nodes.len(), 257);
        assert_eq!(
            plan.nodes
                .iter()
                .map(|node| node.children.len())
                .sum::<usize>(),
            256
        );
        // Nil uses metadata count (1) and tag (1). Each vector adds those
        // plus collection count (1) and child ID (1 below128, otherwise2).
        // No descendant payload is embedded in parent keys.
        assert_eq!(interner.keys.len(), 257);
        assert_eq!(interner.key_bytes, 2 + 128 * 4 + 128 * 5);
        let mut legacy_bytes = 0;
        let mut cursor = &value;
        loop {
            legacy_bytes += super::super::form_key(cursor).unwrap().len();
            match &cursor.kind {
                Kind::Vector(items) => cursor = &items[0],
                Kind::Nil => break,
                _ => unreachable!(),
            }
        }
        assert_eq!(legacy_bytes, 561_545);
        assert_eq!(interner.key_bytes, 1_154);
        let bytes_before = interner.key_bytes;
        let mut warm = Plan::default();
        let old = key(&mut interner, &value);
        let root = interner
            .plan(&value, &mut warm, &mut |_| Ok::<_, Error>(()))
            .unwrap();
        assert_eq!(warm.nodes[root].key, old);
        assert_eq!(interner.keys.len(), 257);
        assert_eq!(interner.key_bytes, bytes_before);
    }
    #[test]
    fn failed_plan_discards_incomplete_nodes_and_recovers() {
        let value = form(Kind::Vector(vec![
            form(Kind::String(vec![97; 300])),
            form(Kind::Bool(true)),
        ]));
        let mut baseline = Interner::default();
        let mut count = 0;
        baseline
            .plan(&value, &mut Plan::default(), &mut |_| {
                count += 1;
                Ok::<_, Error>(())
            })
            .unwrap();
        for stop in 1..=count {
            let mut interner = Interner::default();
            let mut plan = Plan::default();
            let existing = interner
                .plan(&form(Kind::Nil), &mut plan, &mut |_| Ok::<_, Error>(()))
                .unwrap();
            let old_key = plan.nodes[existing].key;
            let old_len = plan.nodes.len();
            let mut index = 0;
            let result = interner.plan(&value, &mut plan, &mut |_| {
                index += 1;
                if index == stop {
                    Err(Error::Cancel)
                } else {
                    Ok(())
                }
            });
            if result.is_err() {
                assert_eq!(plan.nodes.len(), old_len);
                assert_eq!(plan.nodes[existing].key, old_key);
            }
            let root = interner
                .plan(&value, &mut plan, &mut |_| Ok::<_, Error>(()))
                .unwrap();
            assert_eq!(plan.nodes[root].key, key(&mut interner, &value));
        }
    }
    #[test]
    fn binding_failure_does_not_publish_and_sparse_ids_recover() {
        let mut values = vec![Some(17usize)];
        assert_eq!(
            bind(&mut values, KeyId(100), 42, &mut |_| Err(Error::Fuel)),
            Err(Error::Fuel)
        );
        assert_eq!(values, vec![Some(17)]);
        bind(&mut values, KeyId(100), 42, &mut |_| Ok::<_, Error>(())).unwrap();
        assert_eq!(values[0], Some(17));
        assert_eq!(values[99], None);
        assert_eq!(values[100], Some(42));
    }
    #[test]
    fn paged_entry_boundary_collision_publication_is_atomic_at_all_fuel_irq_cuts() {
        fn primed()->Interner {
            let mut table=Interner::default();
            for i in 0..128usize {
                let bucket=if i<4 {7} else {1000+i as u64};
                assert_eq!(table.intern_bucket((i as u64).to_le_bytes().to_vec(),bucket,&mut |_|Ok::<_,Error>(()),&mut 0),Ok(KeyId(i)));
            }
            table
        }
        fn state(table:&mut Interner)->(Vec<(Vec<u8>,Option<usize>)>,Option<usize>,Option<usize>) {
            (table.keys.iter().map(|e|(e.key.clone(),e.next)).collect(),table.heads.get(7,&mut |_|Ok::<_,Error>(())).unwrap().map(HeadIndex::index),table.heads.get(2000,&mut |_|Ok::<_,Error>(())).unwrap().map(HeadIndex::index))
        }
        let candidate=128u64.to_le_bytes().to_vec();
        for bucket in [7,2000] {
            let mut measured=primed();let mut work=0;let mut calls=0;
            assert_eq!(measured.intern_bucket(candidate.clone(),bucket,&mut |n|{work+=n;calls+=1;Ok::<_,Error>(())},&mut 0),Ok(KeyId(128)));
            for fuel in 0..work {
                let mut table=primed();let before=state(&mut table);let mut remaining=fuel;
                assert_eq!(table.intern_bucket(candidate.clone(),bucket,&mut |n|{remaining=remaining.checked_sub(n).ok_or(Error::Fuel)?;Ok(())},&mut 0),Err(Error::Fuel));
                assert_eq!(state(&mut table),before);
                assert_eq!(table.intern_bucket(candidate.clone(),bucket,&mut |_|Ok::<_,Error>(()),&mut 0),Ok(KeyId(128)));
                assert_eq!(table.intern_bucket(candidate.clone(),bucket,&mut |_|Ok::<_,Error>(()),&mut 0),Ok(KeyId(128)));
                assert_eq!(state(&mut table),state(&mut measured));
            }
            for stop in 1..=calls {
                let mut table=primed();let before=state(&mut table);let mut call=0;
                assert_eq!(table.intern_bucket(candidate.clone(),bucket,&mut |_|{call+=1;if call==stop {Err(Error::Cancel)} else {Ok(())}},&mut 0),Err(Error::Cancel));
                assert_eq!(state(&mut table),before);
                assert_eq!(table.intern_bucket(candidate.clone(),bucket,&mut |_|Ok::<_,Error>(()),&mut 0),Ok(KeyId(128)));
                assert_eq!(state(&mut table),state(&mut measured));
            }
        }
    }

}
