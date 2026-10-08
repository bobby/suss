# Bounded Subvec acceptance draft (Refs #17)

Isolated branch `portable/m4-subvector`, based on
`3c80b606d5db721740d31bb16d99abf5fe4e1cc3`. This adds original development
fixtures and regressions. It does not replace or modify retained core forms,
import recipes/reviews, bootstrap artifacts, Cargo configuration, or other lanes.

## Actual retained implementation and provenance

The pinned ClojureScript revision is
`c4295f303100bbf5afac449242d30bca1126f1a1` (EPL-1.0; upstream license and
existing import notices remain in the pinned source/import tree). Complete
`Subvec`, private five-argument `build-subvec`, and public two-/three-argument
`subvec` already exist in `runtime/core-import/suss/core.sus`. Existing reviewed
adaptations retain methods and convert error construction to the portable typed
Error adapter, expand lazy sequence construction, and expand unchecked maximum.
No implementation port is needed for this bounded persistent-vector slice.

Existing source review identities/hashes:

| Form | Review identity | Source SHA-256 |
| --- | --- | --- |
| Subvec | runtime:Subvec:6077 | 461779bbd85a1a3e838660672bbe389a182a3330b7409c888e5706c7f4cd3754 |
| build-subvec | runtime:build-subvec:6218 | 489934fa5fb6315cca595b2cfcef0f0f345f61dfe0efab6a0c63bcc9647e9498 |
| subvec | runtime:subvec:6230 | afe5c911525ef228ccd942a2774ab5d33ab6d980e46166bf5b29606046d3921a |

These existing reviews remain in progress. The declared `seq-iter` fallback for
non-APersistentVector backing is still incomplete. Tests use actual persistent
vector backing; this draft does not claim arbitrary IVector iteration support or
complete #17 acceptance.

## Contract exercised

The original shared fixture has 65 elements and actual retained Subvec views.
34 public observations cover complete projected contents at start/end 31/32/33,
lengths 31/32/33, both public arities, empty views, nested views, metadata,
bidirectional equality against vectors/lists, ordered hash and metadata-insensitive
hash, relative lookup/invocation/reduction, pop/conj/assoc, and argument order.
11 additional pinned error-message observations cover invalid ranges, nil bounds,
nonvectors, empty pop, association bounds/key, and nth bounds.

Nested offsets are flattened *before* backing bounds validation in the pinned
implementation. A nested end beyond its parent view, or a negative relative start,
can therefore be accepted if the flattened range is valid in the backing vector.
The explicit observations preserve that behavior. Conj/assoc at a view's count
replace the next backing element when present, extending the view while leaving
the old vector/view unchanged; at the backing end they append.

Three native tests are authored for both macro and runtime phases. They decode
numeric observations directly from the ABI, and projected canonical vector
contents through FormBridge with exact binary64 bits. This is not direct Subvec
transport support. Separate checks read actual nominal backing fields, verify
nested flattening and metadata/pop backing identity, verify association changes
the backing while preserving an unaffected trie leaf, and root an original view
across removal of all fixture backing aliases and explicit GC. The retained view
is then invoked through a rooted projector and all 33 values are checked.
Bounds tests inspect typed Error descriptor 7 and exact UTF-16 messages after GC,
check recovery, and check invalid-call argument effects occur once in order.

Each native operation has a finite 100M allowance. It is a bounded 65-element
corpus, not a measured performance allowance; native execution must establish
sufficiency. No default production budget is changed.

## Evidence and remaining gates

- `sh scripts/test-subvector-oracle.sh`: 45 complete pinned observations match
  exactly, including error messages. Log: `/private/tmp/suss-m4-subvector-oracle.log`.
  The development runner verifies the exact checkout pin and forces
  `:cache-analysis false`; it runs no Cargo.
- `python3 -m unittest discover -s scripts -p test_subvector_oracle.py`: four
  comparator tests pass (complete ordered observations, mutations/omissions,
  duplicate/unknown fields, malformed tags, error IDs/messages).
- Rustfmt (edition 2024), shell syntax, and `git diff --check` pass.
- Independent read-only static review: no remaining material findings. The runner
  pin check was added; an initial `str_` dependency finding was retracted after
  verifying the actual retained definition and portable adapters.
- Native tests are **uncompiled and unexecuted** while baseline PID 65986 owns
  Cargo. Full baseline, bootstrap regeneration if independently required, and
  final-head CI remain pending. No commit/push or PR promotion is authorized here.

After baseline ownership is released, run
`cargo test -p suss-cli --test portable_subvector --locked -- --test-threads=2`,
resolve actual failures without skipping cases, then run
`cargo test --workspace --locked -- --test-threads=2`. Obtain final independent
review and final-head CI before treating this acceptance slice as passed.
