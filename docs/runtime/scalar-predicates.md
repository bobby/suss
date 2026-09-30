# Portable scalar predicate bindings

The shared runtime now supplies first-class `nil?`, `false?`, `true?`, `undefined?`,
`number?`, `string?` and `identical?` values in canonical `suss.core` live cells.
`cljs.core` aliases those cells. Calls use universal closure invocation and central
arity checks; operands are evaluated once in source order. Lexical bindings shadow
ordinary runtime names, current global reads observe redefinition, and captured
primitive closures retain their original behavior through GC.

`nil?` accepts language nil and the internal undefined value returned by missing
constructor fields. `undefined?` distinguishes that value from nil. False and true
predicates accept only their exact Boolean values; numbers (including NaN/infinity/
signed zero) and UTF-16 strings are identified by their shared runtime layouts.
Numeric zero and empty strings are not booleans. These are original Rust/Wasm
adaptations; no upstream form is copied and no JS/JVM enters the shipped runtime.

`identical?` follows pinned strict primitive identity. Binary64 comparison makes
+0 and -0 identical, and NaN nonidentical even when both operands refer to the
same Number box. Strings compare UTF-16 code units, so independently produced
strings with equal units are identical. Other language objects, functions and
descriptors compare runtime reference identity; equal fields do not make different
nominal objects identical. Nil and internal undefined remain distinct for identity.
The string loop inspects already evaluated values, without re-emitting operands.

Four native regressions independently inspect Boolean sentinels and numeric effect
traces after GC. They cover primitive distinctions, nominal/function identity,
canonical aliases/rebinding, retained original closures, callee/argument order,
wrong arity and subsequent recovery. The shared source corpus retains all prior
303 cases and adds 94 predicate/identity cases: 397 inputs total, with binary64/
Boolean/UTF-16 results decoded independently of Suss equality or printing. The
separate bootstrap-import corpus remains 14 cases; legacy full differential evidence
remains 9 passing/7 exact failures/0 skipped.

Pinned source provenance: ClojureScript 1.12.134 at
`c4295f303100bbf5afac449242d30bca1126f1a1`, core.cljs 241–258,307,2323–2339 and
core.cljc 923–937,988–998,1017–1035. Seven runtime entries record exact source hashes,
arities and macro dependencies in the manual overlay. They are in progress:
compiled predicate macro behavior/core import, source metadata and the complete
portable inventory remain unfinished. The macro entries themselves are unassessed;
these runtime function cells do not certify compiled macro bootstrap or all direct
call expansion/redefinition behavior of the upstream compiler. `fn?`/`ifn?`, other
persistent type predicates and collection equality/hash contracts remain future work.
The shared ten-type prelude, ABI 1, compiler format 2 and numeric helper stay unchanged.

```sh
cargo test -p suss-cli --test portable_scalar_predicates --locked -- --test-threads=2
scripts/test-portable-pipeline-oracle.sh
python3 scripts/cljs_reviews.py
```

These are core/collection prerequisites for issues #9/#16. Neither complete M2
compiler nor M4 core/collection acceptance follows from scalar coverage. Production
frontends and automatic core/macro bootstrap remain incomplete.
