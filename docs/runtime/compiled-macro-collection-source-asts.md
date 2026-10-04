# Collection source AST evidence

Issue #14 requires genuine portable source analysis for compiled macros. The
analyzer now retains original analyzed collection children before constructor
and factory lowering. Native compiled macro projections match all fourteen
selected reference cases in both caller phases after GC, with the two explicit
correspondences below. Metadata expressions, `do`, global `set!` and source calls
retain their analyzed child records. Full issue and milestone acceptance remain
open; this evidence does not establish complete AST schemas or inference.

## Reference observations

The pinned ClojureScript revision is
`c4295f303100bbf5afac449242d30bca1126f1a1`. The original development-only projection
follows each analyzed node's declared `:children` edges, recording operation, tag,
form, field presence, literal flags and child cardinality. It does not reanalyze
forms or serialize entire environment/declaration maps.

Fourteen fresh analyzer traces matched executed Node output, including exactly
one initializer effect. The composed probe/checker command terminated successfully:

```sh
sh scripts/test-collection-source-asts-oracle.sh
python3 -m unittest discover -s scripts -p 'test_collection_source_asts_oracle.py'
```

The checker tests pass all nine checks. They reject replayed/missing effects,
Boolean masquerading as the effect count, missing edges, wrong cardinality, lost
cases, inconsistent absent fields, altered constant data, reordered unordered
children and malformed schemas.
The checked corpus remains raw reference evidence, including its edge ordering.

| Source node | Observed operation | Declared children | Observed tag |
| --- | --- | --- | --- |
| Vector, including empty and 33 items | `:vector` | `[:items]` | `cljs.core/IVector` |
| Map, including empty and nine pairs | `:map` | `[:keys :vals]` | `cljs.core/IMap` |
| Set, including empty and nine entries | `:set` | `[:items]` | `cljs.core/ISet` |
| Vector with meaningful metadata | `:with-meta` | `[:meta :expr]` | Absent on the wrapper |

Empty collections retain present empty child vectors. Constructor and factory
thresholds do not change the source operations. Nested, local-reference and quoted
children retain their own analyzed nodes. Metadata wraps a separate metadata map
and original collection node; a lowered `with-meta` call alone is insufficient to
prove this source structure. Quoted collections remain literal data under the
separate quote/const contract.

## Explicit portability boundaries

The pinned reader/analyzer enumerates unordered literals through hash iteration.
For the three-entry set the reference child order is `1, :key, false`; for the
nine-entry set it is `0, 7, 1, 4, 6, 3, 2, 5, 8`. Suss's approved contract preserves
textual evaluation order. Native comparison must document that correspondence
and test actual once-only effects; sorting effectful edges to hide a difference
would not establish conformance.

The effectful vector includes a `do`/`set!` initializer whose pinned arithmetic
expansion contains a JavaScript-specific `:js` node. The raw corpus keeps that
node, its expanded form and both argument records. Suss instead exposes the
genuine analyzed `(+ effects 1)` source invocation as `:invoke`, with `[:fn :args]`
children and a numeric result tag. The native test makes this one bounded
replacement in a copy of the expected projection, retaining both complete
argument records. Its callee is the resolved `+` var; the intrinsic has no source
declaration tag, so the selected callee tag is absent. This is an explicit
portable adaptation, not an exact JS AST compatibility claim. No JS syntax is
fabricated, no case is omitted, and the executed effect count remains exactly one.
Complete intrinsic declaration metadata remains part of the broader schema work.

## Implementation and verification

`SourceNode` retains analyzed vector/map/set entries, explicit `do` statements
and return value, global assignment target/value, and invocation callee/arguments.
Those records are captured at analysis boundaries, before storage and dispatch
lowering; graph construction never reanalyzes entries. Ordinary and arithmetic
calls retain source callees even when physical lowering uses an intrinsic or
specialized dispatch. Only explicit source `do` forms create `do` source records;
compiler-only bodies do not become source evidence.

