# Bitwise dependencies for retained collection hashing

Checked bitwise primitives and bounded macro expansion now execute the scalar
hashing prerequisites. Complete collection hashing remains unfinished. The pinned
List/Cons hash path requires ordered hashing and Murmur operations before the
complete source types can execute. Preserve their Object blocks and persistent
representations; do not substitute the internal argument array for a list.

Pinned runtime source is core.cljs at c4295f303100bbf5afac449242d30bca1126f1a1:
int at 2942, bitwise forms at 2993–3064, int-rotate-left at 947 and the conditional
imul definitions at 955/956. Macro forms are core.cljc 1223–1273. The conditional imul region has explicit
[hash provenance](../compatibility/bitwise-primitives.json); the top-level inventory
does not discover those nested definitions, so no declaration IDs are invented. The retained bit-count and int-rotate-left forms have explicit hash-bound fixed
defn bootstrap patches, original algorithms/docstrings/metadata and EPL packaging.
Other runtime forms use original primitive adapters; they are not claimed retained
source. The oracle transport and probes are original.

`int` uses signed 32-bit wrapping. `long`, `unchecked-long` and `unchecked-int`
use the private fix function and must not be aliased to that conversion. Shift
counts mask to five bits; unsigned right shift returns a nonnegative binary64
number, including 4294967295. Multiplication must retain wrapping 32-bit output
rather than ordinary f64 multiplication followed by a lossy conversion.
Coercion should share the checked numeric runtime; object-to-primitive behavior
remains an explicit unsupported boundary until separately implemented.

The 54-case shared scalar corpus matches fresh pinned ClojureScript and independently
decoded validated native Wasm exactly, preserving the original 37 cases. It covers
coercions, shifts, wrapping multiplication, advertised direct/computed/variadic
calls, namespace qualification, redefinitions and old captures. Separate native
guards check aliases, exclusions, both compiler phases, located compile errors and
no partial publication. Unsupported-object conversion tests preserve the difference
between nested macro folds and computed calls' complete argument evaluation.

Direct bitwise macros use immutable primitive lowering. Variadic direct calls nest
pairs as the pin does; conversion of each pair precedes later operand syntax.
First-class values reside in canonical live cells and use universal Invoke;
computed calls evaluate all arguments before the function folds their internal
argument buffer. That buffer is private call storage, not a persistent source list.
The alias bit-shift-right-zero-fill preserves its own public cell. Resident native
bootstrap cells now number45; retained source loading additionally defines the two
hashing functions. All source selection/license bytes are reproducible:30 selected
forms,34 artifacts,113 partial reviews/952 unassessed. No review is marked complete.

Separate development diagnostics show captured JS function wrappers can supply
undefined for missing arguments and ignore extra arguments. For example captured
int with zero arguments yields 0 and with three arguments uses the first;
captured bit-and with one argument yields 0; captured imul with one yields 0.
These are not cases in the normal-arity compatibility corpus or native successes.
Direct calls have separate macro expansion behavior. The accepted wrong-arity diagnostic contract is retained: unadvertised calls are
located compile errors for direct macros or language arity errors for computed
functions. These captured JavaScript wrapper outcomes remain explicit differences,
not differential matches or skipped successes. Complete public arity compatibility
is not claimed.

An original private `coerce-int32` runtime intrinsic now reuses scalar coercion.
It rejects unsupported objects as language exceptions, returns zero for nonfinite
values, truncates fractions and computes an exact power-of-two remainder before
Wasm unsigned conversion. It changes no shared GC layouts, language cells or ABI
version. All 21 runtime ABI tests pass; the added regression covers 20 boundaries,
2,048 varied float encodings, scalar sentinels, UTF-16 parsing after forced GC and
typed opaque-object rejection followed by successful conversion.

Next independently review the PR, push significant findings, run the required
full baseline and require exact reviewed-head CI. Then continue retained ordered
hashing/Murmur dependencies and actual persistent sequence/list source. Full List/Cons,
Murmur hashing, equality, printing and compiled macros remain unfinished.
