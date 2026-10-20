//! Exact native GC storage observations for future metadata integrity checks.
//!
//! This primitive does not certify macro syntax or authorize metadata reuse.
//! Strict capture follows all fields, including descriptor methods, UID and hash
//! caches. Cut capture stores exact cut identities as opaque leaves. Caller must
//! validate initial factory direct facts before capturing own storage and prove
//! source provenance independently. Neither cuts nor slot overrides certify it.
//! Legitimate memo/protocol changes are reported, not silently accepted. A later
//! policy must authenticate those changes before preserving cross-call identity.
use super::{Session, SessionError, SessionValue};
use crate::portable::metered_map::{Context, TryOrderedMap};
use std::{
    cmp::Ordering,
    collections::hash_map::DefaultHasher,
    hash::Hasher,
    mem::size_of,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering},
    },
};
use wasmtime::{
    AnyRef, AsContextMut, Func, RootScope, Rooted, StoreContextMut, Val,
};

type Result<T> = std::result::Result<T, SessionError>;
mod original_members;
pub(super) use original_members::OriginalMember;

#[derive(Debug)]
enum Atom {
    I32(i32),
    I64(i64),
    F32(u32),
    F64(u64),
    V128(u128),
    I31(u32),
    NullAny,
    NullFunc,
    NullExtern,
    NullExn,
    NullCont,
    Function(Func),
    Node(usize),
    Opaque(SessionValue),
}

#[derive(Debug)]
enum Contents {
    // Each node owns its exact original GC object and therefore its immutable
    // intrinsic type. Mutable descriptors/prototypes remain ordinary atoms.
    Struct(Vec<Atom>),
    Array(Vec<Atom>),
}

#[derive(Debug)]
struct Node {
    value: SessionValue,
    // Same-hash references still require actual GC-reference equality.
    collision: Option<usize>,
    contents: Option<Contents>,
}

/// Owns the actual objects it observed, with normal Session handle accounting.
/// Identity hashes are used only during capture, never retained across GC.
#[derive(Debug)]
pub(crate) struct StorageSnapshot {
    owner: u64,
    root: Atom,
    nodes: Vec<Node>,
}

struct Meter<'a> {
    store: StoreContextMut<'a, ()>,
    interrupt: &'a AtomicBool,
    #[cfg(test)]
    cancel_after: Option<usize>,
    #[cfg(test)]
    force_hash_collision: bool,
}
impl Meter<'_> {
    fn new<'a>(store: StoreContextMut<'a, ()>, interrupt: &'a AtomicBool) -> Meter<'a> {
        Meter {
            store,
            interrupt,
            #[cfg(test)]
            cancel_after: None,
            #[cfg(test)]
            force_hash_collision: false,
        }
    }
    fn work(&mut self, units: u64) -> Result<()> {
        #[cfg(test)]
        if let Some(remaining) = &mut self.cancel_after {
            if *remaining == 0 {
                self.interrupt.store(true, AtomicOrdering::SeqCst);
                self.cancel_after = None;
            } else {
                *remaining -= 1;
            }
        }
        Session::consume_host_work(self.store.as_context_mut(), units, self.interrupt)
            .map_err(Into::into)
    }
}
impl Context<u64> for Meter<'_> {
    type Error = SessionError;
    fn charge(&mut self, units: u64) -> Result<()> {
        self.work(units)
    }
    fn compare(&mut self, left: &u64, right: &u64) -> Result<Ordering> {
        self.work(1)?;
        Ok(left.cmp(right))
    }
}

fn invalid(message: impl Into<String>) -> SessionError {
    SessionError::Host(wasmtime::Error::msg(message.into()))
}
fn allocation<T>(count: usize, meter: &mut Meter<'_>) -> Result<()> {
    let bytes = count
        .checked_mul(size_of::<T>())
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| invalid("Storage snapshot allocation size overflow"))?;
    meter.work(bytes)
}
fn room<T>(values: &mut Vec<T>, meter: &mut Meter<'_>) -> Result<()> {
    meter.work(1)?;
    if values.len() == values.capacity() {
        let capacity = values
            .capacity()
            .checked_mul(2)
            .ok_or_else(|| invalid("Storage snapshot capacity overflow"))?
            .max(4);
        allocation::<T>(capacity, meter)?;
        allocation::<T>(values.len(), meter)?;
        values
            .try_reserve_exact(capacity - values.len())
            .map_err(|error| invalid(format!("Storage snapshot allocation failed: {error}")))?;
    }
    Ok(())
}

