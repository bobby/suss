# Registered effects slice

Original isolated evaluation code under repository MIT/Apache-2.0 terms; no
upstream source port. Synthetic graphs do not authenticate genuine HIR facts.
See [exact transport](../effects-schema.md).

The one-worker pinned exporter and existing opt builds succeeded. These commands
passed against the actual binaries:

```
python3 check-effects.py /private/tmp/suss-m4-mlir-toolchain/build-export-pinned/suss-mlir-export /private/tmp/suss-m4-mlir-toolchain/build/suss-mlir-opt
/opt/homebrew/bin/node check-oracle.cjs
```

Seven positive fixtures export and roundtrip; 17 retained negative inputs reject
without changing a sentinel output file. The pinned Node v24.5.0 source oracle
agrees with a host model of the actual exported graphs. This is development
oracle evidence, not Wasmtime execution. expected.json contains independent
result/journal/count f64 bit observations for Rust runtime checks.

| fixture | result | journal | condition count |
|---|---:|---:|---:|
| success | 7 | 124 | 1 |
| alternative | 8 | 174 | 1 |
| handled-throw | 17 | 1234 | 1 |
| cleanup-overrides-success | 99 | 12456 | 1 |
| cleanup-overrides-handler | 99 | 123456 | 1 |
| captured-f64 | 7 | 124 | 1 |
| nested-join | 7 | 124 | 1 |

Journal digits witness condition/body/handler/cleanup/outer-handler/outer-cleanup
order. Both selected and unselected arms are observable. The condition closure
increments a separate count exactly once. The captured f64 and nested join
examples exercise closure entry types and logical IDs after CFG splitting.

Existing numeric v1 and complete source-schema gates also passed after the
extension. Generated type parsing calls getChecked for effect_closure.
Native bridge compilation, actual artifact validation and all seven Wasmtime
execution cases now pass on current main; see [current evidence](../../evidence/). Absent handler/cleanup, arbitrary
exception payloads, suspension/cancellation, rooted continuations and complete
source integration remain outside this bounded slice.

Independent read-only review reproduced the seven positives/17 negatives on a
separate temporary fixture copy and found no material C++/schema issue. Six extra
malformed probes (cross-arm use, closure capture, read narrowing, cleanup arity,
extra cell field and empty cell name) rejected without touching output. Logs:
`/private/tmp/suss-effects-independent-review.log` and
`/private/tmp/suss-effects-independent-extra.log`. This review checks the bounded
export boundary, not execution semantics. Final pinned exporter CTest ran
suss-verified-graph-export and suss-effects-export: 2/2 passed, zero failures.

A fresh ClojureScript reference compile/run now strengthens the reference gate.
`mlir_effects.cljs` contains seven original source examples, compiled from the
existing clean `src/main` checkout at
`c4295f303100bbf5afac449242d30bca1126f1a1`, using cached Clojure 1.12.1
and Node v24.5.0. All result/journal/count bits exactly match the unchanged
expected.json and the exported graph model. Raw observations are retained in
cljs-observations.json. No upstream code was copied into these original probes;
ClojureScript remains an EPL-1.0 development oracle, not a shipped dependency.

Reproduce a fresh compile without Cargo:

```
python3 run-cljs.py --upstream /Users/bobby/code/github/bobby/suss/clojurescript --workdir /private/tmp/suss-m4-effects-cljs-oracle
```

The driver uses a separate output directory and a 512 MiB Java heap. It checks
both upstream and Node pins and compares rather than regenerating expected bits.
`check-cljs.py OBSERVATIONS --native EXISTING_BRIDGE_BINARY` additionally runs the
seven actual native graphs and requires exact observation schema, ABI gate,
result bits and cell snapshots. That native comparison was executed successfully; the current-base seven-case
native rerun also passes. Clean commands are in [reproduction](../../reproduce.md).
