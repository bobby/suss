//! Fallible capture of the SAME native fields as SourceNamespace::capture.
//! No guest calls or reconstitution from guest values. Owned fields remain
//! unpublished until capture succeeds; error leaves Environment/factory intact.
//!
//! BTree bounds use Rust 1.96 alloc's B=6 node/search/split invariants (MIT OR
//! Apache-2.0), not guessed sizeof(BTreeMap) backing. Original metering model:
//! https://github.com/rust-lang/rust/blob/1.96.0/library/alloc/src/collections/btree/node.rs
//! https://github.com/rust-lang/rust/blob/1.96.0/library/alloc/src/collections/btree/search.rs
use super::{
    Binding, DefinitionInfo, Environment, Global, OwnerMeter, Phase, Scope, canonical, error,
    lookup_global,
};
use crate::portable::{Diagnostic, hir::SourceNamespace};
use std::{
    collections::{BTreeMap, BTreeSet},
    mem::{align_of, size_of},
    sync::Arc,
};

const KEYS: usize = 11;
const EDGES: usize = KEYS + 1;
fn add(a: usize, b: usize) -> Result<usize, Diagnostic> {
    a.checked_add(b)
        .ok_or_else(|| error("Namespace capture work overflow"))
}
fn mul(a: usize, b: usize) -> Result<usize, Diagnostic> {
    a.checked_mul(b)
        .ok_or_else(|| error("Namespace capture work overflow"))
}
fn pay(meter: &mut OwnerMeter<'_>, work: usize) -> Result<(), Diagnostic> {
    meter(u64::try_from(work).map_err(|_| error("Namespace capture work overflow"))?)
}
// Non-root nodes are nonempty; internal root has at least two children.
// bit-length(n)+1 bounds every current root-to-leaf path.
fn levels(n: usize) -> usize {
    (usize::BITS - n.leading_zeros()) as usize + 1
}
fn step(n: usize, meter: &mut OwnerMeter<'_>) -> Result<(), Diagnostic> {
    meter(16)?; // checked bound arithmetic BEFORE computing the step debit
    // Ascend+descend, index/edge/parent/length reads+branches, BEFORE next(),
    // including its final None. Bounds grow with actual native collection size.
    pay(meter, add(mul(levels(n), 2 * (EDGES + 4))?, 4)?)
}
fn begin_iter<I>(n: usize, meter: &mut OwnerMeter<'_>) -> Result<(), Diagnostic> {
    meter(16)?; // iterator/bound arithmetic before metadata acquisition
    pay(
        meter,
        add(mul(2, size_of::<I>())?, mul(levels(n), EDGES + 4)?)?,
    )
}
// Internal node upper bound: 11 K/V slots, 12 child links, parent, two u16s,
// alignment allowance at all six leaf/internal field joins. Covers leaves too.
// No private std memory is inspected/cast; this is not an exact node-count fee.
fn node_bytes<K, V>() -> Result<usize, Diagnostic> {
    let alignment = align_of::<K>()
        .max(align_of::<V>())
        .max(align_of::<usize>());
    add(
        add(
            mul(KEYS, add(size_of::<K>(), size_of::<V>())?)?,
            mul(EDGES + 1, size_of::<usize>())?,
        )?,
        add(2 * size_of::<u16>(), mul(6, alignment)?)?,
    )
}
fn clone_tree<K, V>(n: usize, meter: &mut OwnerMeter<'_>) -> Result<(), Diagnostic> {
    meter(4)?;
    if n == 0 {
        return Ok(());
    } // std clone's explicit empty fast path allocates nothing
    meter(64)?; // checked node/layout/count preflight, no allocation yet
    // At most n+1 nodes, conservatively including a possible empty root. Allocate+retire
    // backing and all entry initialization/retirement BEFORE clone allocates.
    let nodes = add(n, 1)?;
    pay(
        meter,
        add(
            mul(mul(2, nodes)?, node_bytes::<K, V>()?)?,
            add(
                mul(mul(2, n)?, add(size_of::<K>(), size_of::<V>())?)?,
                mul(nodes, 2 * (KEYS + EDGES + 4))?,
            )?,
        )?,
    )
}
fn text_eq(a: &str, b: &str, meter: &mut OwnerMeter<'_>) -> Result<bool, Diagnostic> {
    pay(meter, add(add(a.len(), b.len())?, 4)?)?;
    Ok(a == b)
}
trait Field: Clone {
    fn prepay_clone(&self, meter: &mut OwnerMeter<'_>) -> Result<(), Diagnostic>;
}
impl Field for String {
    fn prepay_clone(&self, meter: &mut OwnerMeter<'_>) -> Result<(), Diagnostic> {
        meter(8)?; // len and checked extent arithmetic, before reading/using them
        // Actual clone length: acquisition, copy, retirement; metadata separately.
        pay(
            meter,
            add(mul(3, self.len())?, add(2 * size_of::<Self>(), 4)?)?,
        )
    }
}
impl Field for Global {
    fn prepay_clone(&self, meter: &mut OwnerMeter<'_>) -> Result<(), Diagnostic> {
        meter(1)?; // established Global retain unit, before clone
        pay(meter, 2 * size_of::<Self>() + 4)
    }
}
impl Field for (String, String) {
    fn prepay_clone(&self, meter: &mut OwnerMeter<'_>) -> Result<(), Diagnostic> {
        self.0.prepay_clone(meter)?;
        self.1.prepay_clone(meter)
    }
}
fn clone_map<K: Field + Ord, V: Field>(
    source: &BTreeMap<K, V>,
    meter: &mut OwnerMeter<'_>,
) -> Result<BTreeMap<K, V>, Diagnostic> {
    pay(meter, 2 * size_of::<BTreeMap<K, V>>() + 4)?;
    let n = source.len();
    clone_tree::<K, V>(n, meter)?;
    begin_iter::<std::collections::btree_map::Iter<'_, K, V>>(n, meter)?;
    let mut input = source.iter();
    loop {
        step(n, meter)?;
        let Some((key, value)) = input.next() else {
            break;
        };
        key.prepay_clone(meter)?;
        value.prepay_clone(meter)?;
        // Later structural clone traversal, independent of this prepaid plan.
        step(n, meter)?;
    }
    Ok(source.clone())
}
fn clone_set<K: Field + Ord>(
    source: &BTreeSet<K>,
    meter: &mut OwnerMeter<'_>,
) -> Result<BTreeSet<K>, Diagnostic> {
    pay(meter, 2 * size_of::<BTreeSet<K>>() + 4)?;
    let n = source.len();
    clone_tree::<K, ()>(n, meter)?;
    begin_iter::<std::collections::btree_set::Iter<'_, K>>(n, meter)?;
    let mut input = source.iter();
    loop {
        step(n, meter)?;
        let Some(key) = input.next() else { break };
        key.prepay_clone(meter)?;
        step(n, meter)?;
    }
    Ok(source.clone())
}
fn global_text(global: &Global) -> Result<usize, Diagnostic> {
    add(global.namespace().len(), global.name().len())
}
// Native BTree insertion: <=11 comparisons per level, <=one new sibling per
// old level plus a new root. Allocate+retire node backing and two-node relocation
// bounds per level before insert. Actual incoming/max-existing key byte lengths
// bound comparisons; no guest equality/hash or spelling-derived authority.
fn insertion<K, V>(
    n: usize,
    incoming: usize,
    largest: usize,
    meter: &mut OwnerMeter<'_>,
) -> Result<(), Diagnostic> {
    meter(64)?; // checked search/split/backing preflight before computing bounds
    let path = levels(n);
    let comparisons = mul(KEYS, path)?;
    let compare = mul(comparisons, add(add(incoming, largest)?, 8)?)?;
    let nodes = add(path, 1)?;
    let node = node_bytes::<K, V>()?;
    let backing = mul(mul(2, nodes)?, node)?;
    let relocation = mul(mul(2, path)?, node)?;
    let controls = mul(path, KEYS * 12 + EDGES * 4 + 12)?;
    pay(
        meter,
        add(add(compare, backing)?, add(relocation, controls)?)?,
    )
}
fn push_global(
    output: &mut Vec<Global>,
    value: Global,
    meter: &mut OwnerMeter<'_>,
) -> Result<(), Diagnostic> {
    pay(meter, 2 * size_of::<Global>() + 8)?;
    if output.len() == output.capacity() {
        let next = if output.capacity() == 0 {
            4
        } else {
            output
                .capacity()
                .checked_mul(2)
                .ok_or_else(|| error("Namespace capture capacity overflow"))?
        };
        let bytes = mul(next, size_of::<Global>())?;
        // New backing+retirement, live relocation and old backing retirement,
        // before allocation. Requested-capacity policy matches metered buffers.
        pay(
            meter,
            add(
                mul(2, bytes)?,
                add(
                    mul(output.len(), size_of::<Global>())?,
                    mul(output.capacity(), size_of::<Global>())?,
                )?,
            )?,
        )?;
        output
            .try_reserve_exact(next - output.len())
            .map_err(|_| error("Unable to reserve namespace identities"))?;
    }
    output.push(value);
    Ok(())
}
impl Environment {
    fn snapshot_scope_metered(
        &self,
        phase: Phase,
        meter: &mut OwnerMeter<'_>,
    ) -> Result<&Scope, Diagnostic> {
        pay(meter, 12)?;
        let namespace = self.current_namespace(phase);
        // Same canonicalization, no owned lookup String/declaration Vec.
        pay(meter, add(namespace.len(), 13)?)?;
        let namespace = canonical(namespace);
        let n = self.scopes.len();
        begin_iter::<std::collections::btree_map::Iter<'_, (Phase, String), Scope>>(n, meter)?;
        let mut scopes = self.scopes.iter();
        loop {
            step(n, meter)?;
            let Some(((scope_phase, name), scope)) = scopes.next() else {
                break;
            };
            meter(4)?;
            if *scope_phase == phase && text_eq(name, namespace, meter)? {
                return Ok(scope);
            }
        }
        unreachable!("declared current namespace") // same missing-scope invariant
    }
    fn snapshot_declarations_metered(
        &self,
        phase: Phase,
        namespace: &str,
        meter: &mut OwnerMeter<'_>,
    ) -> Result<BTreeMap<Global, Arc<DefinitionInfo>>, Diagnostic> {
        pay(
            meter,
            2 * size_of::<BTreeMap<Global, Arc<DefinitionInfo>>>() + 12,
        )?;
        let mut output = BTreeMap::new();
        let mut largest = 0;
        let n = self.definitions.len();
        begin_iter::<std::collections::btree_map::Iter<'_, Global, Arc<DefinitionInfo>>>(n, meter)?;
        let mut definitions = self.definitions.iter();
        loop {
            step(n, meter)?;
            let Some((global, info)) = definitions.next() else {
                break;
            };
            meter(4)?;
            if global.phase() != phase || !text_eq(global.namespace(), namespace, meter)? {
                continue;
            }
            let incoming = global_text(global)?;
            insertion::<Global, Arc<DefinitionInfo>>(output.len(), incoming, largest, meter)?;
            global.prepay_clone(meter)?;
            pay(meter, 2 * size_of::<Arc<DefinitionInfo>>() + 5)?;
            // Exact immutable entry already is shared_definition_info's owner.
            output.insert(global.clone(), info.clone());
            largest = largest.max(incoming);
        }
        Ok(output)
    }
    fn snapshot_identities_metered(
        &self,
        phase: Phase,
        namespace: &str,
        meter: &mut OwnerMeter<'_>,
    ) -> Result<Vec<Global>, Diagnostic> {
        pay(
            meter,
            2 * size_of::<BTreeSet<Global>>() + 2 * size_of::<Vec<Global>>() + 16,
        )?;
        let mut cells = BTreeSet::new();
        let mut largest = 0;
        let mut insert =
            |global: &Global, cells: &mut BTreeSet<Global>, meter: &mut OwnerMeter<'_>| {
                // Filter the ACTUAL Cell/bootstrap value, never the bindings
                // map key: a foreign key may select a Global in this namespace.
                // Prepay controls/comparison before skipping unrelated inserts.
                meter(4)?;
                if global.phase() != phase || !text_eq(global.namespace(), namespace, meter)? {
                    return Ok(());
                }
                let incoming = global_text(global)?;
                insertion::<Global, ()>(cells.len(), incoming, largest, meter)?;
                global.prepay_clone(meter)?;
                cells.insert(global.clone());
                largest = largest.max(incoming);
                Ok::<(), Diagnostic>(())
            };
        let n = self.bindings.len();
        begin_iter::<std::collections::btree_map::Iter<'_, Global, Binding>>(n, meter)?;
        let mut bindings = self.bindings.iter();
        loop {
            step(n, meter)?;
            let Some((_, binding)) = bindings.next() else {
                break;
            };
            meter(4)?;
            match binding {
                Binding::Cell(global)
                | Binding::InternalCell(global)
                | Binding::ReaderCell(global) => insert(global, &mut cells, meter)?,
                _ => {}
            }
        }
        let n = self.materialized_bootstrap.len();
        begin_iter::<std::collections::btree_set::Iter<'_, Global>>(n, meter)?;
        let mut bootstrap = self.materialized_bootstrap.iter();
        loop {
            step(n, meter)?;
            let Some(global) = bootstrap.next() else {
                break;
            };
            insert(global, &mut cells, meter)?;
        }
        let n = cells.len();
        begin_iter::<std::collections::btree_set::IntoIter<Global>>(n, meter)?;
        let mut cells = cells.into_iter();
        let mut output = Vec::new();
        loop {
            step(n, meter)?;
            let Some(global) = cells.next() else { break };
            meter(4)?;
            if global.phase() != phase || !text_eq(global.namespace(), namespace, meter)? {
                continue;
            }
            // Same canonical borrowed hidden query, using existing actual paid
            // comparisons and exact error propagation, never an unmetered .get.
            pay(meter, add(global.namespace().len(), 13)?)?;
            let hidden = matches!(
                lookup_global(
                    &self.bindings,
                    phase,
                    canonical(global.namespace()),
                    global.name(),
                    meter
                )?,
                Some((_, Binding::ReaderCell(_) | Binding::InternalCell(_)))
            );
            if !hidden {
                push_global(&mut output, global, meter)?;
            }
        }
        Ok(output)
    }
    pub(crate) fn capture_source_namespace_metered(
        &self,
        phase: Phase,
        meter: &mut OwnerMeter<'_>,
    ) -> Result<SourceNamespace, Diagnostic> {
        pay(meter, 2 * size_of::<SourceNamespace>() + 32)?;
        let scope = self.snapshot_scope_metered(phase, meter)?;
        scope.namespace.prepay_clone(meter)?;
        let namespace = scope.namespace.clone();
        let aliases = clone_map(&scope.aliases, meter)?;
        let refers = clone_map(&scope.refers, meter)?;
        let used_refers = clone_set(&scope.used_refers, meter)?;
        let renamed_refers = clone_set(&scope.renamed_refers, meter)?;
        let excluded_core = clone_set(&scope.excluded_core, meter)?;
        let macro_aliases = clone_map(&scope.macro_aliases, meter)?;
        let macro_refers = clone_map(&scope.macro_refers, meter)?;
        let used_macro_refers = clone_set(&scope.used_macro_refers, meter)?;
        let renamed_macro_refers = clone_set(&scope.renamed_macro_refers, meter)?;
        let declarations = self.snapshot_declarations_metered(phase, &scope.namespace, meter)?;
        let identities = self.snapshot_identities_metered(phase, &scope.namespace, meter)?;
        let snapshot = SourceNamespace {
            namespace,
            aliases,
            refers,
            used_refers,
            renamed_refers,
            excluded_core,
            macro_aliases,
            macro_refers,
            used_macro_refers,
            renamed_macro_refers,
            declarations,
            identities,
        };
        // Cancellation arriving during a prepaid clone is checked before factory
        // Arc allocation/publication. Destruction on this error was prepaid too.
        meter(1)?;
        Ok(snapshot)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn environment(phase: Phase, unrelated: usize) -> Environment {
        let source = "(def ^number local 17) (def other 23)";
        let forms = suss_reader::forms::read_forms(source).unwrap();
        let (_, mut env) = crate::portable::hir::prepare(
            &forms,
            0..source.len(),
            &Environment::new("prefilter.user").unwrap(),
            phase,
        ).unwrap();
        let outside = env.declare_cell(phase, "outside.ns", "outside").unwrap();
        env.alias(phase, "external", "outside.ns").unwrap();
        env.refer(phase, "outside", "outside.ns", "outside").unwrap();
        env.refer(phase, "renamed", "outside.ns", "outside").unwrap();
        env.exclude_core(phase, "inc").unwrap();
        env.declare_macro_exports(phase, "external.macros", &["macro-entry".into()]).unwrap();
        env.macro_alias(phase, "mac", "external.macros").unwrap();
        env.macro_refer(phase, "macro-entry", "external.macros", "macro-entry").unwrap();
        env.macro_refer(phase, "renamed-macro", "external.macros", "macro-entry").unwrap();
        let local = env.resolve(phase, &suss_reader::Symbol::new("local"), 0..0)
            .unwrap().global().clone();
        env.materialize_bootstrap(local); // genuine binding/bootstrap duplicate
        let (reader, internal, _) = env.reader_sequence_cells(phase);
        env.materialize_bootstrap(reader);
        env.materialize_bootstrap(internal); // hidden even when also materialized
        let other_phase = if phase == Phase::Runtime { Phase::Macro } else { Phase::Runtime };
        for index in 0..unrelated {
            let cell_phase = if index % 2 == 0 { phase } else { other_phase };
            let cell = env.declare_cell(cell_phase, "outside.ns", &format!("unrelated-{index:04}")).unwrap();
            env.materialize_bootstrap(cell);
            // Same namespace but wrong phase also cannot enter the selected set.
            let cell = env.declare_cell(other_phase, "prefilter.user", &format!("opposite-{index:04}")).unwrap();
            env.materialize_bootstrap(cell);
        }

        // Representable private-map layout, using genuine declared Globals:
        // preserve cells()'s VALUE semantics even if key/value names differ.
        // Public alias()/refer() use Scope and do not themselves manufacture
        // this storage layout. No production invariant permits key filtering.
        let value_only = env.declare_cell(phase, "prefilter.user", "value-only").unwrap();
        assert!(env.bindings.remove(&value_only).is_some());
        let foreign_key = env.declare_cell(other_phase, "outside.key", "foreign-key").unwrap();
        env.bindings.insert(foreign_key, Binding::Cell(value_only));
        let key_only = env.declare_cell(phase, "prefilter.user", "key-only").unwrap();
        env.bindings.insert(key_only, Binding::Cell(outside));
        let bootstrap_only = env.declare_cell(phase, "prefilter.user", "bootstrap-only").unwrap();
        assert!(env.bindings.remove(&bootstrap_only).is_some());
        env.materialize_bootstrap(bootstrap_only);
        env
    }

    fn same_snapshot(actual: &SourceNamespace, expected: &SourceNamespace) {
        assert_eq!(actual.namespace, expected.namespace);
        assert_eq!(actual.aliases, expected.aliases);
        assert_eq!(actual.refers, expected.refers);
        assert_eq!(actual.used_refers, expected.used_refers);
        assert_eq!(actual.renamed_refers, expected.renamed_refers);
        assert_eq!(actual.excluded_core, expected.excluded_core);
        assert_eq!(actual.macro_aliases, expected.macro_aliases);
        assert_eq!(actual.macro_refers, expected.macro_refers);
        assert_eq!(actual.used_macro_refers, expected.used_macro_refers);
        assert_eq!(actual.renamed_macro_refers, expected.renamed_macro_refers);
        assert_eq!(actual.identities, expected.identities);
        assert_eq!(actual.declarations.len(), expected.declarations.len());
        for (global, info) in &expected.declarations {
            assert!(Arc::ptr_eq(&actual.declarations[global], info));
            let (actual_key, _) = actual.declarations.get_key_value(global).unwrap();
            assert!(Arc::ptr_eq(&actual_key.0, &global.0));
        }
    }

    #[test]
    fn namespace_identity_prefilter_matches_legacy_values_order_hidden_cells_and_info_owners() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let environment = environment(phase, 128);
            let generation = environment.source_generation();
            let expected = SourceNamespace::capture(&environment, phase);
            let actual = environment.capture_source_namespace_metered(phase, &mut |_| Ok(())).unwrap();
            same_snapshot(&actual, &expected);
            assert_eq!(actual.declarations.len(), 2);
            assert!(actual.identities.iter().any(|g| g.name() == "value-only"));
            assert!(actual.identities.iter().any(|g| g.name() == "bootstrap-only"));
            assert!(!actual.identities.iter().any(|g| g.name() == "key-only"));
            assert!(!actual.identities.iter().any(|g| g.name().starts_with("opposite-")));
            assert_eq!(actual.identities.iter().filter(|g| g.name() == "local").count(), 1);
            assert!(actual.identities.windows(2).all(|pair| pair[0] < pair[1]));
            assert_eq!(environment.source_generation(), generation);

            // Legacy capture also rejects real Reader/Internal values from
            // their own namespaces, including materialized duplicate entries.
            for namespace in ["suss.core", "suss.internal.reader"] {
                let mut hidden = environment.clone();
                hidden.enter_namespace(phase, namespace).unwrap();
                let expected = SourceNamespace::capture(&hidden, phase);
                let actual = hidden.capture_source_namespace_metered(phase, &mut |_| Ok(())).unwrap();
                same_snapshot(&actual, &expected);
                assert!(!actual.identities.iter().any(|g| g.name() == "sequence"));
            }
        }
    }

    #[test]
    fn namespace_identity_prefilter_meter_failures_release_owners_and_retry_exactly() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let environment = environment(phase, 8);
            let expected = SourceNamespace::capture(&environment, phase);
            let generation = environment.source_generation();
            let counts: Vec<_> = expected.declarations.values().map(Arc::strong_count).collect();
            let mut costs = Vec::new();
            let actual = environment.capture_source_namespace_metered(phase, &mut |work| {
                costs.push(work);
                Ok(())
            }).unwrap();
            same_snapshot(&actual, &expected);
            drop(actual);
            let total = costs.iter().try_fold(0u64, |sum, work| sum.checked_add(*work)).unwrap();
            for cut in 0..costs.len() {
                let mut call = 0;
                let result = environment.capture_source_namespace_metered(phase, &mut |_| {
                    let fail = call == cut;
                    call += 1;
                    if fail { Err(Diagnostic { span: 17..23, message: "namespace prefilter cancellation".into() }) }
                    else { Ok(()) }
                });
                let failure = result.unwrap_err();
                assert_eq!(failure.span, 17..23);
                assert_eq!(failure.message, "namespace prefilter cancellation");
                assert_eq!(environment.source_generation(), generation);
                assert_eq!(expected.declarations.values().map(Arc::strong_count).collect::<Vec<_>>(), counts);
                let retry = environment.capture_source_namespace_metered(phase, &mut |_| Ok(())).unwrap();
                same_snapshot(&retry, &expected);
            }
            for allowance in [0, 1, total / 2, total - 1] {
                let mut remaining = allowance;
                let failure = environment.capture_source_namespace_metered(phase, &mut |work| {
                    remaining = remaining.checked_sub(work).ok_or_else(|| Diagnostic {
                        span: 29..31, message: "namespace prefilter native fuel exhausted".into()
                    })?;
                    Ok(())
                }).unwrap_err();
                assert_eq!(failure.span, 29..31);
                assert_eq!(failure.message, "namespace prefilter native fuel exhausted");
                assert_eq!(expected.declarations.values().map(Arc::strong_count).collect::<Vec<_>>(), counts);
                let mut remaining = total;
                let retry = environment.capture_source_namespace_metered(phase, &mut |work| {
                    remaining = remaining.checked_sub(work).ok_or_else(|| error("retry native fuel exhausted"))?;
                    Ok(())
                }).unwrap();
                same_snapshot(&retry, &expected);
                assert_eq!(remaining, 0);
            }
            same_snapshot(&SourceNamespace::capture(&environment, phase), &expected);
            assert_eq!(environment.source_generation(), generation);
        }
    }
}
