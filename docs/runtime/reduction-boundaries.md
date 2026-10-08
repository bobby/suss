# Reduced callback boundaries

This original #19 acceptance slice targets vector termination at element indexes
31, 32 and 33. It does not certify all collection reduction paths or M4 completion.

The shared original fixture `tests/oracle/fixtures/reduction-boundary-probe.sus`
runs three modes: two-argument `reduce`, initialized `reduce`, and vector
`reduce-kv`. Each uses a 35-element vector. Callbacks assert ascending element
order and the expected incoming accumulator; `reduce-kv` additionally checks that
each key equals its element index. A callback beyond the selected stop throws
instead of allowing an extra effect to go unnoticed. At the stop it returns a
Reduced containing 17, nil or false. Four independent scalar observations check
the unwrapped result, callback count, next expected index and sum of visited values.
The two-argument reduction starts callbacks at index 1, so its count differs by
one from the initialized paths.

The corpus has 108 prewritten expectations. Fresh pinned ClojureScript 1.12.134
at `c4295f303100bbf5afac449242d30bca1126f1a1` matches every expectation. Command:
`sh scripts/test-reduction-boundary-oracle.sh`, exit 0; log
`/private/tmp/suss-m4-reduction-boundary-oracle.log`. The runner has no Cargo step
and uses the existing strict lossless Boolean/binary64 transport. The original
65-case reduction corpus is unchanged.

The native regression in `portable_reduction.rs` is authored for Runtime and
Macro Sessions, forcing GC after each evaluation and independently inspecting
the rooted result before comparison. Its finite 100M allowance is test-only.
Compilation and execution remain pending while the frozen vector workspace
baseline owns the Cargo slot. These reference observations do not establish
native success. No upstream implementation was copied; JVM/Node are development
oracles only. No inventory classification changes or ignored tests are added.

Next execute the focused regression, repair any actual failures, execute the
complete reduction binary and preserve old cases, then obtain the required
workspace baseline, independent review and green final-head CI before promotion.

Independent static review found no material findings. It checked all 108 arithmetic expectations, ordered callback and accumulator guards, nil/false terminal assertions, both-phase GC before decoding and byte-identical preservation of the original 65-case corpus. Native compilation and execution remain pending.
