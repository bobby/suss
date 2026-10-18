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
