//! Canonical BYTE-key storage shared by compiler-generated native tables.
//! This grants no source/guest authority. Source-field admission is separate.
use super::compiled_field_codegen::NativePhase;
use super::{
    HeadIndex, Interner, KeyId, Plan, RawHeads, compare_bytes, hash_bytes, plan_with_intern,
    reserve,
};
use std::{cmp::Ordering, sync::Arc};
use suss_reader::forms::Form;

#[derive(Debug)]
pub(in crate::portable_macro_graph) enum BuildError<E> {
    Meter(E),
    Invalid(&'static str),
}
pub(in crate::portable_macro_graph) struct Prefix {
    data: &'static NativePhase,
    heads: RawHeads,
    next: Vec<Option<HeadIndex>>,
}
impl Prefix {
    fn bytes<E>(
        data: &'static NativePhase,
        index: usize,
        charge: &mut impl FnMut(u64) -> Result<(), E>,
    ) -> Result<&'static [u8], BuildError<E>> {
        charge(6).map_err(BuildError::Meter)?;
        let key = data
            .keys
            .get(index)
            .ok_or(BuildError::Invalid("compiled key index"))?;
        let end = key
            .start
            .checked_add(key.len)
            .ok_or(BuildError::Invalid("compiled key extent"))?;
        data.key_bytes
            .get(key.start..end)
            .ok_or(BuildError::Invalid("compiled key bytes"))
    }
    pub(in crate::portable_macro_graph) fn build<E>(
        data: &'static NativePhase,
        charge: &mut impl FnMut(u64) -> Result<(), E>,
    ) -> Result<Arc<Self>, BuildError<E>> {
        use BuildError::{Invalid, Meter};
        charge(2).map_err(Meter)?;
        if data.keys.is_empty() {
            return Err(Invalid("compiled native keys are empty"));
        }
        let mut value = Self {
            data,
            heads: RawHeads::default(),
            next: Vec::new(),
        };
        reserve(&mut value.next, data.keys.len(), charge).map_err(Meter)?;
        let mut end = 0;
        for index in 0..data.keys.len() {
            charge(3).map_err(Meter)?;
            if data.keys[index].start != end {
                return Err(Invalid("compiled key coverage"));
            }
            let bytes = Self::bytes(data, index, charge)?;
            end = end
                .checked_add(bytes.len())
                .ok_or(Invalid("compiled key extent"))?;
            let hash = hash_bytes(bytes, charge).map_err(Meter)?;
            let head = value.heads.get(hash, charge).map_err(Meter)?;
            let mut cursor = head;
            let mut tail = None;
            while let Some(encoded) = cursor {
                charge(3).map_err(Meter)?;
                let other = encoded.index();
                if compare_bytes(bytes, Self::bytes(data, other, charge)?, charge).map_err(Meter)?
                    == Ordering::Equal
                {
                    return Err(Invalid("duplicate compiled native key"));
                }
                tail = Some(other);
                cursor = value.next[other];
            }
            let number = index
                .checked_add(1)
                .and_then(std::num::NonZeroUsize::new)
                .ok_or(Invalid("compiled native key count"))?;
            let encoded = HeadIndex(number);
            // Reserve and prepay before publishing a head, element or link.
            charge(4).map_err(Meter)?;
            if head.is_none() {
                value
                    .heads
                    .get_or_insert(hash, encoded, charge)
                    .map_err(Meter)?;
            }
            value.next.push(None);
            if let Some(tail) = tail {
                value.next[tail] = Some(encoded);
            }
        }
        if end != data.key_bytes.len() {
            return Err(Invalid("compiled key trailing bytes"));
        }
        charge((std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>()) as u64 + 1)
            .map_err(Meter)?;
        Ok(Arc::new(value))
    }
    fn find<E>(
        &self,
        key: &[u8],
        hash: u64,
        charge: &mut impl FnMut(u64) -> Result<(), E>,
    ) -> Result<Option<KeyId>, E> {
        let mut cursor = self.heads.get(hash, charge)?;
        while let Some(encoded) = cursor {
            charge(9)?;
            let index = encoded.index();
            // build() checked this immutable table's complete byte ranges.
            let range = &self.data.keys[index];
            let bytes = &self.data.key_bytes[range.start..range.start + range.len];
            if compare_bytes(key, bytes, charge)? == Ordering::Equal {
                return Ok(Some(KeyId(index)));
            }
            cursor = self.next[index];
        }
        Ok(None)
    }
    fn len(&self) -> usize {
        self.data.keys.len()
    }
}

