# Retained integer-bit cache execution evidence

The compiler retains the original Number allocation for escaping, multiuse or
effect-separated SSA producers and caches its immutable integer bits for later
same-block integer consumers. Unknown coercions retain their original order;
continuations and integer uses across blocks decline this optimization.

- Integration base `dd03d0739954d9ba91642f82bc61e07a718fb891` incorporates merged
  main `0d53908` (#267). Independent source review approved the planner and merge
  resolutions. The new cfg(test) when-let validation borrows its decoded Form;
  the destructuring whole-file provenance hash reflects the reviewed borrow.
- `python3 run-boxed-integer-bits-focused.py --plan boxed-integer-bits-focused-v3-plan.json --execute`
  (retained driver/plan/commands in the archive): 44 Rust executions, 40 pass,
  four fail, zero ignored; 41 unique names, 37 pass/four fail. Three decoder
  names repeat across binaries. Compiler bitwise 5, artifact cache 12, CLI
  bitwise 10, object coercion 2, let destructuring 5 and pinned when-let 1 pass.
  Compiled when-let is 5 pass/1 fail. Its failure expects the older error prefix;
  actual error retains the intended language exception with compiled-invocation
  stage attribution. Three unchanged record/reduction witnesses still exhaust
  fuel in caller &env transport before their criterion observations.
- Pinned when-let executes all 12 ordered assertion IDs with independent host
  comparison. The unchanged bitwise corpus executes 70 pinned observations.
  Supplemental CLJS compilation/execution at clean pin
  `c4295f303100bbf5afac449242d30bca1126f1a1` returns four true observations for
  the exact new alias/capture/effect/recursion/global-reload expressions. All
  2,986 frozen inputs and regenerated bootstrap correspondence are retained.
- The completed source/domain/storage/raw-key gate passes 59/59. All 220 ordered
  source-child labels/results match the preceding candidate. Both Runtime and
  Macro requests retain the unchanged 10,000,000 allowance and finish at
  105,471 rather than 79,452: actual cost 9,894,529 rather than 9,920,548,
  removing 26,019 units per request. This is no whole-record/CoreEnv cost claim.
- The first focused attempt is retained: bootstrap and five IR tests pass,
  then lib-test compilation fails on the moved Form field. The retry fixes that
  borrow. Six core provenance checks pass; the initial when-let whole-file hash
  failure and corrected passing check are both retained. Oracle captures were
  not rewritten. The staged-diagnostic assertion correction is reviewed but
  unapplied while the preservation producer holds the source freeze.

The archive contains completed evidence only. Remaining preservation gates
(CORE17, full pinned map, cold identifiers, exact when-let validation and the
remaining 21 original collection selections), the candidate full workspace
baseline and final-head CI are not established by this checkpoint. The existing
three fuel failures remain unresolved. Next command already running against
this exact source is `python3 run-boxed-integer-bits-preservation.py --execute`;
its results must be appended separately after terminal source verification.

Evidence: [retained execution archive](m4-boxed-integer-bits-checkpoint-2026-10-17.tar.gz), SHA256
`266d1ce35b4014cbe9eb67fdcf20220cd2dcec0784f0aecb34ef7412e7cdc88c`. Payload hashes and source-path-to-blob maps are internal;
`input-blobs/<sha256>` resolves both focused runs' CAS maps. Initial failures,
raw terminal logs, observer sources/compiled JS, commands, source/bootstrap
bytes and independent review receipts are preserved. Remaining live gates are
excluded rather than reported successful.
