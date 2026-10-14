# Portable transducer import

Fourteen complete pinned declarations and four complete iterator/setup parents
are selected with EPL-1.0 source/license and explicit patches. The Symbol feature
parent and es6-iterable/lazy-seq macros are retained. This target uses the original
`@@iterator` fallback; native Symbol and instance property lookup are not covered
by the retained execution evidence.

The 26 retained raw ClojureScript observations cover sequence's one/two/variadic
clauses, completing, eduction reductions and repeated traversal, multi-output and
completion buffering, nil/false, shortest input, throws, Reduced stopping and
31/32/33 effect boundaries. `portable_transducers` executes the original public
inputs through compiled macros in both Session phases and independently decodes
rooted ABI storage after GC. The focused execution log is
`/private/tmp/suss-m4-integrated-public-focused.log`. This is bounded evidence;
it does not establish every portable collection or reduction contract.

The complete pinned constantly, int-array and object-array dependencies are
retained as `dependency-<line>-<name>.cljs` with explicit patches. Both array
initializers preserve ClojureScript's non-coercing values and sequence-versus-fill
behavior. The 19-case `tests/oracle/array-initializer-*` corpus retains pinned raw
values and ordered effects, including sparse holes, short/long sequences and
vector-fill identity. The native comparison and sparse shift regression are in
`crates/suss-cli/tests/portable_transducers.rs`; the oracle capture alone does not
prove Suss behavior. Invalid-size and initialization-exception behavior are not
certified by these 19 cases.

The retained Jank drop_while source/excerpts carry MPL-2.0 and provide separate
primary source provenance. Whole-suite outcomes must retain failures, unknowns and
unexecuted assertions; supplemental cases do not stand in for unexecuted cases.
Issue acceptance criteria and completion evidence are tracked in GitHub.

`python3 scripts/check_transducer_provenance.py` verifies all fourteen whole
pinned declarations, the three complete dependencies, recipe/patch identities,
case-fold-safe paths and the public lazy-seq declaration-only adaptation.
`python3 -m unittest discover -s scripts -p test_transducer_provenance.py` contains
six mutation checks and one positive check covering missing/duplicate declarations, case collisions,
changed source with refreshed retained hashes, stale hashes and license changes.
These verify source retention, not native runtime acceptance.