pub(in crate::portable_macro_graph) enum GraphInterner {
    Legacy(Interner),
    Prefixed {
        prefix: Arc<Prefix>,
        suffix: Interner,
    },
}
impl Default for GraphInterner {
    fn default() -> Self {
        Self::Legacy(Interner::default())
    }
}
impl GraphInterner {
    pub(in crate::portable_macro_graph) fn has_prefix(&self, expected: &Arc<Prefix>) -> bool {
        matches!(self,Self::Prefixed{prefix,..} if Arc::ptr_eq(prefix,expected))
    }
    pub(in crate::portable_macro_graph) fn with_prefix<E>(
        prefix: &Arc<Prefix>,
        charge: &mut impl FnMut(u64) -> Result<(), E>,
    ) -> Result<Self, E> {
        charge(std::mem::size_of::<Self>() as u64 + 2)?;
        Ok(Self::Prefixed {
            prefix: prefix.clone(),
            suffix: Interner::default(),
        })
    }
    pub(in crate::portable_macro_graph) fn plan<E>(
        &mut self,
        form: &Form,
        plan: &mut Plan,
        charge: &mut impl FnMut(u64) -> Result<(), E>,
    ) -> Result<usize, E> {
        charge(1)?;
        match self {
            Self::Legacy(interner) => interner.plan(form, plan, charge),
            Self::Prefixed { prefix, suffix } => {
                plan_with_intern(form, plan, false, charge, &mut |key, meter| {
                    let hash = hash_bytes(key, meter)?;
                    meter(1)?;
                    if let Some(value) = prefix.find(key, hash, meter)? {
                        return Ok(value);
                    }
                    meter(6)?;
                    let base = prefix.len();
                    base.checked_add(suffix.keys.len())
                        .expect("Canonical prefix/suffix native extent overflow");
                    let id = suffix.intern_bucket_scratch(key, hash, meter, &mut 0)?;
                    Ok(KeyId(
                        base.checked_add(id.0).expect("Prechecked canonical index"),
                    ))
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable_macro_graph::{generated_native_fields, raw_keys::EntryStats};
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Stop {
        Fuel,
        Irq,
    }
    fn ok(_: u64) -> Result<(), Stop> {
        Ok(())
    }
    fn seed(data: &NativePhase) -> Interner {
        let mut control = Interner::default();
        for (i, key) in data.keys.iter().enumerate() {
            let mut bytes = data.key_bytes[key.start..key.start + key.len].to_vec();
            assert_eq!(
                control.intern_scratch(&mut bytes, &mut ok).unwrap(),
                KeyId(i)
            );
        }
        control
    }
    #[test]
    fn compiled_prefix_dynamic_suffix_preserves_original_global_ids_and_rows() {
        for data in [
            &generated_native_fields::RUNTIME,
            &generated_native_fields::MACRO,
        ] {
            let mut work = 0;
            let prefix = Prefix::build(data, &mut |u| {
                work += u;
                Ok::<_, Stop>(())
            })
            .unwrap();
            let mut candidate = GraphInterner::with_prefix(&prefix, &mut ok).unwrap();
            let mut control = seed(data);
            for source in [
                "[nil false -0.0 :new]",
                "^{:audit [1 2]} {:left [17 :right] :right #{:left :new}}",
                "[foo/bar :new \"A😀\"]",
            ] {
                let form = suss_reader::forms::read_forms(source).unwrap().remove(0);
                let mut a = Plan::default();
                let mut b = Plan::default();
                assert_eq!(
                    candidate.plan(&form, &mut a, &mut ok).unwrap(),
                    control.plan(&form, &mut b, &mut ok).unwrap()
                );
                assert_eq!(a.nodes.len(), b.nodes.len());
                for (a, b) in a.nodes.iter().zip(&b.nodes) {
                    assert_eq!(
                        (a.key, a.metadata_count, &a.children),
                        (b.key, b.metadata_count, &b.children)
                    );
                }
            }
            let mut stats = EntryStats::default();
            let sample = control.keys.get(0, &mut ok, &mut stats).unwrap();
            assert!(!sample.key.is_empty());
            eprintln!(
                "compiled-prefix phase={:?} keys={} startup_work={work} runtime_admission=UNIMPLEMENTED",
                data.phase,
                data.keys.len()
            );
        }
    }
    #[test]
    fn compiled_prefix_suffix_fault_cuts_preserve_prefix_and_retry_ids() {
        let data = &generated_native_fields::RUNTIME;
        let prefix = Prefix::build(data, &mut ok).unwrap();
        let form = suss_reader::forms::read_forms("[nil {:source-control [17 :fresh]} :fresh]")
            .unwrap()
            .remove(0);
        let mut original = seed(data);
        let mut expected = Plan::default();
        original.plan(&form, &mut expected, &mut ok).unwrap();
        let mut count = 0;
        let mut probe = GraphInterner::with_prefix(&prefix, &mut ok).unwrap();
        probe
            .plan(&form, &mut Plan::default(), &mut |_| {
                count += 1;
                Ok::<_, Stop>(())
            })
            .unwrap();
        for stop in [Stop::Fuel, Stop::Irq] {
            for cut in 0..count {
                let mut candidate = GraphInterner::with_prefix(&prefix, &mut ok).unwrap();
                let mut plan = Plan::default();
                let mut at = 0;
                assert_eq!(
                    candidate.plan(&form, &mut plan, &mut |_| {
                        let now = at;
                        at += 1;
                        if now == cut { Err(stop) } else { Ok(()) }
                    }),
                    Err(stop)
                );
                assert!(plan.nodes.is_empty());
                candidate.plan(&form, &mut plan, &mut ok).unwrap();
                for (a, b) in plan.nodes.iter().zip(&expected.nodes) {
                    assert_eq!(
                        (a.key, a.metadata_count, &a.children),
                        (b.key, b.metadata_count, &b.children)
                    );
                }
                assert_eq!(plan.nodes.len(), expected.nodes.len());
                assert_eq!(prefix.len(), data.keys.len());
            }
        }
    }
}
