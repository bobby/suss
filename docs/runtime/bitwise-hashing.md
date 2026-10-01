# Bitwise dependencies for retained collection hashing

This contains a tested private coercion prerequisite; public bitwise forms and
collection hashing are not implemented. The pinned
List/Cons hash path requires ordered hashing and Murmur operations before the
complete source types can execute. Preserve their Object blocks and persistent
representations; do not substitute the internal argument array for a list.

Pinned runtime source is core.cljs at c4295f303100bbf5afac449242d30bca1126f1a1:
int at 2942, bitwise forms at 2993–3064, int-rotate-left at 947 and the conditional
imul definitions at 955/956. Macro forms are core.cljc 1223–1273. No upstream form
is copied by this preparation; the oracle transport/probes are original.

`int` uses signed 32-bit wrapping. `long`, `unchecked-long` and `unchecked-int`
use the private fix function and must not be aliased to that conversion. Shift
counts mask to five bits; unsigned right shift returns a nonnegative binary64
number, including 4294967295. Multiplication must retain wrapping 32-bit output
rather than ordinary f64 multiplication followed by a lossy conversion.
Coercion should share the checked numeric runtime; object-to-primitive behavior
remains an explicit unsupported boundary until separately implemented.

The 37-case shared scalar corpus matches fresh pinned ClojureScript exactly.
The native regression currently reports 36 unresolved-name failures, with the
local-shadow case executing successfully. This is deliberately a red regression
on an unpublished preparation branch; no ignore, baseline acceptance, source
selection or inventory status is added. The decoder reads actual GC storage and
compares exact float bits/Boolean sentinels without Suss equality or printing.

Separate development diagnostics show captured JS function wrappers can supply
undefined for missing arguments and ignore extra arguments. For example captured
int with zero arguments yields 0 and with three arguments uses the first;
captured bit-and with one argument yields 0; captured imul with one yields 0.
These are not cases in the normal-arity compatibility corpus or native successes.
Direct calls have separate macro expansion behavior. Resolve this distinction
against the accepted wrong-arity diagnostic contract before certifying public
arity compatibility; do not infer minimum arity behavior from declarations alone.

An original private `coerce-int32` runtime intrinsic now reuses scalar coercion.
It rejects unsupported objects as language exceptions, returns zero for nonfinite
values, truncates fractions and computes an exact power-of-two remainder before
Wasm unsigned conversion. It changes no shared GC layouts, language cells or ABI
version. All 21 runtime ABI tests pass; the added regression covers 20 boundaries,
2,048 varied float encodings, scalar sentinels, UTF-16 parsing after forced GC and
typed opaque-object rejection followed by successful conversion.

Next implement bitwise/imul lowering and public source integration,
then retain portable runtime forms with explicit source/license/patch provenance.
Certify advertised direct, computed and variadic calls, namespace/rebinding,
ordered coercion, typed rejection and compile-atomic recovery. Full List/Cons,
Murmur hashing, equality, printing and compiled macros remain unfinished.
