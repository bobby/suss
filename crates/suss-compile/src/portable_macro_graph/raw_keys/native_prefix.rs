//! Development proof of immutable canonical BYTE keys plus graph-local suffixes.
//! This module is test-only. It neither admits fields nor grants source authority.
use super::*;
use std::sync::Arc;

struct FrozenPrefix {
    interner: Interner,
}
impl FrozenPrefix {
    fn freeze<E>(interner: Interner, charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<Arc<Self>, E> {
        // Transfer existing arrays, allocate the Arc object and its two counters.
        charge((std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>()) as u64 + 1)?;
        Ok(Arc::new(Self { interner }))
    }
    fn find<E>(&self, key: &[u8], hash: u64, charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<Option<KeyId>, E> {
        let Some(head) = self.interner.heads.get(hash, charge)? else { return Ok(None) };
        charge(1)?;
        let mut cursor = Some(head.index());
        charge(7)?; // initialize local diagnostic counters without touching shared state
        let mut stats = EntryStats::default();
        while let Some(index) = cursor {
            charge(2)?;
            let entry = self.interner.keys.get(index, charge, &mut stats)?;
            if compare_bytes(key, &entry.key, charge)? == Ordering::Equal { return Ok(Some(KeyId(index))) }
            cursor = entry.next;
        }
        Ok(None)
    }
}
struct PrefixInterner {
    prefix: Arc<FrozenPrefix>,
    suffix: Interner,
}
impl PrefixInterner {
    fn new<E>(prefix: &Arc<FrozenPrefix>, charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<Self, E> {
        // Empty suffix remains inline. Prepay header construction and Arc retain.
        charge(std::mem::size_of::<Self>() as u64 + 2)?;
        Ok(Self { prefix: prefix.clone(), suffix: Interner::default() })
    }
    fn intern_bucket<E>(&mut self, key: &mut Vec<u8>, hash: u64,
        charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<KeyId, E> {
        charge(1)?;
        if let Some(id) = self.prefix.find(key, hash, charge)? { return Ok(id) }
        // Both counts are actual native allocations, not decoded external IDs.
        // Match the existing canonical entry native-extent overflow policy.
        charge(6)?; // read extents, check addition, and prepay final global-ID addition
        let base = self.prefix.interner.keys.len();
        base.checked_add(self.suffix.keys.len()).expect("Canonical prefix/suffix native extent overflow");
        let id = self.suffix.intern_bucket_scratch(key, hash, charge, &mut 0)?;
        Ok(KeyId(base.checked_add(id.0).expect("Prechecked canonical index")))
    }
    fn plan<E>(&mut self, form: &Form, plan: &mut Plan,
        charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<usize, E> {
        plan_with_intern(form, plan, false, charge, &mut |key, meter| {
            let hash = hash_bytes(key, meter)?;
            self.intern_bucket(key, hash, meter)
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop { Fuel, Irq }
fn read(source: &str) -> Form { suss_reader::forms::read_forms(source).unwrap().remove(0) }
fn rows(plan: &Plan) -> Vec<(KeyId, usize, Vec<usize>)> {
    plan.nodes.iter().map(|n| (n.key, n.metadata_count, n.children.clone())).collect()
}
fn ok(_: u64) -> Result<(), Stop> { Ok(()) }
fn prefix_pair() -> (Arc<FrozenPrefix>, Interner) {
    let mut a = Interner::default(); let mut b = Interner::default();
    for source in ["nil", "[:a :b]", "^{:audit [1 2]} [foo/bar #{:a :b}]", "\"A😀\""] {
        let form = read(source);
        a.plan(&form, &mut Plan::default(), &mut ok).unwrap();
        b.plan(&form, &mut Plan::default(), &mut ok).unwrap();
    }
    (FrozenPrefix::freeze(a, &mut ok).unwrap(), b)
}
#[test]
fn prefix_and_dynamic_suffix_match_original_plan_rows_and_global_ids() {
    let (prefix, mut original) = prefix_pair();
    let count = prefix.interner.keys.len();
    let mut candidate = PrefixInterner::new(&prefix, &mut ok).unwrap();
    for source in ["[:a :b]", "{:new [foo/bar :a 17] :b #{:a :fresh}}", "'^{:audit [1 2]} [foo/bar #{:a :b}]", "\"A😀\"", "[nil false -0.0 :fresh]"] {
        let form = read(source); let mut a = Plan::default(); let mut b = Plan::default();
        assert_eq!(original.plan(&form, &mut a, &mut ok).unwrap(), candidate.plan(&form, &mut b, &mut ok).unwrap());
        assert_eq!(rows(&a), rows(&b), "{source}");
        assert_eq!(original.keys.len(), count + candidate.suffix.keys.len());
    }
    assert_eq!(prefix.interner.keys.len(), count);
    let mut other = PrefixInterner::new(&prefix, &mut ok).unwrap();
    other.plan(&read("[different :suffix]"), &mut Plan::default(), &mut ok).unwrap();
    assert_eq!(prefix.interner.keys.len(), count, "graph-local suffix never mutates prefix");
}
#[test]
fn prefix_and_suffix_forced_collisions_still_require_full_byte_equality() {
    let mut seed = Interner::default();
    for key in [b"alpha".to_vec(), b"alphabet".to_vec()] {
        seed.intern_bucket(key, 7, &mut ok, &mut 0).unwrap();
    }
    let prefix = FrozenPrefix::freeze(seed, &mut ok).unwrap();
    let mut candidate = PrefixInterner::new(&prefix, &mut ok).unwrap();
    for (bytes, id) in [("alpha", 0), ("alphabet", 1), ("alphabeta", 2), ("alpha", 0), ("alphabeta", 2), ("unrelated", 3)] {
        assert_eq!(candidate.intern_bucket(&mut bytes.as_bytes().to_vec(), 7, &mut ok).unwrap(), KeyId(id));
    }
    assert_eq!(candidate.suffix.keys.len(), 2);
}
#[test]
fn prefix_failure_cuts_leave_private_plan_empty_and_original_ids_retryable() {
    let (prefix, mut expected) = prefix_pair(); let form = read("[:a {:new [17 :fresh]} :b]");
    let mut oracle = Plan::default(); expected.plan(&form, &mut oracle, &mut ok).unwrap();
    let mut probe = PrefixInterner::new(&prefix, &mut ok).unwrap(); let mut calls = 0;
    probe.plan(&form, &mut Plan::default(), &mut |_| { calls += 1; Ok::<_, Stop>(()) }).unwrap();
    for error in [Stop::Fuel, Stop::Irq] {
        for cut in 0..calls {
            let mut candidate = PrefixInterner::new(&prefix, &mut ok).unwrap(); let mut plan = Plan::default(); let mut step = 0;
            let err = candidate.plan(&form, &mut plan, &mut |_| {
                let here = step; step += 1; if here == cut { Err(error) } else { Ok(()) }
            }).unwrap_err();
            assert_eq!(err, error); assert!(plan.nodes.is_empty());
            candidate.plan(&form, &mut plan, &mut ok).unwrap(); assert_eq!(rows(&plan), rows(&oracle));
        }
    }
    // These are native callback fault cuts, not a claim of Session epoch testing.
}

// A second development storage model borrows encoded keys instead of copying
// each key into an owned canonical Entry. Plan/field authority is still external.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum BorrowedBuildError<E> { Meter(E), Duplicate, Overflow }
pub(super) struct BorrowedPrefix<'a> {
    keys: Vec<&'a [u8]>,
    next: Vec<Option<HeadIndex>>,
    heads: RawHeads,
}
impl<'a> BorrowedPrefix<'a> {
    pub(super) fn build<E>(keys: impl IntoIterator<Item=&'a [u8]>,
        charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<Arc<Self>, BorrowedBuildError<E>> {
        Self::build_hashed(keys, charge, &mut |key, meter| hash_bytes(key, meter))
    }
    fn build_hashed<E, C: FnMut(u64) -> Result<(), E>>(
        keys: impl IntoIterator<Item=&'a [u8]>, charge: &mut C,
        hash: &mut impl FnMut(&[u8], &mut C) -> Result<u64, E>,
    ) -> Result<Arc<Self>, BorrowedBuildError<E>> {
        use BorrowedBuildError::{Duplicate, Meter, Overflow};
        let mut table = Self { keys: Vec::new(), next: Vec::new(), heads: RawHeads::default() };
        for key in keys {
            charge(3).map_err(Meter)?;
            let hash = hash(key, charge).map_err(Meter)?;
            let head = table.heads.get(hash, charge).map_err(Meter)?;
            let mut cursor = head;
            let mut tail = None;
            while let Some(index) = cursor {
                charge(3).map_err(Meter)?;
                let i = index.index();
                if compare_bytes(key, table.keys[i], charge).map_err(Meter)? == Ordering::Equal { return Err(Duplicate) }
                tail = Some(i); cursor = table.next[i];
            }
            let encoded = table.keys.len().checked_add(1).and_then(std::num::NonZeroUsize::new).ok_or(Overflow)?;
            let index = HeadIndex(encoded);
            reserve(&mut table.keys, 1, charge).map_err(Meter)?;
            reserve(&mut table.next, 1, charge).map_err(Meter)?;
            // Prepay both pushes and the optional tail link before publication.
            charge(5).map_err(Meter)?;
            if head.is_none() { table.heads.get_or_insert(hash, index, charge).map_err(Meter)?; }
            table.keys.push(key); table.next.push(None);
            if let Some(tail) = tail { table.next[tail] = Some(index); }
        }
        charge((std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>()) as u64 + 1).map_err(Meter)?;
        Ok(Arc::new(table))
    }
    fn find<E>(&self, key: &[u8], hash: u64,
        charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<Option<KeyId>, E> {
        let mut cursor = self.heads.get(hash, charge)?;
        while let Some(index) = cursor {
            charge(3)?;
            let i = index.index();
            if compare_bytes(key, self.keys[i], charge)? == Ordering::Equal { return Ok(Some(KeyId(i))) }
            cursor = self.next[i];
        }
        Ok(None)
    }
    pub(super) fn len(&self) -> usize { self.keys.len() }
}
pub(super) struct BorrowedInterner<'a> {
    prefix: Arc<BorrowedPrefix<'a>>,
    suffix: Interner,
}
impl<'a> BorrowedInterner<'a> {
    pub(super) fn new<E>(prefix: &Arc<BorrowedPrefix<'a>>,
        charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<Self, E> {
        charge(std::mem::size_of::<Self>() as u64 + 2)?;
        Ok(Self { prefix: prefix.clone(), suffix: Interner::default() })
    }
    fn intern_bucket<E>(&mut self, key: &mut Vec<u8>, hash: u64,
        charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<KeyId, E> {
        charge(1)?;
        if let Some(id) = self.prefix.find(key, hash, charge)? { return Ok(id) }
        charge(6)?;
        let base = self.prefix.len();
        base.checked_add(self.suffix.keys.len()).expect("Canonical prefix/suffix native extent overflow");
        let id = self.suffix.intern_bucket_scratch(key, hash, charge, &mut 0)?;
        Ok(KeyId(base.checked_add(id.0).expect("Prechecked canonical index")))
    }
    pub(super) fn plan<E>(&mut self, form: &Form, plan: &mut Plan,
        charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<usize, E> {
        plan_with_intern(form, plan, false, charge, &mut |key, meter| {
            let hash = hash_bytes(key, meter)?;
            self.intern_bucket(key, hash, meter)
        })
    }
}
#[test]
fn borrowed_prefix_and_local_suffix_match_original_byte_plans_without_key_copies() {
    let (seed, mut control) = prefix_pair();
    let mut stats = EntryStats::default();
    let keys: Vec<&[u8]> = (0..seed.interner.keys.len()).map(|i|
        seed.interner.keys.get(i, &mut ok, &mut stats).unwrap().key.as_slice()).collect();
    let table = BorrowedPrefix::build(keys.iter().copied(), &mut ok).unwrap();
    for (actual, expected) in table.keys.iter().zip(&keys) { assert!(std::ptr::eq(*actual, *expected), "borrow original bytes"); }
    let mut candidate = BorrowedInterner::new(&table, &mut ok).unwrap();
    for source in ["[:a :b]", "{:fresh [foo/bar :a 17] :b #{:a :new}}", "[nil false -0.0 :new]"] {
        let f=read(source);let mut a=Plan::default();let mut b=Plan::default();
        assert_eq!(control.plan(&f,&mut a,&mut ok).unwrap(),candidate.plan(&f,&mut b,&mut ok).unwrap());
        assert_eq!(rows(&a),rows(&b));
    }
}
#[test]
fn borrowed_prefix_collisions_reject_duplicates_and_preserve_suffix_identity() {
    let values: [&[u8];2] = [b"alpha",b"alphabet"];
    let table=BorrowedPrefix::build_hashed(values,&mut ok,&mut |_,m| {m(1)?;Ok(7)}).unwrap();
    let mut candidate=BorrowedInterner::new(&table,&mut ok).unwrap();
    for (key,id) in [("alpha",0),("alphabet",1),("alphabeta",2),("alpha",0),("alphabeta",2)] {
        assert_eq!(candidate.intern_bucket(&mut key.as_bytes().to_vec(),7,&mut ok).unwrap(),KeyId(id));
    }
    assert!(matches!(BorrowedPrefix::build_hashed([b"alpha".as_slice(),b"alpha".as_slice()],&mut ok,&mut |_,m|{m(1)?;Ok(7)}),Err(BorrowedBuildError::Duplicate)));
}
#[test]
fn borrowed_prefix_build_faults_publish_no_table_and_retries_match() {
    let values: [&[u8];3]=[b"alpha",b"alphabet",b"third"];
    let mut calls=0;
    BorrowedPrefix::build(values,&mut |_|{calls+=1;Ok::<_,Stop>(())}).unwrap();
    for error in [Stop::Fuel,Stop::Irq] {
        for cut in 0..calls {
            let mut at=0;
            let failed=BorrowedPrefix::build(values,&mut |_|{let here=at;at+=1;if here==cut{Err(error)}else{Ok(())}});
            assert!(matches!(failed,Err(BorrowedBuildError::Meter(e)) if e==error));
            let retry=BorrowedPrefix::build(values,&mut ok).unwrap();
            assert_eq!(retry.len(),values.len());
            for (i,key) in values.iter().enumerate() {
                let hash=hash_bytes(key,&mut ok).unwrap();assert_eq!(retry.find(key,hash,&mut ok).unwrap(),Some(KeyId(i)));
            }
        }
    }
}