struct Cut { value: SessionValue, collision: Option<usize> }
// Search strategy only: all larger valid cut lists keep the current index.
const BORROWED_CUT_LIMIT: usize = 32;
struct Capture<'cuts> {
    owner: u64,
    handles: Arc<AtomicUsize>,
    nodes: Vec<Node>,
    index: TryOrderedMap<u64, usize>,
    cuts: Vec<Cut>,
    cut_index: TryOrderedMap<u64, usize>,
    borrowed_cuts: Option<&'cuts [&'cuts SessionValue]>,
}
impl<'cuts> Capture<'cuts> {
    fn hash(reference: &Rooted<AnyRef>, meter: &mut Meter<'_>) -> Result<u64> {
        meter.work(1)?;
        let mut hash = DefaultHasher::new();
        reference.ref_hash(&meter.store, &mut hash)?;
        let hash = hash.finish();
        #[cfg(test)]
        let hash = if meter.force_hash_collision { 0 } else { hash };
        Ok(hash)
    }
    fn prepare_cuts(&mut self, cuts: &'cuts [&'cuts SessionValue], root: &Val, meter: &mut Meter<'_>) -> Result<()> {
        meter.work(4)?; // empty-state checks, supplied length and strategy dispatch
        debug_assert!(self.borrowed_cuts.is_none() && self.cuts.is_empty());
        if cuts.len() <= BORROWED_CUT_LIMIT {
            // Owners were checked before RootScope. Validate every supplied
            // entry, including duplicates/immediates/unused cuts, before atoms.
            for cut in cuts {
                meter.work(4)?; // iteration, root access and exact reference check
                let reference = cut.value.to_rooted(&mut meter.store);
                if let Some(root) = root.anyref().flatten() {
                    if Rooted::<AnyRef>::ref_eq(&meter.store, root, &reference)? {
                        return Err(invalid("Storage snapshot root cannot be cut"));
                    }
                }
            }
            meter.work(1)?; // private slice publication
            self.borrowed_cuts = Some(cuts);
            return Ok(());
        }
        self.add_cuts(cuts, root, meter)
    }

    fn add_cuts(&mut self, cuts: &[&SessionValue], root: &Val, meter: &mut Meter<'_>) -> Result<()> {
        // Session checked owners before one RootScope. No guest call/allocation
        // or GC while either ephemeral identity index lives.
        for cut in cuts {
            meter.work(2)?;
            let reference = cut.value.to_rooted(&mut meter.store);
            if let Some(root) = root.anyref().flatten() {
                if Rooted::<AnyRef>::ref_eq(&meter.store, root, &reference)? {
                    return Err(invalid("Storage snapshot root cannot be cut"));
                }
            }
            // Immediate scalars are immutable leaves, never opaque boxes.
            if reference.as_i31(&meter.store)?.is_some() { continue }
            let hash = Self::hash(&reference, meter)?;
            let head = self.cut_index.get(&hash, meter)?.copied();
            let mut cursor = head;
            let mut duplicate = false;
            while let Some(id) = cursor {
                meter.work(1)?;
                let previous = self.cuts[id].value.value.to_rooted(&mut meter.store);
                if Rooted::<AnyRef>::ref_eq(&meter.store, &reference, &previous)? {
                    duplicate = true; break;
                }
                cursor = self.cuts[id].collision;
            }
            if duplicate { continue }
            room(&mut self.cuts, meter)?;
            meter.work(1)?;
            let value = reference.to_owned_rooted(&mut meter.store)?;
            self.handles.fetch_add(1, AtomicOrdering::Relaxed);
            let id = self.cuts.len();
            self.cuts.push(Cut { value: SessionValue { identity: self.owner,
                value, handles: self.handles.clone() }, collision: head });
            self.cut_index.insert(hash, id, meter)?;
        }
        Ok(())
    }

    fn atom(&mut self, value: Val, meter: &mut Meter<'_>) -> Result<Atom> {
        meter.work(1)?;
        Ok(match value {
            Val::I32(x) => Atom::I32(x),
            Val::I64(x) => Atom::I64(x),
            Val::F32(x) => Atom::F32(x),
            Val::F64(x) => Atom::F64(x),
            Val::V128(x) => Atom::V128(x.into()),
            Val::AnyRef(None) => Atom::NullAny,
            Val::FuncRef(None) => Atom::NullFunc,
            Val::ExternRef(None) => Atom::NullExtern,
            Val::ExnRef(None) => Atom::NullExn,
            Val::ContRef(None) => Atom::NullCont,
            Val::FuncRef(Some(f)) => Atom::Function(f),
            Val::AnyRef(Some(reference)) => {
                if let Some(value) = reference.as_i31(&meter.store)? {
                    return Ok(Atom::I31(value.get_u32()));
                }
                // No guest execution or guest heap allocation occurs in this
                // traversal. The ephemeral index is discarded before returning;
                // later checks use rooted ref_eq, not pre-GC reference hashes.
                meter.work(2)?; // optional borrowed mode and entry into cut search
                if let Some(cuts) = self.borrowed_cuts {
                    for cut in cuts {
                        meter.work(4)?; // iterator/load/root access/exact comparison
                        let expected = cut.value.to_rooted(&mut meter.store);
                        if Rooted::<AnyRef>::ref_eq(&meter.store, &reference, &expected)? {
                            meter.work(4)?; // owned reference, counter and handle retention
                            // Keep only reached opaque occurrences. Never clone
                            // the caller's SessionValue/accounting counter.
                            let value = cut.value.clone();
                            self.handles.fetch_add(1, AtomicOrdering::Relaxed);
                            return Ok(Atom::Opaque(SessionValue {
                                identity: self.owner,
                                value,
                                handles: self.handles.clone(),
                            }));
                        }
                    }
                }
                // An opaque borrowed hit needs no hash/index work. Ordinary
                // nodes and the unchanged large-cut path still pay all of it.
                let hash = Self::hash(&reference, meter)?;
                if !self.cuts.is_empty() {
                    let mut cursor = self.cut_index.get(&hash, meter)?.copied();
                    while let Some(id) = cursor {
                        meter.work(1)?;
                        let expected = self.cuts[id].value.value.to_rooted(&mut meter.store);
                        if Rooted::<AnyRef>::ref_eq(&meter.store, &reference, &expected)? {
                            // Atom buffer bytes were already charged; the
                            // clone performs an owned-root reference increment.
                            meter.work(1)?;
                            return Ok(Atom::Opaque(self.cuts[id].value.clone()));
                        }
                        cursor = self.cuts[id].collision;
                    }
                }
                let head = self.index.get(&hash, meter)?.copied();
                let mut cursor = head;
                while let Some(id) = cursor {
                    meter.work(1)?;
                    let previous = self.nodes[id].value.value.to_rooted(&mut meter.store);
                    if Rooted::<AnyRef>::ref_eq(&meter.store, &reference, &previous)? {
                        return Ok(Atom::Node(id));
                    }
                    cursor = self.nodes[id].collision;
                }
                if reference.as_struct(&meter.store)?.is_none()
                    && reference.as_array(&meter.store)?.is_none()
                {
                    return Err(invalid("Unsupported heap reference in storage snapshot"));
                }
                room(&mut self.nodes, meter)?;
                meter.work(1)?;
                let value = reference.to_owned_rooted(&mut meter.store)?;
                self.handles.fetch_add(1, AtomicOrdering::Relaxed);
                let id = self.nodes.len();
                self.nodes.push(Node {
                    value: SessionValue {
                        identity: self.owner,
                        value,
                        handles: self.handles.clone(),
                    },
                    collision: head,
                    contents: None,
                });
                // Capture is private until complete. Any error drops all its
                // owned roots; no partial snapshot is returned or published.
                self.index.insert(hash, id, meter)?;
                Atom::Node(id)
            }
            Val::ExternRef(Some(_)) | Val::ExnRef(Some(_)) | Val::ContRef(Some(_)) => {
                return Err(invalid(
                    "Opaque host/exception/continuation reference in storage snapshot",
                ));
            }
        })
    }

    fn finish(mut self, root: Val, meter: &mut Meter<'_>) -> Result<StorageSnapshot> {
        let root = self.atom(root, meter)?;
        let mut id = 0;
        while id < self.nodes.len() {
            meter.work(1)?;
            let reference = self.nodes[id].value.value.to_rooted(&mut meter.store);
            let contents = if let Some(structure) = reference.as_struct(&meter.store)? {
                let ty = structure.ty(&meter.store)?;
                let count = ty.fields().len();
                allocation::<Atom>(count, meter)?;
                let mut values = Vec::new();
                values.try_reserve_exact(count).map_err(|error| {
                    invalid(format!("Storage snapshot allocation failed: {error}"))
                })?;
                for field in 0..count {
                    meter.work(1)?;
                    let value = structure.field(&mut meter.store, field)?;
                    values.push(self.atom(value, meter)?);
                }
                Contents::Struct(values)
            } else if let Some(array) = reference.as_array(&meter.store)? {
                let _ty = array.ty(&meter.store)?;
                let count = array.len(&meter.store)?;
                allocation::<Atom>(count as usize, meter)?;
                let mut values = Vec::new();
                values.try_reserve_exact(count as usize).map_err(|error| {
                    invalid(format!("Storage snapshot allocation failed: {error}"))
                })?;
                for index in 0..count {
                    meter.work(1)?;
                    let value = array.get(&mut meter.store, index)?;
                    values.push(self.atom(value, meter)?);
                }
                Contents::Array(values)
            } else {
                return Err(invalid("Unsupported storage snapshot node"));
            };
            self.nodes[id].contents = Some(contents);
            id += 1;
        }
        Ok(StorageSnapshot {
            owner: self.owner,
            root,
            nodes: self.nodes,
        })
    }
}

impl StorageSnapshot {
    pub(super) fn matches_in_context(
        &self,
        store: StoreContextMut<'_, ()>,
        root: Val,
        owner: u64,
        interrupt: &AtomicBool,
    ) -> Result<bool> {
        if self.owner != owner { return Err(SessionError::ForeignValue); }
        self.matches(root, &mut Meter::new(store, interrupt))
    }

