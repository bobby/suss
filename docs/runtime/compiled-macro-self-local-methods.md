# Named function locals in compiled macro environments

Named self locals now carry actual first-pass parameter declarations, callable
flags and maximum fixed arity before any method body expands. This supplies
`:fn-var`, `:variadic?`, `:max-fixed-arity` and `:method-params` to a source macro
inside the first method, including declarations from later methods. Argument
records retain present `:tag nil` and `:shadow nil` when unknown, plus their
selected declaration environment and binding information.

The compiler allocates real parameter identities using the same validation and
declaration operation as body analysis. First-pass records retain the original
shadow environment; body arguments are analyzed with the completed self facts.
Bodies still expand in textual order. Source method records remain separate from
physical recurrence bindings. Graph memoization distinguishes the initial and
completed self facts, and all existing transport bounds remain unchanged.

The parameter staging policy adapts `cljs/analyzer.cljc` lines 2208–2352 at pinned
commit `c4295f303100bbf5afac449242d30bca1126f1a1`, SHA-256
`297802c627474434f1ef868e31f5f9913c290a4e80c509a40c704dced95bbf47`.
The Rust module retains the upstream copyright/EPL notice; the distribution
includes [EPL 1.0](../../runtime/core-import/epl-v10.html). Native identities and
representations are original. No Java or JavaScript is added to shipped code.

The executing regression uses both unchanged named-self observations from
`tests/oracle/declaration-environment-observations.json`, preserving original
source coordinates. It compares all eight selected local fields. Method binding
data uses an explicit twelve-field view, with three selected environment fields
and two selected information fields. Presence, nil and collection kinds remain
distinct; the view does not certify arbitrary extension fields or whole ASTs.
Runtime and Macro callers retain the returned data after GC and actually invoke
the fixed and variadic functions.

Before the fix, both phases completed execution and reported 24 field
mismatches. The corrected focused test passes (one test, 70.15 seconds).
The native helper and caller each have a finite 100-million fuel fixture budget
for constructing the full quoted method-binding observation. Production fuel
defaults and graph/form limits are unchanged. Initial compiler changes were
rejected by the bootstrap identity guard; regeneration updates both manifest
fingerprints, with unchanged generated Wasm bytes.

```sh
CARGO_BUILD_JOBS=2 cargo test -p suss-cli --locked \
  --test compiled_macro_self_local_methods -- --test-threads=2
```

All 56 affected checks pass, including complete core projection and persistent
session behavior. Java-free bootstrap reproduction and four executing bootstrap
tests pass. The required full workspace baseline, independent review and exact
final-head CI remain pending for this branch.
Complete portable environments/inference, source/macro dependency cache
invalidation, evaluator retirement and original M3 lifecycle acceptance remain
open. This is partial progress toward issue #14.

Independent review also executes same-name self and duplicate argument shadows.
The body parameter shadows completed self callable facts, while the first-pass
parameter record retains the earlier self record. Both method contexts, maximum
fixed arity and textual expansion order match the retained actual pinned probe.
The additional native regression passes in both phases after GC, with actual
fixed and rest-argument calls. Production source and bootstrap identities are
unchanged by this review; full baseline and final-head CI remain pending.
