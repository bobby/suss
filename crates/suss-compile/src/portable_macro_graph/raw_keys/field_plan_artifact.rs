//! DEV-ONLY exact BYTE plan framing and privately bound source-field proof.
//!
//! Parent declares this module only under cfg(test). No Graph/prefix/runtime
//! loader or guest API is added. IDs describe a FRESH, phase-specific prefix;
//! they cannot be pasted into a Graph that has already interned generic forms.
//! All meters below are native proxy meters. StopFuel/Irq are injected native
//! surrogates, NOT Session fuel/epoch IRQ observations. Setup bootstrap restore,
//! namespace_scope_for's internal collection, adversarial fixture construction
//! and assertions are outside the reported candidate work. Reported decode,
//! exact source correspondence/replanning, bind and staged reference retirement
//! are separate; no hidden per-request reintern/remap saving is claimed.
use super::{EntryStats, Interner, KeyId, Plan};
use crate::portable::{self, bootstrap, resolve::{DefinitionInfo, Environment, Global, Phase}};
use sha2::{Digest, Sha256};
use std::{mem::size_of, sync::Arc};
use suss_reader::forms::Form;

type Result<T> = std::result::Result<T, Failure>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Failure { Invalid(&'static str), Overflow, StopFuel, Irq }
#[derive(Default)]
struct Meter {
    work: u64,
    calls: usize,
    requested: u64,
    storage: u64, // subset of work, never added to it a second time
    fail: Option<(usize, bool)>,
}
impl Meter {
    fn pay(&mut self, units: u64) -> Result<()> {
        let call = self.calls;
        self.calls = self.calls.checked_add(1).ok_or(Failure::Overflow)?;
        self.requested = self.requested.checked_add(units).ok_or(Failure::Overflow)?;
        if self.fail.is_some_and(|(at, _)| at == call) {
            return Err(if self.fail.unwrap().1 { Failure::Irq } else { Failure::StopFuel });
        }
        self.work = self.work.checked_add(units).ok_or(Failure::Overflow)?;
        Ok(())
    }
    fn allocation(&mut self, units: u64) -> Result<()> {
        self.pay(units)?;
        self.storage = self.storage.checked_add(units).ok_or(Failure::Overflow)?;
        Ok(())
    }
}
fn units(n: usize) -> Result<u64> { u64::try_from(n).map_err(|_| Failure::Overflow) }
fn extent<T>(n: usize) -> Result<u64> {
    units(n.checked_mul(size_of::<T>()).ok_or(Failure::Overflow)?)
}
// Same precharged geometric native Vec allocation/relocation convention as
// raw_keys::reserve; checked arithmetic and fallible allocation in this proof.
fn room<T>(v: &mut Vec<T>, extra: usize, m: &mut Meter) -> Result<()> {
    m.pay(0)?;
    if extra > v.capacity() - v.len() {
        let cap = v.len().checked_add(extra).ok_or(Failure::Overflow)?
            .max(v.capacity().checked_mul(2).ok_or(Failure::Overflow)?).max(4);
        m.allocation(extent::<T>(cap)?.checked_add(extent::<T>(v.len())?).ok_or(Failure::Overflow)?)?;
        v.try_reserve_exact(cap - v.len()).map_err(|_| Failure::Invalid("allocation"))?;
    }
    Ok(())
}
fn push<T>(v: &mut Vec<T>, x: T, m: &mut Meter) -> Result<()> {
    m.pay(1)?; room(v, 1, m)?; v.push(x); Ok(())
}
fn copy(data: &[u8], m: &mut Meter) -> Result<Vec<u8>> {
    let mut v = Vec::new(); room(&mut v, data.len(), m)?;
    for chunk in data.chunks(256) {
        m.pay(units(chunk.len())?.checked_mul(2).ok_or(Failure::Overflow)?)?;
        v.extend_from_slice(chunk);
    }
    Ok(v)
}
fn equal(a: &[u8], b: &[u8], m: &mut Meter) -> Result<bool> {
    // Never use digest equality or bucket identity as byte equality.
    Ok(super::compare_bytes(a, b, &mut |n| m.pay(n))? == std::cmp::Ordering::Equal)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Selector { Definition, Declaration, Initializer }
impl Selector {
    const ALL: [Self; 3] = [Self::Definition, Self::Declaration, Self::Initializer];
    fn index(self) -> usize { match self { Self::Definition => 0, Self::Declaration => 1, Self::Initializer => 2 } }
    fn from(n: usize) -> Result<Self> {
        Self::ALL.get(n).copied().ok_or(Failure::Invalid("selector"))
    }
    fn field(self, owner: &DefinitionInfo) -> Option<&Form> {
        match self { Self::Definition => Some(&owner.definition_form),
            Self::Declaration => Some(&owner.declaration), Self::Initializer => owner.initializer_form.as_ref() }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Identity { phase: Phase, source: [u8; 32], compiler: [u8; 32], dependency: [u8; 32] }
fn digest(data: &[u8], m: &mut Meter) -> Result<[u8; 32]> {
    let mut digest = Sha256::new();
    for chunk in data.chunks(256) { m.pay(units(chunk.len())?.checked_mul(3).ok_or(Failure::Overflow)?)?; digest.update(chunk); }
    m.pay(32)?; Ok(digest.finalize().into())
}
fn identity(phase: Phase, m: &mut Meter) -> Result<Identity> {
    let mut compiler = [0; 32];
    let text = env!("SUSS_COMPILER_SOURCE_SHA256").as_bytes();
    if text.len() != 64 { return Err(Failure::Invalid("compiler digest")); }
    for (i, out) in compiler.iter_mut().enumerate() {
        m.pay(4)?;
        fn hex(b: u8) -> Result<u8> { match b { b'0'..=b'9' => Ok(b-b'0'), b'a'..=b'f' => Ok(b-b'a'+10), _ => Err(Failure::Invalid("compiler digest")) } }
        *out = hex(text[i*2])? * 16 + hex(text[i*2+1])?;
    }
    Ok(Identity { phase, compiler, source: digest(bootstrap::SOURCE.as_bytes(), m)?,
        dependency: digest(include_bytes!("../../../../../runtime/core-import/manifest.json"), m)? })
}
struct NativeOwner<'a> { ordinal: usize, global: &'a Global, owner: &'a Arc<DefinitionInfo> }
struct Catalog<'a> { phase: Phase, env: &'a Environment, owners: Vec<NativeOwner<'a>> }
fn catalog<'a>(phase: Phase, env: &'a Environment, m: &mut Meter) -> Result<Catalog<'a>> {
    // Public scope materialization itself is existing unmetered fixture input
    // discovery, not included in any reported plan/decode/bind saving.
    let scope = env.namespace_scope_for(phase, "suss.core").ok_or(Failure::Invalid("namespace"))?;
    let mut owners = Vec::new();
    for (ordinal, (global, info)) in scope.declarations.iter().enumerate() {
        m.pay(4)?;
        if !info.analysis_completed { continue; }
        let owner = env.shared_definition_info(global).ok_or(Failure::Invalid("owner"))?;
        if global.phase() != phase {
            return Err(Failure::Invalid("phase"));
        }
        if !std::ptr::eq(*info, owner.as_ref()) { return Err(Failure::Invalid("source borrow")); }
        push(&mut owners, NativeOwner { ordinal, global, owner }, m)?;
    }
    Ok(Catalog { phase, env, owners })
}
#[derive(Clone, Debug)]
struct Owner { ordinal: usize, namespace: Vec<u8>, name: Vec<u8>, mask: u8 }
#[derive(Clone, Debug, PartialEq, Eq)]
struct Row { key: usize, metadata: usize, children: Vec<usize> }
#[derive(Clone, Debug)]
struct Field { owner: usize, selector: Selector, root: usize, rows: Vec<Row> }
#[derive(Clone, Debug)]
struct Artifact { identity: Identity, keys: Vec<Vec<u8>>, owners: Vec<Owner>, fields: Vec<Field> }
fn rows(plan: &Plan, m: &mut Meter) -> Result<Vec<Row>> {
    let mut rows = Vec::new(); room(&mut rows, plan.nodes.len(), m)?;
    for n in &plan.nodes {
        m.pay(3)?;
        let mut children = Vec::new(); room(&mut children, n.children.len(), m)?;
        for child in &n.children { m.pay(2)?; children.push(*child); }
        rows.push(Row { key: n.key.0, metadata: n.metadata_count, children });
    }
    Ok(rows)
}
// The ORIGINAL byte planner, canonical ID order and complete Plan are the oracle.
// Snapshot/retained native storage is measured separately from plan+clear.
fn original(c: &Catalog<'_>, id: Identity, planning: &mut Meter, storage: &mut Meter)
    -> Result<(Artifact, Interner, Plan)>
{
    let mut a = Artifact { identity: id, keys: Vec::new(), owners: Vec::new(), fields: Vec::new() };
    let mut keys = Interner::default(); let mut plan = Plan::default();
    for (index, native) in c.owners.iter().enumerate() {
        storage.pay(4)?;
        let mask = if native.owner.initializer_form.is_some() { 7 } else { 3 };
        let owner = Owner { ordinal: native.ordinal, namespace: copy(native.global.namespace().as_bytes(), storage)?,
            name: copy(native.global.name().as_bytes(), storage)?, mask };
        push(&mut a.owners, owner, storage)?;
        for selector in Selector::ALL {
            storage.pay(1)?;
            if let Some(form) = selector.field(native.owner) {
                let root = keys.plan(form, &mut plan, &mut |n| planning.pay(n))?;
                push(&mut a.fields, Field { owner: index, selector, root, rows: rows(&plan, storage)? }, storage)?;
                plan.clear_metered(&mut |n| planning.pay(n))?;
            }
        }
    }
    room(&mut a.keys, keys.keys.len(), storage)?;
    let mut stats = EntryStats::default();
    for index in 0..keys.keys.len() {
        let entry = keys.keys.get(index, &mut |n| storage.pay(n), &mut stats)?;
        let bytes = copy(&entry.key, storage)?; push(&mut a.keys, bytes, storage)?;
    }
    Ok((a, keys, plan))
}
fn compare_rows(a: &[Row], plan: &Plan, m: &mut Meter) -> Result<()> {
    m.pay(1)?;
    if a.len() != plan.nodes.len() { return Err(Failure::Invalid("row count")); }
    for (a, b) in a.iter().zip(&plan.nodes) {
        m.pay(4)?;
        if a.key != b.key.0 || a.metadata != b.metadata_count || a.children.len() != b.children.len() {
            return Err(Failure::Invalid("row"));
        }
        for (a, b) in a.children.iter().zip(&b.children) { m.pay(2)?; if a != b { return Err(Failure::Invalid("child order")); } }
    }
    Ok(())
}
fn replan(c: &Catalog<'_>, a: &Artifact, keys: &mut Interner, plan: &mut Plan, m: &mut Meter)
    -> Result<()>
{
    let mut next = 0;
    for (index, native) in c.owners.iter().enumerate() {
        m.pay(4)?;
        let owner = a.owners.get(index).ok_or(Failure::Invalid("missing owner"))?;
        if owner.ordinal != native.ordinal || !equal(&owner.namespace, native.global.namespace().as_bytes(), m)?
            || !equal(&owner.name, native.global.name().as_bytes(), m)?
            || owner.mask != if native.owner.initializer_form.is_some() {7} else {3} { return Err(Failure::Invalid("owner correspondence")); }
        for selector in Selector::ALL {
            m.pay(1)?;
            if let Some(form) = selector.field(native.owner) {
                let f = a.fields.get(next).ok_or(Failure::Invalid("missing field"))?;
                m.pay(3)?;
                if f.owner != index || f.selector != selector { return Err(Failure::Invalid("field correspondence")); }
                let root = keys.plan(form, plan, &mut |n| m.pay(n))?;
                if root != f.root { return Err(Failure::Invalid("root correspondence")); }
                compare_rows(&f.rows, plan, m)?;
                plan.clear_metered(&mut |n| m.pay(n))?; next += 1;
            }
        }
    }
    if next != a.fields.len() || a.owners.len() != c.owners.len() || keys.keys.len() != a.keys.len() {
        return Err(Failure::Invalid("catalog extent"));
    }
    let mut stats = EntryStats::default();
    for (index, bytes) in a.keys.iter().enumerate() {
        let entry = keys.keys.get(index, &mut |n| m.pay(n), &mut stats)?;
        if !equal(bytes, &entry.key, m)? { return Err(Failure::Invalid("key ID/bytes")); }
    }
    Ok(())
}
// Canonical framing of original bytes/rows, NOT a replacement semantic codec.
fn integer(out: &mut Vec<u8>, value: usize, m: &mut Meter) -> Result<()> {
    super::size(out, value, &mut |n| m.pay(n))
}
fn blob(out: &mut Vec<u8>, value: &[u8], m: &mut Meter) -> Result<()> {
    integer(out, value.len(), m)?; super::append(out, value, &mut |n| m.pay(n))
}
fn encode(a: &Artifact, m: &mut Meter) -> Result<Vec<u8>> {
    let mut out = Vec::new(); super::append(&mut out, b"SFP1", &mut |n| m.pay(n))?;
    integer(&mut out, 1, m)?; integer(&mut out, if a.identity.phase == Phase::Runtime {0} else {1}, m)?;
    for digest in [&a.identity.source, &a.identity.compiler, &a.identity.dependency] {
        super::append(&mut out, digest, &mut |n| m.pay(n))?;
    }
    integer(&mut out, a.keys.len(), m)?;
    for key in &a.keys { m.pay(1)?; blob(&mut out, key, m)?; }
    integer(&mut out, a.owners.len(), m)?;
    for o in &a.owners { m.pay(4)?; integer(&mut out, o.ordinal, m)?; blob(&mut out, &o.namespace, m)?; blob(&mut out, &o.name, m)?; integer(&mut out, o.mask as usize, m)?; }
    integer(&mut out, a.fields.len(), m)?;
    for f in &a.fields {
        m.pay(4)?; integer(&mut out, f.owner, m)?; integer(&mut out, f.selector.index(), m)?;
        integer(&mut out, f.root, m)?; integer(&mut out, f.rows.len(), m)?;
        for row in &f.rows {
            m.pay(3)?; integer(&mut out, row.key, m)?; integer(&mut out, row.metadata, m)?; integer(&mut out, row.children.len(), m)?;
            for child in &row.children { m.pay(1)?; integer(&mut out, *child, m)?; }
        }
    }
    m.pay(1)?; Ok(out)
}
struct Reader<'a> { bytes: &'a [u8], at: usize }
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize, m: &mut Meter) -> Result<&'a [u8]> {
        m.pay(2)?;
        let end = self.at.checked_add(n).ok_or(Failure::Overflow)?;
        let value = self.bytes.get(self.at..end).ok_or(Failure::Invalid("truncated"))?;
        for chunk in value.chunks(256) { m.pay(units(chunk.len())?)?; }
        self.at = end; Ok(value)
    }
    fn byte(&mut self, m: &mut Meter) -> Result<u8> { Ok(self.take(1, m)?[0]) }
    fn integer(&mut self, m: &mut Meter) -> Result<usize> {
        let mut v = 0u64;
        for shift in (0..70).step_by(7) {
            m.pay(6)?; let b = self.byte(m)?;
            if shift == 63 && b & 0xfe != 0 { return Err(Failure::Overflow); }
            v |= u64::from(b & 127) << shift;
            if b & 128 == 0 {
                if shift != 0 && b == 0 { return Err(Failure::Invalid("noncanonical integer")); }
                return usize::try_from(v).map_err(|_| Failure::Overflow);
            }
        }
        Err(Failure::Overflow)
    }
    fn blob(&mut self, m: &mut Meter) -> Result<Vec<u8>> {
        let n = self.integer(m)?; let bytes = self.take(n, m)?; copy(bytes, m)
    }
    fn extent(&self, count: usize, min_bytes: usize, m: &mut Meter) -> Result<()> {
        m.pay(2)?;
        let min = count.checked_mul(min_bytes).ok_or(Failure::Overflow)?;
        if min > self.bytes.len() - self.at { return Err(Failure::Invalid("impossible count")); } Ok(())
    }
    fn end(&self, m: &mut Meter) -> Result<()> {
        m.pay(1)?; if self.at != self.bytes.len() { Err(Failure::Invalid("trailing bytes")) } else { Ok(()) }
    }
}
struct Shape { metadata: usize, children: Vec<usize> }
fn key_shape(bytes: &[u8], id: usize, m: &mut Meter) -> Result<Shape> {
    let mut r = Reader { bytes, at: 0 }; let metadata = r.integer(m)?;
    r.extent(metadata, 1, m)?; let mut children = Vec::new();
    for _ in 0..metadata { let key = r.integer(m)?; if key >= id { return Err(Failure::Invalid("key metadata domain")); } push(&mut children, key, m)?; }
    let tag = r.byte(m)?;
    let count = match tag {
        0 => 0,
        1 => { if r.byte(m)? > 1 { return Err(Failure::Invalid("boolean")); } 0 }
        2 => { r.take(8, m)?; 0 } // preserve ALL f64 bits, including NaN/-0
        3 => { let n = r.integer(m)?.checked_mul(2).ok_or(Failure::Overflow)?; r.take(n, m)?; 0 }
        4 | 5 => {
            let ns = r.byte(m)?; if ns > 1 { return Err(Failure::Invalid("identifier")); }
            for _ in 0..(1 + usize::from(ns)) {
                let n = r.integer(m)?; let text = r.take(n, m)?;
                m.pay(units(text.len())?)?;
                std::str::from_utf8(text).map_err(|_| Failure::Invalid("utf8"))?;
            }
            0
        }
        6..=10 | 13 => r.integer(m)?,
        11 => 1,
        12 => 2,
        _ => return Err(Failure::Invalid("key tag")),
    };
    r.extent(count, 1, m)?;
    for _ in 0..count { let key = r.integer(m)?; if key >= id { return Err(Failure::Invalid("key child domain")); } push(&mut children, key, m)?; }
    r.end(m)?; Ok(Shape { metadata, children })
}
fn validate(a: &Artifact, m: &mut Meter) -> Result<()> {
    m.pay(3)?;
    if a.owners.is_empty() || a.fields.is_empty() || a.keys.is_empty() { return Err(Failure::Invalid("empty catalog")); }
    let mut shapes = Vec::new(); let mut uniqueness = Interner::default();
    for (id, key) in a.keys.iter().enumerate() {
        let shape = key_shape(key, id, m)?;
        // This deliberately pays for an independent original byte index and a
        // copy. It is validation cost, not a hidden free prefix install/remap.
        let bytes = copy(key, m)?;
        if uniqueness.intern(bytes, &mut |n| m.pay(n))?.0 != id { return Err(Failure::Invalid("duplicate key ID")); }
        push(&mut shapes, shape, m)?;
    }
    let mut expected_fields = 0usize;
    for (i, o) in a.owners.iter().enumerate() {
        m.pay(4)?;
        if o.mask != 3 && o.mask != 7 { return Err(Failure::Invalid("presence")); }
        if !equal(&o.namespace, b"suss.core", m)? || o.name.is_empty() { return Err(Failure::Invalid("Global")); }
        m.pay(units(o.name.len())?)?;
        std::str::from_utf8(&o.name).map_err(|_| Failure::Invalid("global utf8"))?;
        if i != 0 {
            let prev = &a.owners[i-1];
            if prev.ordinal >= o.ordinal || super::compare_bytes(&prev.name, &o.name, &mut |n| m.pay(n))? != std::cmp::Ordering::Less {
                return Err(Failure::Invalid("owner order/duplicate"));
            }
        }
        expected_fields = expected_fields.checked_add(if o.mask == 7 {3} else {2}).ok_or(Failure::Overflow)?;
    }
    if expected_fields != a.fields.len() { return Err(Failure::Invalid("missing/extra field")); }
    let mut cursor = 0;
    for (owner, desc) in a.owners.iter().enumerate() {
        for selector in Selector::ALL {
            m.pay(2)?;
            if desc.mask & (1 << selector.index()) == 0 { continue; }
            let f = &a.fields[cursor]; cursor += 1;
            if f.owner != owner || f.selector != selector { return Err(Failure::Invalid("field order/duplicate")); }
            if f.root != 0 || f.rows.is_empty() { return Err(Failure::Invalid("root")); }
            let mut stack = Vec::new(); push(&mut stack, f.root, m)?; let mut next = 0;
            while let Some(index) = stack.pop() {
                m.pay(4)?;
                // Native Plan is a complete preorder occurrence tree, not a
                // semantic-key DAG. Reused child indexes are not occurrences.
                if index != next || index >= f.rows.len() { return Err(Failure::Invalid("cycle/disconnected/duplicate occurrence")); }
                next += 1; let row = &f.rows[index];
                let shape = shapes.get(row.key).ok_or(Failure::Invalid("KeyId domain"))?;
                if row.metadata != shape.metadata || row.children.len() != shape.children.len() { return Err(Failure::Invalid("metadata/edge count")); }
                for (child, key) in row.children.iter().zip(&shape.children) {
                    m.pay(3)?;
                    if *child <= index || f.rows.get(*child).is_none_or(|r| r.key != *key) { return Err(Failure::Invalid("child index/key domain")); }
                }
                for child in row.children.iter().rev() { push(&mut stack, *child, m)?; }
            }
            if next != f.rows.len() { return Err(Failure::Invalid("disconnected rows")); }
        }
    }
    Ok(())
}
fn decode(bytes: &[u8], expected: Identity, m: &mut Meter) -> Result<Artifact> {
    let mut r = Reader { bytes, at: 0 };
    if !equal(r.take(4, m)?, b"SFP1", m)? || r.integer(m)? != 1 { return Err(Failure::Invalid("version")); }
    let phase = match r.integer(m)? {0 => Phase::Runtime,1 => Phase::Macro,_ => return Err(Failure::Invalid("phase"))};
    if phase != expected.phase { return Err(Failure::Invalid("phase")); }
    for hash in [&expected.source, &expected.compiler, &expected.dependency] {
        if !equal(r.take(32, m)?, hash, m)? { return Err(Failure::Invalid("provenance")); }
    }
    let mut a = Artifact { identity: expected, keys: Vec::new(), owners: Vec::new(), fields: Vec::new() };
    let n = r.integer(m)?; r.extent(n, 1, m)?;
    for _ in 0..n { let key = r.blob(m)?; push(&mut a.keys, key, m)?; }
    let n = r.integer(m)?; r.extent(n, 4, m)?;
    for _ in 0..n {
        let ordinal = r.integer(m)?; let namespace = r.blob(m)?; let name = r.blob(m)?;
        let mask = u8::try_from(r.integer(m)?).map_err(|_| Failure::Invalid("presence"))?;
        push(&mut a.owners, Owner { ordinal, namespace, name, mask }, m)?;
    }
    let n = r.integer(m)?; r.extent(n, 4, m)?;
    for _ in 0..n {
        let owner = r.integer(m)?; let selector = Selector::from(r.integer(m)?)?;
        let root = r.integer(m)?; let count = r.integer(m)?; r.extent(count, 3, m)?;
        let mut rows = Vec::new();
        for _ in 0..count {
            let key = r.integer(m)?; let metadata = r.integer(m)?; let count = r.integer(m)?; r.extent(count, 1, m)?;
            let mut children = Vec::new();
            for _ in 0..count { let child = r.integer(m)?; push(&mut children, child, m)?; }
            push(&mut rows, Row { key, metadata, children }, m)?;
        }
        push(&mut a.fields, Field { owner, selector, root, rows }, m)?;
    }
    r.end(m)?; validate(&a, m)?; m.pay(1)?; Ok(a)
}

// Decoder storage is not authority. This private proof requires a COMPLETE
// original replan against current actual source; its cost is reported separately.
// A future runtime trusted-install correspondence must replace that work only
// through independently reviewed evidence, not silently call this every request.
struct Correspondence<'a, 'c> { artifact: Arc<Artifact>, catalog: &'c Catalog<'a> }
fn correspondence<'a, 'c>(a: Arc<Artifact>, c: &'c Catalog<'a>, m: &mut Meter)
    -> Result<Correspondence<'a, 'c>>
{
    m.pay(2)?;
    if a.identity.phase != c.phase { return Err(Failure::Invalid("source phase")); }
    let mut interner = Interner::default(); let mut plan = Plan::default();
    replan(c, &a, &mut interner, &mut plan, m)?;
    m.pay(1)?; Ok(Correspondence { artifact: a, catalog: c })
}
enum Origin<'a> { PrivateShipped(&'a Environment), PublicRestore(&'a Environment) }
struct Association<'a> {
    owner: Arc<DefinitionInfo>, global: &'a Global, selector: Selector,
    source: &'a Form, field: usize,
}
struct Installed<'a> {
    artifact: Arc<Artifact>, phase: Phase, associations: Vec<Association<'a>>,
    by_owner: Vec<[Option<usize>; 3]>,
}
fn bind<'a>(proof: &Correspondence<'a, '_>, origin: Origin<'_>, m: &mut Meter) -> Result<Installed<'a>> {
    m.pay(3)?;
    let env = match origin {
        Origin::PrivateShipped(env) => env,
        Origin::PublicRestore(env) => { let _ = env; return Err(Failure::Invalid("public restore has no private install capability")); }
    };
    let c = proof.catalog;
    // Not cells-name/digest equality: only this actual shipped catalog instance
    // may supply this test-only install. Environment clones can still LOOK UP
    // their unchanged genuine Arcs after installation.
    let shipped = bootstrap::shipped(c.phase).map_err(|_| Failure::Invalid("shipped catalog"))?;
    if !std::ptr::eq(env, c.env) || !std::ptr::eq(env, &shipped.environment) { return Err(Failure::Invalid("foreign installation origin")); }
    let count = c.owners.last().map_or(Ok(0), |n| n.ordinal.checked_add(1).ok_or(Failure::Overflow))?;
    let mut by_owner = Vec::new(); room(&mut by_owner, count, m)?;
    m.pay(units(count)?.checked_mul(3).ok_or(Failure::Overflow)?)?;
    by_owner.resize(count, [None;3]);
    let mut associations = Vec::new();
    for (index, f) in proof.artifact.fields.iter().enumerate() {
        m.pay(5)?;
        let n = c.owners.get(f.owner).ok_or(Failure::Invalid("missing association"))?;
        let source = f.selector.field(n.owner).ok_or(Failure::Invalid("absent association"))?;
        let slot = &mut by_owner[n.ordinal][f.selector.index()];
        if slot.is_some() { return Err(Failure::Invalid("duplicate association")); }
        room(&mut associations, 1, m)?; m.pay(4)?;
        *slot = Some(associations.len());
        associations.push(Association { owner: n.owner.clone(), global: n.global,
            selector: f.selector, source, field: index });
    }
    m.pay(3)?;
    Ok(Installed { artifact: proof.artifact.clone(), phase: c.phase, associations, by_owner })
}
impl Installed<'_> {
    fn lookup<'a>(&'a self, ordinal: usize, phase: Phase, global: &Global,
        owner: &Arc<DefinitionInfo>, selector: Selector, field: &Form, m: &mut Meter)
        -> Result<Option<(&'a Field, &'a Form)>>
    {
        m.pay(8)?;
        if phase != self.phase || global.phase() != phase { return Ok(None); }
        let Some(index) = self.by_owner.get(ordinal).and_then(|row| row[selector.index()]) else { return Ok(None); };
        let a = &self.associations[index];
        if !Arc::ptr_eq(owner, &a.owner) || selector != a.selector
            || !std::ptr::eq(a.source, field)
            || selector.field(owner).is_none_or(|actual| !std::ptr::eq(actual, field)) { return Ok(None); }
        if !equal(a.global.namespace().as_bytes(), global.namespace().as_bytes(), m)?
            || !equal(a.global.name().as_bytes(), global.name().as_bytes(), m)? { return Ok(None); }
        Ok(Some((&self.artifact.fields[a.field], a.source)))
    }
}
fn shared(a: Artifact, m: &mut Meter) -> Result<Arc<Artifact>> {
    m.allocation(extent::<Artifact>(1)?.checked_add(extent::<usize>(2)?).ok_or(Failure::Overflow)?)?;
    m.pay(1)?; Ok(Arc::new(a))
}
fn publish<'a>(slot: &mut Option<Installed<'a>>, candidate: Installed<'a>, m: &mut Meter) -> Result<()> {
    m.pay(1)?; *slot = Some(candidate); Ok(())
}
fn retire(slot: &mut Option<Installed<'_>>, m: &mut Meter) -> Result<()> {
    let n = slot.as_ref().map_or(0, |s| s.associations.len());
    // Atomic staged retirement of association references, while the fixture's
    // actual catalog and shared artifact still own native/decoded graphs.
    // NOT full recursive last-owner Drop accounting; early-error Drop follows
    // existing native cleanup policy and is explicitly outside accepted charges.
    m.pay(units(n)?.checked_mul(4).and_then(|n| n.checked_add(1)).ok_or(Failure::Overflow)?)?;
    drop(slot.take()); Ok(())
}
fn clear_flat(a: &mut Artifact, m: &mut Meter) -> Result<()> {
    // Explicit normal artifact buffer cleanup, charged before each bounded pop.
    // Does not claim to meter system allocator internals or last Form/owner Drop.
    while let Some(f) = a.fields.last_mut() {
        while !f.rows.is_empty() { m.pay(4)?; f.rows.pop(); }
        m.pay(4)?; a.fields.pop();
    }
    while !a.keys.is_empty() { m.pay(4)?; a.keys.pop(); }
    while !a.owners.is_empty() { m.pay(4)?; a.owners.pop(); }
    m.pay(0)
}
fn warm_original(c: &Catalog<'_>, a: &Artifact, keys: &mut Interner, plan: &mut Plan,
    planning: &mut Meter, comparisons: &mut Meter) -> Result<()>
{
    let mut index = 0;
    for owner in &c.owners {
        for selector in Selector::ALL {
            comparisons.pay(1)?;
            if let Some(form) = selector.field(owner.owner) {
                let f = &a.fields[index];
                let root = keys.plan(form, plan, &mut |n| planning.pay(n))?;
                if root != f.root { return Err(Failure::Invalid("warm root")); }
                compare_rows(&f.rows, plan, comparisons)?;
                plan.clear_metered(&mut |n| planning.pay(n))?; index += 1;
            }
        }
    }
    if index != a.fields.len() || keys.keys.len() != a.keys.len() { return Err(Failure::Invalid("warm extent")); }
    let mut stats = EntryStats::default();
    for (id, bytes) in a.keys.iter().enumerate() {
        let entry = keys.keys.get(id, &mut |n| comparisons.pay(n), &mut stats)?;
        if !equal(bytes, &entry.key, comparisons)? { return Err(Failure::Invalid("warm key table")); }
    }
    Ok(())
}
fn compare_artifacts(a: &Artifact, b: &Artifact, m: &mut Meter) -> Result<()> {
    m.pay(4)?;
    if a.identity != b.identity || a.keys.len() != b.keys.len()
        || a.owners.len() != b.owners.len() || a.fields.len() != b.fields.len() { return Err(Failure::Invalid("artifact extent")); }
    for (a,b) in a.keys.iter().zip(&b.keys) { if !equal(a,b,m)? { return Err(Failure::Invalid("artifact byte")); } }
    for (a,b) in a.owners.iter().zip(&b.owners) {
        m.pay(3)?;
        if a.ordinal != b.ordinal || a.mask != b.mask || !equal(&a.namespace,&b.namespace,m)? || !equal(&a.name,&b.name,m)? { return Err(Failure::Invalid("artifact owner")); }
    }
    for (a,b) in a.fields.iter().zip(&b.fields) {
        m.pay(4)?;
        if a.owner != b.owner || a.selector != b.selector || a.root != b.root || a.rows.len() != b.rows.len() { return Err(Failure::Invalid("artifact field")); }
        for (a,b) in a.rows.iter().zip(&b.rows) {
            m.pay(4)?;
            if a.key != b.key || a.metadata != b.metadata || a.children.len() != b.children.len() { return Err(Failure::Invalid("artifact row")); }
            for (a,b) in a.children.iter().zip(&b.children) { m.pay(2)?; if a != b { return Err(Failure::Invalid("artifact child")); } }
        }
    }
    Ok(())
}
#[test]
fn completed_catalog_field_artifact_roundtrip_and_owner_binding() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let prepared = bootstrap::shipped(phase).unwrap();
        let mut setup = Meter::default(); let id = identity(phase, &mut setup).unwrap();
        let c = catalog(phase, &prepared.environment, &mut setup).unwrap();
        assert_eq!(c.owners.len(),615);
        let mut cold = Meter::default(); let mut snapshot = Meter::default();
        let (a, mut table, mut scratch) = original(&c,id,&mut cold,&mut snapshot).unwrap();
        assert_eq!(a.fields.len(),1577);
        let mut warm = Meter::default(); let mut verify = Meter::default();
        warm_original(&c,&a,&mut table,&mut scratch,&mut warm,&mut verify).unwrap();
        let mut second_cold = Meter::default(); let mut second_storage = Meter::default();
        let (second, _, _) = original(&c,id,&mut second_cold,&mut second_storage).unwrap();
        compare_artifacts(&a,&second,&mut verify).unwrap();
        assert_eq!(cold.work,second_cold.work,"independent original interner has exact first-seen work/IDs");
        let mut enc = Meter::default(); let bytes = encode(&a,&mut enc).unwrap();
        let mut dec = Meter::default(); let decoded = decode(&bytes,id,&mut dec).unwrap();
        compare_artifacts(&a,&decoded,&mut verify).unwrap();
        let mut sharing = Meter::default(); let decoded = shared(decoded,&mut sharing).unwrap();
        let mut correspondence_work = Meter::default();
        let proof = correspondence(decoded.clone(),&c,&mut correspondence_work).unwrap();
        let mut binding = Meter::default();
        let candidate = bind(&proof,Origin::PrivateShipped(&prepared.environment),&mut binding).unwrap();
        let mut publication = Meter::default(); let mut installed = None;
        publish(&mut installed,candidate,&mut publication).unwrap();
        let mut lookup = Meter::default(); let mut reads = Meter::default();
        for owner in &c.owners {
            for selector in Selector::ALL {
                if let Some(form) = selector.field(owner.owner) {
                    let (f, original_borrow) = installed.as_ref().unwrap().lookup(owner.ordinal,phase,owner.global,owner.owner,selector,form,&mut lookup).unwrap().unwrap();
                    assert!(std::ptr::eq(original_borrow,form),"ALL original spans/metadata/bits remain the actual borrowed field");
                    for row in &f.rows { reads.pay(3).unwrap(); for _ in &row.children { reads.pay(1).unwrap(); } }
                }
            }
        }
        let mut cleanup = Meter::default(); retire(&mut installed,&mut cleanup).unwrap(); assert!(installed.is_none());
        // a is a separately owned snapshot, so cleanup does not free another
        // installation's data. References to the actual shipped owners stay live.
        let keys=a.keys.len(); let fields=a.fields.len(); let mut a=a; clear_flat(&mut a,&mut cleanup).unwrap();
        println!("field-plan-artifact phase={phase:?} completed={} fields={fields} keys={keys} bytes={} cold_plan_clear={} warm_plan_clear={} snapshot_storage={} encode={} decode_validate_storage={} decode_backing_subset={} shared_handle={} exact_correspondence_replan={} bind={} publication={} lookup={} template_reads={} normal_reference_and_flat_cleanup={} control_comparisons={} source_identity_and_input_setup={} acceptance=false runtime_win=UNKNOWN remap=NONE prefix=fresh-native-only",c.owners.len(),bytes.len(),cold.work,warm.work,snapshot.work,enc.work,dec.work,dec.storage,sharing.work,correspondence_work.work,binding.work,publication.work,lookup.work,reads.work,cleanup.work,verify.work,setup.work);
    }
}
// Bounded fixture from the SAME genuine shipped catalog; no raw525 substitute.
fn bounded<'a>(c: &Catalog<'a>) -> Catalog<'a> {
    let n = c.owners.iter().find(|n| n.global.name()=="string-hash-cache-count").unwrap();
    Catalog { phase:c.phase, env:c.env, owners:vec![NativeOwner { ordinal:n.ordinal,global:n.global,owner:n.owner }] }
}
fn fixture(c: &Catalog<'_>, id: Identity) -> Artifact {
    original(c,id,&mut Meter::default(),&mut Meter::default()).unwrap().0
}
#[test]
fn artifact_decoder_rejects_shape_identity_and_occurrence_corruption() {
    let env=&bootstrap::shipped(Phase::Runtime).unwrap().environment;
    let c=catalog(Phase::Runtime,env,&mut Meter::default()).unwrap();let c=bounded(&c);
    let id=identity(c.phase,&mut Meter::default()).unwrap();let a=fixture(&c,id);
    let bytes=encode(&a,&mut Meter::default()).unwrap();
    for n in 0..bytes.len() { assert!(decode(&bytes[..n],id,&mut Meter::default()).is_err(),"truncation{n}"); }
    let mut bad=bytes.clone();bad.push(0);assert!(decode(&bad,id,&mut Meter::default()).is_err());
    for offset in [0,4,5,6,38,70] { let mut b=bytes.clone();b[offset]^=1;assert!(decode(&b,id,&mut Meter::default()).is_err(),"header{offset}"); }
    let mut bad=bytes[..102].to_vec();bad.extend_from_slice(&[255;10]);bad.push(0);assert!(decode(&bad,id,&mut Meter::default()).is_err());
    let mut variants=Vec::new();
    let mut b=a.clone();b.fields.pop();variants.push(b);
    let mut b=a.clone();b.fields.insert(0,b.fields[0].clone());variants.push(b);
    let mut b=a.clone();b.fields[0].owner=usize::MAX;variants.push(b);
    let mut b=a.clone();b.fields[0].root=usize::MAX;variants.push(b);
    let mut b=a.clone();b.fields[0].rows[0].key=usize::MAX;variants.push(b);
    let mut b=a.clone();b.fields[0].rows[0].metadata=usize::MAX;variants.push(b);
    let mut b=a.clone();b.fields[0].rows[0].children[0]=0;variants.push(b);
    let mut b=a.clone();b.fields[0].rows[0].children[0]=usize::MAX;variants.push(b);
    let mut b=a.clone();let extra=b.fields[0].rows[0].clone();b.fields[0].rows.push(extra);variants.push(b);
    let mut b=a.clone();let duplicate=b.fields[0].rows[0].children[0];b.fields[0].rows[0].children[1]=duplicate;variants.push(b);
    let mut b=a.clone();b.owners[0].mask=0;variants.push(b);
    let mut b=a.clone();b.keys.push(b.keys[0].clone());variants.push(b);
    let mut b=a.clone();b.keys[0].push(0);variants.push(b);
    let mut b=a.clone();b.keys[0]=vec![0,255];variants.push(b);
    for (i,b) in variants.iter().enumerate() { let encoded=encode(b,&mut Meter::default()).unwrap();assert!(decode(&encoded,id,&mut Meter::default()).is_err(),"malformation{i}"); }
    // Invalid selector cannot be represented by the typed candidate; corrupt its
    // actual framing byte instead. Setup parsing is outside measured decode.
    let mut m=Meter::default();let mut r=Reader{bytes:&bytes,at:0};r.take(102,&mut m).unwrap();
    for _ in 0..r.integer(&mut m).unwrap() {r.blob(&mut m).unwrap();}
    for _ in 0..r.integer(&mut m).unwrap() {r.integer(&mut m).unwrap();r.blob(&mut m).unwrap();r.blob(&mut m).unwrap();r.integer(&mut m).unwrap();}
    r.integer(&mut m).unwrap();r.integer(&mut m).unwrap();let offset=r.at;let mut b=bytes.clone();b[offset]=3;assert!(decode(&b,id,&mut Meter::default()).is_err());
}
#[test]
fn original_byte_equality_survives_forced_bucket_collisions() {
    let env=&bootstrap::shipped(Phase::Runtime).unwrap().environment;
    let c=catalog(Phase::Runtime,env,&mut Meter::default()).unwrap();let c=bounded(&c);
    let id=identity(c.phase,&mut Meter::default()).unwrap();let a=fixture(&c,id);
    let mut forced=Interner::default();let mut m=Meter::default();let mut comparisons=0;
    for (id,key) in a.keys.iter().enumerate() {
        let bytes=copy(key,&mut m).unwrap();assert_eq!(forced.intern_bucket(bytes,0,&mut |n|m.pay(n),&mut comparisons).unwrap(),KeyId(id));
    }
    for (id,key) in a.keys.iter().enumerate().rev() {
        let bytes=copy(key,&mut m).unwrap();assert_eq!(forced.intern_bucket(bytes,0,&mut |n|m.pay(n),&mut comparisons).unwrap(),KeyId(id));
    }
    assert_eq!(forced.keys.len(),a.keys.len());
    println!("field-plan-collision keys={} work={} comparison_subset={} bounded=true",a.keys.len(),m.work,comparisons);
}
#[test]
fn artifact_bound_lookup_rejects_equal_copies_revisions_fields_phases_and_public_restore() {
    let phase=Phase::Runtime;let shipped=bootstrap::shipped(phase).unwrap();
    let c=catalog(phase,&shipped.environment,&mut Meter::default()).unwrap();let c=bounded(&c);
    let id=identity(phase,&mut Meter::default()).unwrap();let a=fixture(&c,id);
    let bytes=encode(&a,&mut Meter::default()).unwrap();
    let decoded=shared(decode(&bytes,id,&mut Meter::default()).unwrap(),&mut Meter::default()).unwrap();
    let proof=correspondence(decoded,&c,&mut Meter::default()).unwrap();
    let installed=bind(&proof,Origin::PrivateShipped(&shipped.environment),&mut Meter::default()).unwrap();
    let n=&c.owners[0];let selector=Selector::Definition;let original=selector.field(n.owner).unwrap();
    let query=|phase,global:&Global,owner:&Arc<DefinitionInfo>,selector,field:&Form| {
        installed.lookup(n.ordinal,phase,global,owner,selector,field,&mut Meter::default()).unwrap().is_some()
    };
    assert!(query(phase,n.global,n.owner,selector,original));
    assert!(query(phase,n.global,n.owner,selector,original),"repeat lookup retains actual borrowed source");
    let cloned=shipped.environment.clone();let same=cloned.shared_definition_info(n.global).unwrap();
    assert!(Arc::ptr_eq(n.owner,same));assert!(query(phase,n.global,same,selector,selector.field(same).unwrap()));
    let namespace=portable::hir::SourceNamespace::capture_namespace(&cloned,phase,"suss.core").unwrap();
    let namespace=namespace.clone();let (_,same)=namespace.declarations.iter().find(|(g,_)|*g==n.global).unwrap();
    assert!(query(phase,n.global,same,selector,selector.field(same).unwrap()));
    let equal_copy=Arc::new((**n.owner).clone());
    assert!(!query(phase,n.global,&equal_copy,selector,selector.field(&equal_copy).unwrap()));
    let equal_form=original.clone();assert!(!query(phase,n.global,n.owner,selector,&equal_form));
    assert!(!query(Phase::Macro,n.global,n.owner,selector,original));
    assert!(!query(phase,n.global,n.owner,Selector::Declaration,original));
    assert!(!query(phase,n.global,n.owner,selector,&n.owner.declaration));
    assert!(installed.lookup(usize::MAX,phase,n.global,n.owner,selector,original,&mut Meter::default()).unwrap().is_none());
    let mut forged=(**n.owner).clone();forged.origin=Some(portable::SourceOrigin::new("forged",None));
    forged.definition_form.span=original.span.clone();let forged=Arc::new(forged);
    assert!(!query(phase,n.global,&forged,selector,selector.field(&forged).unwrap()));
    let mut replaced=cloned.clone();replaced.record_definition(n.global.clone(),(**n.owner).clone());
    let newer=replaced.shared_definition_info(n.global).unwrap();
    assert!(!query(phase,n.global,newer,selector,selector.field(newer).unwrap()));
    let mut env=cloned.clone();env.enter_namespace(phase,"suss.core").unwrap();
    let reanalyzed=portable::prepare_fragment("(def string-hash-cache-count 0)",&env,phase).unwrap();
    let newer=reanalyzed.environment.shared_definition_info(n.global).unwrap();
    assert!(!Arc::ptr_eq(n.owner,newer));assert!(!query(phase,n.global,newer,selector,selector.field(newer).unwrap()));
    // Existing public restore yields independently allocated equal compiler
    // objects, never this private shipped installation provenance.
    let restored=bootstrap::restore(phase,
        include_bytes!("../../../../../runtime/bootstrap/runtime.wasm"),
        include_bytes!("../../../../../runtime/bootstrap/runtime.json")).unwrap();
    let restored_owner=restored.environment.shared_definition_info(n.global).unwrap();
    assert!(!query(phase,n.global,restored_owner,selector,selector.field(restored_owner).unwrap()));
    assert!(matches!(bind(&proof,Origin::PublicRestore(&restored.environment),&mut Meter::default()),Err(Failure::Invalid(_))));
    assert!(matches!(bind(&proof,Origin::PrivateShipped(&restored.environment),&mut Meter::default()),Err(Failure::Invalid(_))));
    // A valid decoded artifact without the offline correspondence token cannot
    // call bind at all. Test-only private tokens are not a public security API.
    let mut wrong=a.clone();wrong.fields.pop();
    assert!(correspondence(Arc::new(wrong),&c,&mut Meter::default()).is_err());
    let mut wrong=a;wrong.fields.push(wrong.fields[0].clone());
    assert!(correspondence(Arc::new(wrong),&c,&mut Meter::default()).is_err());
}
fn fault_cuts<T>(stage:&str, mut run:impl FnMut(&mut Meter)->Result<T>) {
    let mut baseline=Meter::default();let value=run(&mut baseline).unwrap();drop(value);
    let mut accepted=0u64;let mut requested=0u64;
    for irq in [false,true] {
        for cut in 0..baseline.calls {
            let mut m=Meter{fail:Some((cut,irq)),..Meter::default()};
            let err=match run(&mut m) {Err(e)=>e,Ok(_)=>panic!("{stage} cut{cut} published result")};
            assert_eq!(err,if irq{Failure::Irq}else{Failure::StopFuel});
            accepted=accepted.checked_add(m.work).unwrap();requested=requested.checked_add(m.requested).unwrap();
            let mut retry=Meter::default();assert!(run(&mut retry).is_ok());assert_eq!(retry.work,baseline.work);
        }
    }
    println!("field-plan-fault stage={stage} charge_calls={} cuts_per_surrogate={} baseline_work={} failed_attempt_accepted_total={} failed_attempt_requested_total={} scope=native-StopFuel-Irq-surrogates-NOT-Session",baseline.calls,baseline.calls,baseline.work,accepted,requested);
}
#[test]
fn artifact_decode_bind_and_publication_fault_cuts_leave_no_partial_install_and_retry() {
    let env=&bootstrap::shipped(Phase::Runtime).unwrap().environment;
    let c=catalog(Phase::Runtime,env,&mut Meter::default()).unwrap();let c=bounded(&c);
    let id=identity(c.phase,&mut Meter::default()).unwrap();let a=fixture(&c,id);
    let bytes=encode(&a,&mut Meter::default()).unwrap();
    fault_cuts("decode-and-validate",|m|decode(&bytes,id,m));
    let decoded=decode(&bytes,id,&mut Meter::default()).unwrap();
    fault_cuts("shared-storage-publication",|m|shared(decoded.clone(),m));
    let a=shared(decoded,&mut Meter::default()).unwrap();let proof=correspondence(a,&c,&mut Meter::default()).unwrap();
    fault_cuts("private-association-build",|m|bind(&proof,Origin::PrivateShipped(env),m));
    for irq in [false,true] {
        let old=bind(&proof,Origin::PrivateShipped(env),&mut Meter::default()).unwrap();
        let pointer=old.associations.as_ptr();let mut slot=Some(old);
        let new=bind(&proof,Origin::PrivateShipped(env),&mut Meter::default()).unwrap();
        let mut m=Meter{fail:Some((0,irq)),..Meter::default()};
        assert_eq!(publish(&mut slot,new,&mut m).unwrap_err(),if irq{Failure::Irq}else{Failure::StopFuel});
        assert_eq!(slot.as_ref().unwrap().associations.as_ptr(),pointer);
        let n=&c.owners[0];assert!(slot.as_ref().unwrap().lookup(n.ordinal,c.phase,n.global,n.owner,Selector::Definition,&n.owner.definition_form,&mut Meter::default()).unwrap().is_some());
        let new=bind(&proof,Origin::PrivateShipped(env),&mut Meter::default()).unwrap();
        publish(&mut slot,new,&mut Meter::default()).unwrap();
        let old_pointer=slot.as_ref().unwrap().associations.as_ptr();
        let mut m=Meter{fail:Some((0,irq)),..Meter::default()};assert!(retire(&mut slot,&mut m).is_err());
        assert_eq!(slot.as_ref().unwrap().associations.as_ptr(),old_pointer);
        retire(&mut slot,&mut Meter::default()).unwrap();assert!(slot.is_none());
    }
}

#[cfg(test)]
#[path = "field_plan_view.rs"]
mod field_plan_view;