    /// Only strict array atom mismatches invoke `allow`, after exact root,
    /// array identity, type and length checks. Struct fields are never exempt.
    /// Caller must authenticate the precise array/index/current value and meter
    /// its own hook work; numeric shape is not authority. Hook must be read-only
    /// and must not run guest callbacks or mutate already inspected storage.
    pub(super) fn matches_in_context_with_slots(
        &self, store: StoreContextMut<'_, ()>, root: Val, owner: u64,
        interrupt: &AtomicBool,
        mut allow: impl FnMut(StoreContextMut<'_, ()>, Val, u32, &Val) -> Result<bool>,
    ) -> Result<bool> {
        if self.owner != owner { return Err(SessionError::ForeignValue) }
        self.matches_with_slots(root, &mut Meter::new(store, interrupt), &mut allow)
    }
    /// Initial membership only. Opaque cut roots count, descendants do not.
    /// Current storage/source authorization still requires independent checks.
    pub(super) fn contains_reference_in_context(
        &self, store: StoreContextMut<'_, ()>, value: Val, owner: u64,
        interrupt: &AtomicBool,
    ) -> Result<bool> {
        if self.owner != owner { return Err(SessionError::ForeignValue) }
        let mut meter = Meter::new(store, interrupt);
        if self.atom_matches(&self.root, value.clone(), &mut meter)? { return Ok(true) }
        for (id, node) in self.nodes.iter().enumerate() {
            meter.work(1)?;
            if self.atom_matches(&Atom::Node(id), value.clone(), &mut meter)? { return Ok(true) }
            let atoms = match node.contents.as_ref().expect("complete snapshot") {
                Contents::Array(atoms) | Contents::Struct(atoms) => atoms,
            };
            for atom in atoms {
                meter.work(1)?;
                if matches!(atom, Atom::I31(_) | Atom::Opaque(_)) &&
                    self.atom_matches(atom, value.clone(), &mut meter)? { return Ok(true) }
            }
        }
        Ok(false)
    }

    fn atom_matches(&self, expected: &Atom, actual: Val, meter: &mut Meter<'_>) -> Result<bool> {
        meter.work(1)?;
        Ok(match (expected, actual) {
            (Atom::I32(a), Val::I32(b)) => *a == b,
            (Atom::I64(a), Val::I64(b)) => *a == b,
            (Atom::F32(a), Val::F32(b)) => *a == b,
            (Atom::F64(a), Val::F64(b)) => *a == b,
            (Atom::V128(a), Val::V128(b)) => *a == u128::from(b),
            (Atom::NullAny, Val::AnyRef(None))
            | (Atom::NullFunc, Val::FuncRef(None))
            | (Atom::NullExtern, Val::ExternRef(None))
            | (Atom::NullExn, Val::ExnRef(None))
            | (Atom::NullCont, Val::ContRef(None)) => true,
            (Atom::I31(a), Val::AnyRef(Some(b))) => {
                b.as_i31(&meter.store)?.is_some_and(|b| b.get_u32() == *a)
            }
            (Atom::Function(a), Val::FuncRef(Some(b))) => {
                a.to_raw(&mut meter.store) == b.to_raw(&mut meter.store)
            }
            (Atom::Opaque(value), Val::AnyRef(Some(actual))) => {
                let expected = value.value.to_rooted(&mut meter.store);
                Rooted::<AnyRef>::ref_eq(&meter.store, &expected, &actual)?
            }
            (Atom::Node(id), Val::AnyRef(Some(actual))) => {
                let expected = self.nodes[*id].value.value.to_rooted(&mut meter.store);
                Rooted::<AnyRef>::ref_eq(&meter.store, &expected, &actual)?
            }
            _ => false,
        })
    }

    fn matches(&self, root: Val, meter: &mut Meter<'_>) -> Result<bool> {
        self.matches_with_slots(root, meter, &mut |_, _, _, _| Ok(false))
    }
    fn matches_with_slots(
        &self, root: Val, meter: &mut Meter<'_>,
        allow: &mut impl FnMut(StoreContextMut<'_, ()>, Val, u32, &Val) -> Result<bool>,
    ) -> Result<bool> {
        if !self.atom_matches(&self.root, root, meter)? {
            return Ok(false);
        }
        for node in &self.nodes {
            meter.work(1)?;
            let reference = node.value.value.to_rooted(&mut meter.store);
            match node
                .contents
                .as_ref()
                .expect("only complete snapshots escape capture")
            {
                Contents::Struct(values) => {
                    let Some(structure) = reference.as_struct(&meter.store)? else {
                        return Ok(false);
                    };
                    // Exact incoming reference identity fixes this owned
                    // object's intrinsic type. Keep fallible kind/type/count
                    // checks and every mutable field observation.
                    meter.work(3)?;
                    if structure.ty(&meter.store)?.fields().len() != values.len() {
                        return Ok(false);
                    }
                    for (field, expected) in values.iter().enumerate() {
                        meter.work(1)?;
                        let actual = structure.field(&mut meter.store, field)?;
                        if !self.atom_matches(expected, actual, meter)? {
                            return Ok(false);
                        }
                    }
                }
                Contents::Array(values) => {
                    let Some(array) = reference.as_array(&meter.store)? else {
                        return Ok(false);
                    };
                    let _ty = array.ty(&meter.store)?;
                    if array.len(&meter.store)? as usize != values.len() {
                        return Ok(false);
                    }
                    for (index, expected) in values.iter().enumerate() {
                        meter.work(1)?;
                        let actual = array.get(&mut meter.store, index as u32)?;
                        if !self.atom_matches(expected, actual.clone(), meter)? {
                            meter.work(1)?;
                            if !allow(meter.store.as_context_mut(), Val::AnyRef(Some(reference)), index as u32, &actual)? {
                                return Ok(false);
                            }
                        }
                    }
                }
            }
        }
        Ok(true)
    }
}

impl Session {
    /// Membership in the initial observation only; current storage must still
    /// pass storage_matches before this can contribute to any integrity audit.
    pub(super) fn storage_contains_reference(
        &mut self,
        value: &SessionValue,
        snapshot: &StorageSnapshot,
    ) -> Result<bool> {
        self.check(value)?;
        if snapshot.owner != self.identity { return Err(SessionError::ForeignValue); }
        let _request = self.operation_budget_scope()?;
        let interrupt = self.host_work_interrupt();
        let mut scope = RootScope::new(&mut self.store);
        let value = Val::AnyRef(Some(value.value.to_rooted(&mut scope)));
        snapshot.contains_reference_in_context(scope.as_context_mut(), value, self.identity, &interrupt)
    }

    pub(crate) fn capture_storage(&mut self, value: &SessionValue) -> Result<StorageSnapshot> {
        self.capture_storage_accounted(value, self.handles.clone())
    }

    pub(super) fn capture_storage_accounted(
        &mut self,
        value: &SessionValue,
        handles: Arc<AtomicUsize>,
    ) -> Result<StorageSnapshot> {
        self.capture_storage_cut_accounted(value, &[], handles)
    }

    /// Exact own-storage observation with opaque boundaries, not certification.
    /// Validate initial factory direct facts before capture; independently prove
    /// source provenance. Root cannot be cut. Containing slots remain strict.
    pub(crate) fn capture_storage_cut(
        &mut self, value: &SessionValue, cuts: &[&SessionValue],
    ) -> Result<StorageSnapshot> {
        self.capture_storage_cut_accounted(value, cuts, self.handles.clone())
    }
    pub(super) fn capture_storage_cut_accounted(
        &mut self, value: &SessionValue, cuts: &[&SessionValue], handles: Arc<AtomicUsize>,
    ) -> Result<StorageSnapshot> {
        self.check(value)?;
        let _request = self.operation_budget_scope()?;
        for cut in cuts { self.charge_host_work(1)?; self.check(cut)?; }
        let interrupt = self.host_work_interrupt();
        let mut scope = RootScope::new(&mut self.store);
        let root = Val::AnyRef(Some(value.value.to_rooted(&mut scope)));
        let mut meter = Meter::new(scope.as_context_mut(), &interrupt);
        // Prepay the added slice metadata and initialization/retirement controls.
        meter.work(size_of::<Option<&[&SessionValue]>>() as u64 + 8)?;
        let mut capture = Capture { owner: self.identity, handles,
            nodes: Vec::new(), index: TryOrderedMap::default(),
            cuts: Vec::new(), cut_index: TryOrderedMap::default(), borrowed_cuts: None };
        capture.prepare_cuts(cuts, &root, &mut meter)?;
        capture.finish(root, &mut meter)
    }

    pub(crate) fn storage_matches(
        &mut self,
        value: &SessionValue,
        snapshot: &StorageSnapshot,
    ) -> Result<bool> {
        self.check(value)?;
        if snapshot.owner != self.identity {
            return Err(SessionError::ForeignValue);
        }
        let _request = self.operation_budget_scope()?;
        let interrupt = self.host_work_interrupt();
        let mut scope = RootScope::new(&mut self.store);
        let root = Val::AnyRef(Some(value.value.to_rooted(&mut scope)));
        snapshot.matches(root, &mut Meter::new(scope.as_context_mut(), &interrupt))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::hir::Literal;
    use wasmtime::{ArrayType, StructType, FieldType, Mutability, StorageType, StructRef, StructRefPre, ValType};

    fn raw_array(session: &mut Session, values: &[&SessionValue]) -> SessionValue {
        session.budget().unwrap();
        let mut scope = RootScope::new(&mut session.store);
        let mut output = [Val::null_any_ref()];
        session
            .runtime
            .get_func(&mut scope, "args-new")
            .unwrap()
            .call(&mut scope, &[Val::I32(values.len() as i32)], &mut output)
            .unwrap();
        let array = output[0]
            .unwrap_anyref()
            .unwrap()
            .as_array(&scope)
            .unwrap()
            .unwrap();
        for (index, value) in values.iter().enumerate() {
            let value = Val::AnyRef(Some(value.value.to_rooted(&mut scope)));
            array.set(&mut scope, index as u32, value).unwrap();
        }
        super::super::own(
            &mut scope,
            output[0].clone(),
            session.identity,
            &session.handles,
        )
        .unwrap()
    }
    fn runtime_value(session: &mut Session, name: &str, values: &[&SessionValue]) -> SessionValue {
        session.budget().unwrap();
        let mut scope = RootScope::new(&mut session.store);
        let values = values
            .iter()
            .map(|v| Val::AnyRef(Some(v.value.to_rooted(&mut scope))))
            .collect::<Vec<_>>();
        let function = session.runtime.get_func(&mut scope, name).unwrap();
        super::super::call(
            &mut scope,
            session.runtime,
            function,
            &values,
            session.identity,
            &session.handles,
        )
        .unwrap()
    }
    fn set_array(session: &mut Session, array: &SessionValue, index: u32, value: &SessionValue) {
        let mut scope = RootScope::new(&mut session.store);
        let target = array
            .value
            .to_rooted(&mut scope)
            .as_array(&scope)
            .unwrap()
            .unwrap();
        let value = Val::AnyRef(Some(value.value.to_rooted(&mut scope)));
        target.set(&mut scope, index, value).unwrap();
    }
    fn structure(session: &mut Session, ty: ValType, value: Val) -> SessionValue {
        let ty = StructType::new(
            &session.engine,
            [FieldType::new(Mutability::Var, StorageType::ValType(ty))],
        )
        .unwrap();
        let allocator = StructRefPre::new(&mut session.store, ty);
        let mut scope = RootScope::new(&mut session.store);
        let value = StructRef::new(&mut scope, &allocator, &[value])
            .unwrap()
            .to_anyref();
        super::super::own(
            &mut scope,
            Val::AnyRef(Some(value)),
            session.identity,
            &session.handles,
        )
        .unwrap()
    }
    fn set_field(session: &mut Session, value: &SessionValue, field: Val) {
        let mut scope = RootScope::new(&mut session.store);
        value
            .value
            .to_rooted(&mut scope)
            .as_struct(&scope)
            .unwrap()
            .unwrap()
            .set_field(&mut scope, 0, field)
            .unwrap();
    }
    fn exhausted(error: SessionError, trap: wasmtime::Trap) {
        assert!(
            matches!(error, SessionError::Trap(ref error)
            if error.downcast_ref::<wasmtime::Trap>() == Some(&trap)),
            "{error:?}"
        );
    }

    #[test]
    fn native_storage_snapshot_hash_collisions_preserve_distinct_nodes() {
        let mut session = Session::new().unwrap();
        let number = session.data_scalar(&Literal::Number(17.0)).unwrap();
        let left = raw_array(&mut session, &[&number]);
        let right = raw_array(&mut session, &[&number]);
        let root = raw_array(&mut session, &[&left, &left, &right]);
        let handles = session.stats().external_value_handles;
        let snapshot = {
            let _request = session.operation_budget_scope().unwrap();
            let interrupt = session.host_work_interrupt();
            let mut scope = RootScope::new(&mut session.store);
            let value = Val::AnyRef(Some(root.value.to_rooted(&mut scope)));
            let mut meter = Meter::new(scope.as_context_mut(), &interrupt);
            meter.force_hash_collision = true;
            Capture {
                owner: session.identity,
                handles: session.handles.clone(),
                nodes: Vec::new(),
                index: TryOrderedMap::default(),
            cuts: Vec::new(),
            cut_index: TryOrderedMap::default(), borrowed_cuts: None,
            }
            .finish(value, &mut meter)
            .unwrap()
        };
        assert_eq!(snapshot.nodes.len(), 4, "root, two arrays and boxed number");
        session.collect().unwrap();
        assert!(session.storage_matches(&root, &snapshot).unwrap());
        set_array(&mut session, &root, 1, &right);
        assert!(!session.storage_matches(&root, &snapshot).unwrap());
        set_array(&mut session, &root, 1, &left);
        set_array(&mut session, &root, 2, &left);
        assert!(!session.storage_matches(&root, &snapshot).unwrap());
        set_array(&mut session, &root, 2, &right);
        assert!(session.storage_matches(&root, &snapshot).unwrap());
        drop(snapshot);
        assert_eq!(session.stats().external_value_handles, handles);
    }

    #[test]
    fn native_storage_snapshot_preserves_aliases_cycles_and_gc_identity() {
        let mut session = Session::new().unwrap();
        let number = session.data_scalar(&Literal::Number(17.0)).unwrap();
        let left = raw_array(&mut session, &[&number]);
        let right = raw_array(&mut session, &[&number]);
        let root = raw_array(&mut session, &[&left, &left, &right]);
        let handles = session.stats().external_value_handles;
        let snapshot = session.capture_storage(&root).unwrap();
        assert!(session.stats().external_value_handles > handles);
        session.collect().unwrap();
        assert!(session.storage_matches(&root, &snapshot).unwrap());
        set_array(&mut session, &root, 1, &right);
        assert!(
            !session.storage_matches(&root, &snapshot).unwrap(),
            "equal contents cannot hide alias splitting"
        );
        set_array(&mut session, &root, 1, &left);
        set_array(&mut session, &root, 2, &left);
        assert!(
            !session.storage_matches(&root, &snapshot).unwrap(),
            "equal contents cannot hide alias merging"
        );
        set_array(&mut session, &root, 2, &right);
        assert!(session.storage_matches(&root, &snapshot).unwrap());
        drop(snapshot);
        assert_eq!(session.stats().external_value_handles, handles);
        // Physical cycles are finite graphs here, not certificates that cyclic
        // values are valid macro syntax. Semantic decoders must still reject them.
        set_array(&mut session, &left, 0, &root);
        set_array(&mut session, &right, 0, &right);
        let cyclic = session.capture_storage(&root).unwrap();
        session.collect().unwrap();
        assert!(session.storage_matches(&root, &cyclic).unwrap());
        set_array(&mut session, &right, 0, &number);
        assert!(!session.storage_matches(&root, &cyclic).unwrap());
    }

    #[test]
    fn native_storage_snapshot_checks_actual_owned_source_array_backing() {
        let mut session = Session::new().unwrap();
        let original = session.data_scalar(&Literal::Number(17.0)).unwrap();
        let replacement = session.data_scalar(&Literal::Number(99.0)).unwrap();
        let root = session.data_array(&[&original]).unwrap();
        let snapshot = session.capture_storage(&root).unwrap();
        session.collect().unwrap();
        assert!(session.storage_matches(&root, &snapshot).unwrap());
        let index = session.data_scalar(&Literal::Number(0.0)).unwrap();
        session.budget().unwrap();
        let mut scope = RootScope::new(&mut session.store);
        let args =
            [&root, &index, &replacement].map(|v| Val::AnyRef(Some(v.value.to_rooted(&mut scope))));
        let function = session
            .runtime
            .get_func(&mut scope, "source-array-set")
            .unwrap();
        let value = super::super::call(
            &mut scope,
            session.runtime,
            function,
            &args,
            session.identity,
            &session.handles,
        )
        .unwrap();
        drop(value);
        drop(scope);
        assert!(!session.storage_matches(&root, &snapshot).unwrap());
        // Checking does not repair the previous input or publish a new seal.
        assert!(!session.storage_matches(&root, &snapshot).unwrap());
    }

    #[test]
    fn native_storage_snapshot_preserves_float_bits_and_all_scalar_variants() {
        let mut session = Session::new().unwrap();
        let cases = [
            (ValType::F64, Val::F64(0), Val::F64((-0.0f64).to_bits())),
            (
                ValType::F64,
                Val::F64(0x7ff8_0000_0000_0001),
                Val::F64(0x7ff8_0000_0000_0002),
            ),
            (ValType::F32, Val::F32(0), Val::F32((-0.0f32).to_bits())),
            (ValType::F32, Val::F32(0x7fc0_0001), Val::F32(0x7fc0_0002)),
            (ValType::I32, Val::I32(i32::MIN), Val::I32(i32::MAX)),
            (ValType::I64, Val::I64(i64::MIN), Val::I64(i64::MAX)),
            (
                ValType::V128,
                Val::V128(1u128.into()),
                Val::V128((1u128 << 127).into()),
            ),
        ];
        for (ty, before, after) in cases {
            let root = structure(&mut session, ty, before.clone());
            let snapshot = session.capture_storage(&root).unwrap();
            assert!(session.storage_matches(&root, &snapshot).unwrap());
            set_field(&mut session, &root, after);
            assert!(!session.storage_matches(&root, &snapshot).unwrap());
            set_field(&mut session, &root, before);
            assert!(session.storage_matches(&root, &snapshot).unwrap());
        }
        let nil = session.data_scalar(&Literal::Nil).unwrap();
        let root = raw_array(&mut session, &[&nil]);
        let snapshot = session.capture_storage(&root).unwrap();
        let mut scope = RootScope::new(&mut session.store);
        root.value
            .to_rooted(&mut scope)
            .as_array(&scope)
            .unwrap()
            .unwrap()
            .set(&mut scope, 0, Val::null_any_ref())
            .unwrap();
        drop(scope);
        assert!(
            !session.storage_matches(&root, &snapshot).unwrap(),
            "i31 nil differs from null"
        );
        let text = session
            .data_scalar(&Literal::String(vec![0xd800, 0, 0xdfff]))
            .unwrap();
        let snapshot = session.capture_storage(&text).unwrap();
        let mut scope = RootScope::new(&mut session.store);
        text.value
            .to_rooted(&mut scope)
            .as_array(&scope)
            .unwrap()
            .unwrap()
            .set(&mut scope, 2, Val::I32(0xd800))
            .unwrap();
        drop(scope);
        assert!(!session.storage_matches(&text, &snapshot).unwrap());
    }

    #[test]
    fn native_storage_snapshot_compares_functions_not_only_their_signatures() {
        let mut session = Session::new().unwrap();
        let first = session.runtime.get_func(&mut session.store, "nil").unwrap();
        let other = session
            .runtime
            .get_func(&mut session.store, "true")
            .unwrap();
        let root = structure(&mut session, ValType::FUNCREF, Val::FuncRef(Some(first)));
        let snapshot = session.capture_storage(&root).unwrap();
        let same = session.runtime.get_func(&mut session.store, "nil").unwrap();
        set_field(&mut session, &root, Val::FuncRef(Some(same)));
        session.collect().unwrap();
        assert!(session.storage_matches(&root, &snapshot).unwrap());
        set_field(&mut session, &root, Val::FuncRef(Some(other)));
        assert!(!session.storage_matches(&root, &snapshot).unwrap());
    }

    #[test]
    fn native_storage_snapshot_reports_uid_warming_without_repairing_it() {
        let mut session = Session::new().unwrap();
        let nil = session.data_scalar(&Literal::Nil).unwrap();
        let root = session.data_array(&[&nil]).unwrap();
        let cold = session.capture_storage(&root).unwrap();
        let uid = runtime_value(&mut session, "identity-uid", &[&root]);
        assert!(!session.storage_matches(&root, &cold).unwrap());
        let warm = session.capture_storage(&root).unwrap();
        session.collect().unwrap();
        assert!(session.storage_matches(&root, &warm).unwrap());
        let again = runtime_value(&mut session, "identity-uid", &[&root]);
        let a = uid.value.to_rooted(&mut session.store);
        let b = again.value.to_rooted(&mut session.store);
        assert!(
            Rooted::<AnyRef>::ref_eq(&session.store, &a, &b).unwrap(),
            "checking must not clear the existing UID memo"
        );
        let forged = session.data_scalar(&Literal::Number(999.0)).unwrap();
        let mut scope = RootScope::new(&mut session.store);
        let value = Val::AnyRef(Some(forged.value.to_rooted(&mut scope)));
        root.value
            .to_rooted(&mut scope)
            .as_struct(&scope)
            .unwrap()
            .unwrap()
            .set_field(&mut scope, 3, value)
            .unwrap();
        drop(scope);
        assert!(
            !session.storage_matches(&root, &warm).unwrap(),
            "a valid-looking numeric UID is still a storage change"
        );
    }

    #[test]
    fn native_storage_snapshot_covers_schema_and_reports_live_protocol_changes() {
        let mut session = Session::new().unwrap();
        let name = session
            .data_scalar(&Literal::String(vec![u16::from(b'x')]))
            .unwrap();
        let fields = raw_array(&mut session, &[&name]);
        let descriptor = runtime_value(&mut session, "descriptor-new", &[&fields]);
        let nil = session.data_scalar(&Literal::Nil).unwrap();
        let data = raw_array(&mut session, &[&nil]);
        let object = runtime_value(&mut session, "object-new", &[&descriptor, &data]);
        let before = session.capture_storage(&object).unwrap();
        let empty = raw_array(&mut session, &[]);
        let key = runtime_value(&mut session, "descriptor-new", &[&empty]);
        session.budget().unwrap();
        let mut scope = RootScope::new(&mut session.store);
        let args = [&descriptor, &key].map(|v| Val::AnyRef(Some(v.value.to_rooted(&mut scope))));
        let mut marker = [Val::AnyRef(None)];
        session
            .runtime
            .get_func(&mut scope, "protocol-marker-set")
            .unwrap()
            .call(&mut scope, &args, &mut marker)
            .unwrap();
        drop(scope);
        assert!(
            !session.storage_matches(&object, &before).unwrap(),
            "the exact primitive reports legitimate method-table changes; reuse policy must keep dispatch live"
        );
        let mut scope = RootScope::new(&mut session.store);
        let args = [&key, &object].map(|v| Val::AnyRef(Some(v.value.to_rooted(&mut scope))));
        let mut result = [Val::I32(0)];
        session
            .runtime
            .get_func(&mut scope, "protocol-marker-satisfies")
            .unwrap()
            .call(&mut scope, &args, &mut result)
            .unwrap();
        assert_eq!(
            result[0].unwrap_i32(),
            1,
            "checking must not restore the old method table"
        );
        drop(scope);
        let changed_methods = session.capture_storage(&object).unwrap();
        let mut scope = RootScope::new(&mut session.store);
        name.value
            .to_rooted(&mut scope)
            .as_array(&scope)
            .unwrap()
            .unwrap()
            .set(&mut scope, 0, Val::I32(i32::from(b'y')))
            .unwrap();
        drop(scope);
        assert!(
            !session.storage_matches(&object, &changed_methods).unwrap(),
            "descriptor identity alone must not exempt its reachable field schema"
        );
    }

    #[test]
    fn native_storage_snapshot_rejects_opaque_payload_without_leaking_roots() {
        let mut session = Session::new().unwrap();
        let ty = StructType::new(
            &session.engine,
            [FieldType::new(
                Mutability::Var,
                StorageType::ValType(ValType::EXTERNREF),
            )],
        )
        .unwrap();
        let allocator = StructRefPre::new(&mut session.store, ty);
        let mut scope = RootScope::new(&mut session.store);
        let external = wasmtime::ExternRef::new(&mut scope, 17u32).unwrap();
        let object = StructRef::new(&mut scope, &allocator, &[Val::ExternRef(Some(external))])
            .unwrap()
            .to_anyref();
        let root = super::super::own(
            &mut scope,
            Val::AnyRef(Some(object)),
            session.identity,
            &session.handles,
        )
        .unwrap();
        drop(scope);
        let handles = session.stats().external_value_handles;
        let error = session.capture_storage(&root).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Opaque host/exception/continuation reference"),
            "{error:?}"
        );
        assert_eq!(session.stats().external_value_handles, handles);
    }

    #[test]
    fn native_storage_snapshot_follows_deep_graphs_iteratively() {
        let mut session = Session::new().unwrap();
        let mut root = session.data_scalar(&Literal::Nil).unwrap();
        for _ in 0..512 {
            root = raw_array(&mut session, &[&root]);
        }
        let snapshot = session.capture_storage(&root).unwrap();
        assert_eq!(snapshot.nodes.len(), 512);
        session.collect().unwrap();
        assert!(session.storage_matches(&root, &snapshot).unwrap());
    }

    #[test]
    fn native_storage_snapshot_uses_fuel_for_capture_and_check_then_recovers() {
        let mut session = Session::new().unwrap();
        let nil = session.data_scalar(&Literal::Nil).unwrap();
        let root = raw_array(&mut session, &vec![&nil; 4096]);
        let snapshot = session.capture_storage(&root).unwrap();
        let handles = session.stats().external_value_handles;
        session.set_operation_fuel(5000);
        exhausted(
            session.capture_storage(&root).unwrap_err(),
            wasmtime::Trap::OutOfFuel,
        );
        assert_eq!(session.operation_fuel_remaining().unwrap(), 0);
        assert_eq!(
            session.stats().external_value_handles,
            handles,
            "failed capture cannot leak owned roots"
        );
        exhausted(
            session.storage_matches(&root, &snapshot).unwrap_err(),
            wasmtime::Trap::OutOfFuel,
        );
        assert_eq!(session.operation_fuel_remaining().unwrap(), 0);
        // No allowance increase between the failed operation and recovery.
        let small = session.capture_storage(&nil).unwrap();
        assert!(session.storage_matches(&nil, &small).unwrap());
        assert_eq!(session.options().fuel_per_operation, 5000);
    }

    #[test]
    fn native_storage_snapshot_mid_capture_and_check_interrupts_are_atomic() {
        let mut session = Session::new().unwrap();
        let nil = session.data_scalar(&Literal::Nil).unwrap();
        let root = raw_array(&mut session, &vec![&nil; 128]);
        let snapshot = session.capture_storage(&root).unwrap();
        let handles = session.stats().external_value_handles;
        for checking in [false, true] {
            let _request = session.operation_budget_scope().unwrap();
            let interrupt = session.host_work_interrupt();
            let mut scope = RootScope::new(&mut session.store);
            let root_value = Val::AnyRef(Some(root.value.to_rooted(&mut scope)));
            let mut meter = Meter::new(scope.as_context_mut(), &interrupt);
            meter.cancel_after = Some(12);
            let error = if checking {
                snapshot.matches(root_value, &mut meter).unwrap_err()
            } else {
                Capture {
                    owner: session.identity,
                    handles: session.handles.clone(),
                    nodes: Vec::new(),
                    index: TryOrderedMap::default(),
            cuts: Vec::new(),
            cut_index: TryOrderedMap::default(), borrowed_cuts: None,
                }
                .finish(root_value, &mut meter)
                .unwrap_err()
            };
            exhausted(error, wasmtime::Trap::Interrupt);
            assert_eq!(meter.store.get_fuel().unwrap(), 0);
            drop(meter);
            drop(scope);
            assert_eq!(session.stats().external_value_handles, handles);
        }
        assert!(session.storage_matches(&root, &snapshot).unwrap());
    }

    #[test]
    fn native_storage_snapshot_owner_check_precedes_any_foreign_root_access() {
        let mut session = Session::new().unwrap();
        let root = session.data_scalar(&Literal::Number(17.0)).unwrap();
        let snapshot = session.capture_storage(&root).unwrap();
        let mut other = Session::new().unwrap();
        let local = other.data_scalar(&Literal::Number(17.0)).unwrap();
        assert!(matches!(
            other.capture_storage(&root),
            Err(SessionError::ForeignValue)
        ));
        assert!(matches!(
            other.storage_matches(&root, &snapshot),
            Err(SessionError::ForeignValue)
        ));
        assert!(matches!(
            other.storage_matches(&local, &snapshot),
            Err(SessionError::ForeignValue)
        ));
        drop(session);
        assert!(matches!(
            other.storage_matches(&local, &snapshot),
            Err(SessionError::ForeignValue)
        ));
    }

    #[test]
    fn native_cut_storage_opaque_identity_mutability_membership_and_own_spine() {
        let mut session = Session::new().unwrap();
        let number = structure(&mut session, ValType::F64, Val::F64(17.0f64.to_bits()));
        let nil = session.data_scalar(&Literal::Nil).unwrap();
        let child = raw_array(&mut session, &[&number]);
        let other = raw_array(&mut session, &[&number]);
        let root = raw_array(&mut session, &[&child, &nil]);
        let strict = session.capture_storage(&root).unwrap();
        let handles = session.stats().external_value_handles;
        let cut = session.capture_storage_cut(&root, &[&child, &child, &nil]).unwrap();
        assert_eq!(cut.nodes.len(), 1, "cut contents and immutable nil are not nodes");
        session.collect().unwrap();
        set_field(&mut session, &number, Val::F64(99.0f64.to_bits()));
        assert!(!session.storage_matches(&root, &strict).unwrap());
        assert!(session.storage_matches(&root, &cut).unwrap());
        assert!(session.storage_contains_reference(&child, &cut).unwrap());
        assert!(session.storage_contains_reference(&nil, &cut).unwrap());
        assert!(!session.storage_contains_reference(&number, &cut).unwrap(), "opaque descendants are not members");
        set_array(&mut session, &root, 0, &other);
        assert!(!session.storage_matches(&root, &cut).unwrap(), "equal-content cut replacement is still a changed slot");
        set_array(&mut session, &root, 0, &child);
        set_array(&mut session, &root, 1, &number);
        assert!(!session.storage_matches(&root, &cut).unwrap(), "own slots remain strict");
        drop(cut);
        assert_eq!(session.stats().external_value_handles, handles);
        assert!(session.capture_storage_cut(&root, &[&root]).is_err());
        assert!(session.capture_storage_cut(&nil, &[&nil]).is_err(), "root is never cut, even an immediate");
    }
    #[test]
    fn native_cut_storage_slot_hook_is_exact_and_never_exempts_root_or_struct() {
        let mut session = Session::new().unwrap();
        let nil = session.data_scalar(&Literal::Nil).unwrap();
        let memo = session.data_scalar(&Literal::Number(719.0)).unwrap();
        let root = raw_array(&mut session, &[&nil, &nil]);
        let snapshot = session.capture_storage(&root).unwrap();
        set_array(&mut session, &root, 1, &memo);
        assert!(!session.storage_matches(&root, &snapshot).unwrap());
        let check = |session: &mut Session, candidate: &SessionValue| {
            let _request = session.operation_budget_scope().unwrap();
            let interrupt = session.host_work_interrupt();
            let mut scope = RootScope::new(&mut session.store);
            let actual_root = Val::AnyRef(Some(candidate.value.to_rooted(&mut scope)));
            let expected_array = root.value.to_rooted(&mut scope);
            let expected_value = memo.value.to_rooted(&mut scope);
            let mut calls = 0;
            let result = snapshot.matches_in_context_with_slots(scope.as_context_mut(), actual_root,
                session.identity, &interrupt, |mut store, array, index, actual| {
                    Session::consume_host_work(store.as_context_mut(), 3, &interrupt)?;
                    calls += 1;
                    Ok(index == 1 && Rooted::<AnyRef>::ref_eq(&store,
                        array.unwrap_anyref().unwrap(), &expected_array)? &&
                        matches!(actual, Val::AnyRef(Some(value)) if Rooted::<AnyRef>::ref_eq(&store, value, &expected_value)?))
                }).unwrap();
            (result, calls)
        };
        assert_eq!(check(&mut session, &root), (true, 1));
        set_array(&mut session, &root, 0, &memo);
        assert_eq!(check(&mut session, &root), (false, 1));
        let wrong_length = raw_array(&mut session, &[&nil]);
        assert_eq!(check(&mut session, &wrong_length), (false, 0));
        let wrong_type = structure(&mut session, ValType::I32, Val::I32(5));
        assert_eq!(check(&mut session, &wrong_type), (false, 0));
        let own = session.capture_storage(&wrong_type).unwrap();
        set_field(&mut session, &wrong_type, Val::I32(6));
        let _request = session.operation_budget_scope().unwrap();
        let irq = session.host_work_interrupt();
        let mut scope = RootScope::new(&mut session.store);
        let value = Val::AnyRef(Some(wrong_type.value.to_rooted(&mut scope)));
        let mut called = false;
        assert!(!own.matches_in_context_with_slots(scope.as_context_mut(), value,
            session.identity, &irq, |_, _, _, _| { called = true; Ok(true) }).unwrap());
        assert!(!called, "struct mismatch cannot invoke array override");
    }
    #[test]
    fn native_cut_storage_forced_collision_cuts_do_not_hide_other_nodes_or_aliases() {
        let mut session = Session::new().unwrap();
        let nil = session.data_scalar(&Literal::Nil).unwrap();
        let number = session.data_scalar(&Literal::Number(99.0)).unwrap();
        let a = raw_array(&mut session, &[&nil]);
        let b = raw_array(&mut session, &[&nil]);
        let c = raw_array(&mut session, &[&nil]);
        let root = raw_array(&mut session, &[&a, &b, &c]);
        let snapshot = {
            let _request = session.operation_budget_scope().unwrap();
            let irq = session.host_work_interrupt();
            let mut scope = RootScope::new(&mut session.store);
            let value = Val::AnyRef(Some(root.value.to_rooted(&mut scope)));
            let mut meter = Meter::new(scope.as_context_mut(), &irq);
            meter.force_hash_collision = true;
            let mut capture = Capture { owner: session.identity, handles: session.handles.clone(),
                nodes: Vec::new(), index: TryOrderedMap::default(), cuts: Vec::new(),
                cut_index: TryOrderedMap::default(), borrowed_cuts: None };
            capture.add_cuts(&[&a, &b, &a, &nil], &value, &mut meter).unwrap();
            capture.finish(value, &mut meter).unwrap()
        };
        assert_eq!(snapshot.nodes.len(), 2, "root and uncut c");
        session.collect().unwrap();
        set_array(&mut session, &a, 0, &number);
        set_array(&mut session, &b, 0, &number);
        assert!(session.storage_matches(&root, &snapshot).unwrap());
        set_array(&mut session, &c, 0, &number);
        assert!(!session.storage_matches(&root, &snapshot).unwrap());
        set_array(&mut session, &c, 0, &nil);
        set_array(&mut session, &root, 0, &b);
        assert!(!session.storage_matches(&root, &snapshot).unwrap());
    }
    #[test]
    fn native_cut_storage_fuel_irq_owner_and_failed_capture_cleanup() {
        let mut session = Session::new().unwrap();
        let nil = session.data_scalar(&Literal::Nil).unwrap();
        let child = raw_array(&mut session, &[&nil]);
        let root = raw_array(&mut session, &vec![&nil; 4096]);
        let snapshot = session.capture_storage_cut(&root, &[&child]).unwrap();
        let handles = session.stats().external_value_handles;
        session.set_operation_fuel(5000);
        exhausted(session.capture_storage_cut(&root, &[&child]).unwrap_err(), wasmtime::Trap::OutOfFuel);
        assert_eq!(session.stats().external_value_handles, handles);
        {
            let _request = session.operation_budget_scope().unwrap();
            let irq = session.host_work_interrupt();
            let mut scope = RootScope::new(&mut session.store);
            let value = Val::AnyRef(Some(root.value.to_rooted(&mut scope)));
            exhausted(snapshot.matches_in_context_with_slots(scope.as_context_mut(), value,
                session.identity, &irq, |_, _, _, _| Ok(false)).unwrap_err(), wasmtime::Trap::OutOfFuel);
        }
        for checking in [false, true] {
            let _request = session.operation_budget_scope().unwrap();
            let irq = session.host_work_interrupt();
            let mut scope = RootScope::new(&mut session.store);
            let value = Val::AnyRef(Some(root.value.to_rooted(&mut scope)));
            let mut meter = Meter::new(scope.as_context_mut(), &irq);
            meter.cancel_after = Some(12);
            let error = if checking {
                snapshot.matches_with_slots(value, &mut meter, &mut |_, _, _, _| Ok(false)).unwrap_err()
            } else {
                let mut capture = Capture { owner: session.identity, handles: session.handles.clone(),
                    nodes: Vec::new(), index: TryOrderedMap::default(), cuts: Vec::new(),
                    cut_index: TryOrderedMap::default(), borrowed_cuts: None };
                capture.add_cuts(&[&child], &value, &mut meter)
                    .and_then(|()| capture.finish(value, &mut meter)).unwrap_err()
            };
            exhausted(error, wasmtime::Trap::Interrupt);
            drop(meter); drop(scope);
            assert_eq!(session.stats().external_value_handles, handles);
        }
        let small = session.capture_storage_cut(&nil, &[&child]).unwrap();
        assert!(session.storage_matches(&nil, &small).unwrap());
        assert_eq!(session.options().fuel_per_operation, 5000);
        let mut foreign = Session::new().unwrap();
        let other = foreign.data_scalar(&Literal::Nil).unwrap();
        assert!(matches!(session.capture_storage_cut(&nil, &[&other]), Err(SessionError::ForeignValue)));
        let _request = foreign.operation_budget_scope().unwrap();
        let irq = foreign.host_work_interrupt();
        let mut scope = RootScope::new(&mut foreign.store);
        let value = Val::AnyRef(Some(other.value.to_rooted(&mut scope)));
        assert!(matches!(small.contains_reference_in_context(scope.as_context_mut(), value,
            foreign.identity, &irq), Err(SessionError::ForeignValue)));
    }
    #[test]
    fn native_storage_snapshot_owned_nodes_survive_external_type_and_module_drop() {
        eprintln!("snapshot-owned-layout Node={} Contents={} Snapshot={} Atom={}",
            size_of::<Node>(), size_of::<Contents>(), size_of::<StorageSnapshot>(), size_of::<Atom>());
        let mut session = Session::new().unwrap();
        // The host allocator's temporary StructType is dropped by structure().
        let host = structure(&mut session, ValType::I32, Val::I32(17));
        let values = {
            let _request = session.operation_budget_scope().unwrap();
            let module = wasmtime::Module::new(&session.engine, r#"(module
                (type $s (struct (field (mut i32))))
                (type $a (array (mut i32)))
                (type $immutable (array i32))
                (func (export "object") (result (ref $s)) i32.const 17 struct.new $s)
                (func (export "array") (result (ref $a)) i32.const 17 i32.const 2 array.new $a)
                (func (export "immutable") (result (ref $immutable))
                    i32.const 17 i32.const 2 array.new $immutable))"#).unwrap();
            let instance = wasmtime::Instance::new(&mut session.store, &module, &[]).unwrap();
            let mut values = Vec::new();
            for name in ["object", "object", "array", "immutable"] {
                let mut scope = RootScope::new(&mut session.store);
                let function = instance.get_func(&mut scope, name).unwrap();
                let mut result = [Val::null_any_ref()];
                function.call(&mut scope, &[], &mut result).unwrap();
                values.push(super::super::own(&mut scope, result[0].clone(),
                    session.identity, &session.handles).unwrap());
            }
            values
        }; // Caller Module/Instance/function handles are gone; Store owns registrations.
        let host_snapshot = session.capture_storage(&host).unwrap();
        let guest_snapshot = session.capture_storage(&values[0]).unwrap();
        let array_snapshot = session.capture_storage(&values[2]).unwrap();
        session.collect().unwrap();
        assert!(session.storage_matches(&host, &host_snapshot).unwrap());
        assert!(session.storage_matches(&values[0], &guest_snapshot).unwrap());
        assert!(session.storage_matches(&values[2], &array_snapshot).unwrap());
        // Same intrinsic type/different object, and same array contents with a
        // different mutability/type, never replace the captured references.
        assert!(!session.storage_matches(&values[1], &guest_snapshot).unwrap());
        assert!(!session.storage_matches(&values[3], &array_snapshot).unwrap());
        for (root, snapshot) in [(&host, &host_snapshot), (&values[0], &guest_snapshot)] {
            set_field(&mut session, root, Val::I32(99));
            assert!(!session.storage_matches(root, snapshot).unwrap());
            set_field(&mut session, root, Val::I32(17));
            assert!(session.storage_matches(root, snapshot).unwrap());
        }
        for value in [99, 17] {
            let mut scope = RootScope::new(&mut session.store);
            let array = values[2].value.to_rooted(&mut scope).as_array(&scope).unwrap().unwrap();
            array.set(&mut scope, 1, Val::I32(value)).unwrap();
            drop(scope);
            assert_eq!(session.storage_matches(&values[2], &array_snapshot).unwrap(), value == 17);
        }
        session.collect().unwrap();
        assert!(session.storage_matches(&host, &host_snapshot).unwrap());
        assert!(session.storage_matches(&values[0], &guest_snapshot).unwrap());
        assert!(session.storage_matches(&values[2], &array_snapshot).unwrap());
    }

    #[test]
    fn borrowed_cuts_keep_snapshot_handle_accounting_and_owned_leaves_after_inputs_drop() {
        eprintln!("borrowed-cut-layout slice={} node={} cut={} snapshot={}",
            size_of::<Option<&[&SessionValue]>>(), size_of::<Node>(), size_of::<Cut>(), size_of::<StorageSnapshot>());
        for phase in [crate::portable::resolve::Phase::Runtime, crate::portable::resolve::Phase::Macro] {
            let mut session = match phase {
                crate::portable::resolve::Phase::Runtime => Session::new().unwrap(),
                _ => Session::new_macro().unwrap(),
            };
            let nil = session.data_scalar(&Literal::Nil).unwrap();
            let child = raw_array(&mut session, &[&nil]);
            let root = raw_array(&mut session, &[&child, &child, &nil]);
            let external = session.stats().external_value_handles;
            let handles = Arc::new(AtomicUsize::new(0));
            let snapshot = session.capture_storage_cut_accounted(
                &root, &[&child, &nil, &child], handles.clone(),
            ).unwrap();
            // One array root and two opaque child occurrences belong to the
            // proof counter. The caller's root counter must not increase.
            assert_eq!(snapshot.nodes.len(), 1);
            assert_eq!(handles.load(AtomicOrdering::Relaxed), 3);
            assert_eq!(session.stats().external_value_handles, external);
            drop(child);
            session.collect().unwrap();
            assert!(session.storage_matches(&root, &snapshot).unwrap());
            set_array(&mut session, &root, 1, &nil);
            assert!(!session.storage_matches(&root, &snapshot).unwrap());
            drop(root);
            session.collect().unwrap();
            assert_eq!(handles.load(AtomicOrdering::Relaxed), 3);
            drop(snapshot);
            assert_eq!(handles.load(AtomicOrdering::Relaxed), 0);
            assert_eq!(session.stats().external_value_handles, external - 2);
        }
    }

    #[test]
    fn borrowed_cut_boundaries_fuel_irq_unused_foreign_and_root_rejection_recover() {
        let mut session = Session::new().unwrap();
        let nil = session.data_scalar(&Literal::Nil).unwrap();
        let child = raw_array(&mut session, &[&nil]);
        let root = raw_array(&mut session, &[&child]);
        let mut foreign = Session::new().unwrap();
        let unused_foreign = raw_array(&mut foreign, &[]);
        let handles = Arc::new(AtomicUsize::new(0));
        for count in [BORROWED_CUT_LIMIT, BORROWED_CUT_LIMIT + 1] {
            let cuts = vec![&child; count];
            let cost = {
                let request = session.operation_budget_scope().unwrap();
                let before = session.store.get_fuel().unwrap();
                drop(session.capture_storage_cut_accounted(&root, &cuts, handles.clone()).unwrap());
                let cost = before - session.store.get_fuel().unwrap();
                drop(request);
                cost
            };
            let external = session.stats().external_value_handles;
            for allowance in 0..cost {
                let request = session.operation_budget_scope().unwrap();
                session.store.set_fuel(allowance).unwrap();
                exhausted(session.capture_storage_cut_accounted(&root, &cuts, handles.clone()).unwrap_err(), wasmtime::Trap::OutOfFuel);
                assert_eq!(handles.load(AtomicOrdering::Relaxed), 0, "count={count} allowance={allowance}");
                assert_eq!(session.stats().external_value_handles, external);
                drop(request);
            }
            let request = session.operation_budget_scope().unwrap();
            session.interrupt_handle().interrupt();
            assert!(session.capture_storage_cut_accounted(&root, &cuts, handles.clone()).unwrap_err().is_interrupt());
            drop(request);
            assert_eq!(handles.load(AtomicOrdering::Relaxed), 0);
            let snapshot = session.capture_storage_cut_accounted(&root, &cuts, handles.clone()).unwrap();
            assert!(session.storage_matches(&root, &snapshot).unwrap());
            drop(snapshot);
            let mut malformed = cuts.clone();
            malformed[count - 1] = &unused_foreign;
            assert!(matches!(session.capture_storage_cut_accounted(&root, &malformed, handles.clone()), Err(SessionError::ForeignValue)));
            malformed[count - 1] = &root;
            assert!(matches!(session.capture_storage_cut_accounted(&root, &malformed, handles.clone()), Err(SessionError::Host(_))));
            let immediate_root = vec![&nil; count];
            assert!(matches!(session.capture_storage_cut_accounted(&nil, &immediate_root, handles.clone()), Err(SessionError::Host(_))));
            assert_eq!(handles.load(AtomicOrdering::Relaxed), 0);
            assert_eq!(session.stats().external_value_handles, external);
        }
    }


    #[test]
    fn borrowed_cut_strategies_match_current_index_across_supplied_boundaries_and_collisions() {
        fn capture_mode(
            session: &mut Session, root: &SessionValue, cuts: &[&SessionValue],
            indexed: bool, collision: bool,
        ) -> (StorageSnapshot, u64) {
            session.check(root).unwrap();
            let _request = session.operation_budget_scope().unwrap();
            let before = session.store.get_fuel().unwrap();
            for cut in cuts { session.charge_host_work(1).unwrap(); session.check(cut).unwrap(); }
            let interrupt = session.host_work_interrupt();
            let mut scope = RootScope::new(&mut session.store);
            let value = Val::AnyRef(Some(root.value.to_rooted(&mut scope)));
            let mut meter = Meter::new(scope.as_context_mut(), &interrupt);
            meter.force_hash_collision = collision;
            meter.work(size_of::<Option<&[&SessionValue]>>() as u64 + 8).unwrap();
            let mut capture = Capture {
                owner: session.identity, handles: session.handles.clone(),
                nodes: Vec::new(), index: TryOrderedMap::default(),
                cuts: Vec::new(), cut_index: TryOrderedMap::default(), borrowed_cuts: None,
            };
            if indexed {
                capture.add_cuts(cuts, &value, &mut meter).unwrap();
            } else {
                capture.prepare_cuts(cuts, &value, &mut meter).unwrap();
                assert_eq!(capture.borrowed_cuts.is_some(), cuts.len() <= BORROWED_CUT_LIMIT);
            }
            let snapshot = capture.finish(value, &mut meter).unwrap();
            let cost = before - meter.store.get_fuel().unwrap();
            (snapshot, cost)
        }
        let mut session = Session::new().unwrap();
        let nil = session.data_scalar(&Literal::Nil).unwrap();
        let number = session.data_scalar(&Literal::Number(17.0)).unwrap();
        for count in [0, 1, 31, 32, 33] {
            for collision in [false, true] {
                let child = raw_array(&mut session, &[&nil]);
                let strict_child = raw_array(&mut session, &[&nil]);
                let unused = raw_array(&mut session, &[]);
                let root = raw_array(&mut session, &[&child, &child, &strict_child, &nil]);
                let mut cuts = vec![&unused; count];
                if count > 0 { cuts[0] = &child; }
                if count > 2 { cuts[1] = &child; cuts[2] = &nil; }
                let external = session.stats().external_value_handles;
                let (borrowed, borrowed_cost) = capture_mode(&mut session, &root, &cuts, false, collision);
                let (indexed, indexed_cost) = capture_mode(&mut session, &root, &cuts, true, collision);
                eprintln!("snapshot-cut-strategies supplied={count} collision={collision} selected={borrowed_cost} forced-indexed={indexed_cost}");
                assert_eq!(borrowed.nodes.len(), indexed.nodes.len());
                session.collect().unwrap();
                for snapshot in [&borrowed, &indexed] {
                    assert!(session.storage_matches(&root, snapshot).unwrap());
                    assert!(!session.storage_contains_reference(&unused, snapshot).unwrap());
                }
                set_array(&mut session, &child, 0, &number);
                for snapshot in [&borrowed, &indexed] {
                    assert_eq!(session.storage_matches(&root, snapshot).unwrap(), count != 0);
                }
                set_array(&mut session, &child, 0, &nil);
                set_array(&mut session, &strict_child, 0, &number);
                for snapshot in [&borrowed, &indexed] {
                    assert!(!session.storage_matches(&root, snapshot).unwrap());
                }
                set_array(&mut session, &strict_child, 0, &nil);
                session.collect().unwrap();
                for snapshot in [&borrowed, &indexed] {
                    assert!(session.storage_matches(&root, snapshot).unwrap());
                }
                drop(borrowed); drop(indexed);
                assert_eq!(session.stats().external_value_handles, external);
            }
        }
    }

}