Metadata has separate analyzed `:meta` and `:expr` children. The expression retains
the original form, lexical entry snapshot and expression context; the wrapper
has no raw collection/function tag. Metadata maps use a separate fact frame.
The same helper handles functions without requiring a collection record. Quoted
constants continue through their separate data/quote contract. Every new graph
recipe uses the existing charged, bounded graph builder.

The pinned assignment AST has no raw tag and no inferred tag unless the original
form has a hint; its value child remains independently analyzed and tagged. This
repair prevents a lowered assignment result type from becoming source evidence.

## Regression history and remaining gates

`compiled_macro_collection_source_asts` projects actual compiled-macro initializer
records in both caller phases and independently decodes results after GC. The
first parent run terminated before semantic assertions: its fixture used an
unavailable `vector` helper. The prepared repair uses a vector literal, normalizes
empty child sequences, and enforces the reference depth bound. The repaired run reached a semantic assertion and failed on empty-vector: its
initializer lacked `:op :vector`, `:children [:items]` and the present empty
`:items` vector. The repaired test terminated with zero passed and one failed
in 6.22 seconds. The original fixture compilation failure is retained separately.

`portable_collection_source_analysis` observes compiler expansion callbacks across
empty and small/large collection literals in both phases. It requires each marker
to expand once in textual order and preserves the enclosing source form, span and
tag. This supplemental compiler regression passed one test with zero failures in
0.02 seconds on the unchanged parent. It does not claim
runtime initializer behavior or replace the native macro/GC regression.

Compiler changes require regeneration of both phase bootstrap pairs and Java-free
reproduction before native acceptance tests. The latest control/metadata pairs
were regenerated; all four Java-free bootstrap tests passed in12.87 seconds.
The fourteen-case native test then passed in15.04 seconds, covering both caller
phases after GC. Three compiler tests passed in0.02 seconds: collection entry
capture, separate metadata/function records, and effectful do/assignment/intrinsic
children. Expansion callbacks run once; these compiler checks do not replace
runtime initializer evidence.

Independent review, significant fixes, the unfiltered workspace baseline and
exact final-head CI remain required before a PR is ready. All28affected native checks across8groups passed, including graph bounds,
source tags/hints, quote records, callable metadata and collection effects.
All150Python checks also passed. No issue closure, evaluator retirement, lifecycle
acceptance or M3 completion follows from this bounded increment.

Semantic references are pinned `cljs/analyzer.cljc` lines 4370–4398 and
4453–4463, `parse set!`2698 and source invocation analysis, and `cljs/compiler.cljc` collection emitters beginning at lines
546, 582 and 608 (upstream EPL-1.0). The development projection and regressions
are original code; no upstream collection implementation was copied here.

## Bounded projection and ordering evidence

The initial native recursive projection exhausted default fuel on33 children,
then on construction of all quoted results. A diagnostic probe confirmed
`OutOfFuel`; temporary logging was removed. The test uses an explicit finite
40-million fuel allowance for both operations. Shipped defaults, depth bounds,
frozen observations and semantic assertions remain unchanged.

The native expected projection applies fixture-specific permutations to the
small set and factory map/set constant children, retaining each complete child
and keeping map keys/values paired. The raw primary corpus is unchanged and the
strict checker rejects silent edge reordering. The additional executing unordered
literal test checks nine effectful set entries and nine effectful map pairs,
requiring source AST edge order and once-only textual runtime effects in both
phases. This test passed in both phases in the28-test affected run.

## Independent empty-do review correction

Review of PR #191 found that the implicit nil return of `(do)` used expression
context even inside a statement or function return. Pinned `parse-do` preserves
its enclosing context for empty/single-form bodies. The source capture now uses
that same context; empty do and its nil child both retain `clj-nil` source tags.
Four compiler checks cover all three contexts in both phases. Three executing
native collection checks pass, including empty-do context/constant/value presence
and nil after GC in both Stores. Graph/context checks also pass; both bootstrap
pairs reproduce without Java. Full baseline and exact final-head CI remain gates.
