# Record helper fuel measurements

The exact source fixtures, process receipts, executable digests and stage traces
are retained here. `processes.json` compares the same source through `-e` and
script mode, without fuel flags: both before processes fail on fuel exhaustion;
both after processes return `17`. AOT compile/run is exercised independently by
`compiled_record_default_budget`.

| Consecutive definition | Before caller environment | After caller environment | Before invocation through decode | After invocation through decode |
| --- | ---: | ---: | ---: | ---: |
| `(defrecord R [a b])` | 14,638,260 | 977,581 | 16,345,348 | 2,774,977 |
| `(defrecord S [x])` | 14,654,648 | 992,352 | 15,660,353 | 2,090,371 |

The before executable comes from ef8826334b42392ecf1a02c62db29f59d73cc871.
The after executable has compiler-source fingerprint
389966c686cbe697e6bdab21613e8f49196e374060cbe98d314d3ff0801f0ac8.
Its private projection packs actual namespace/protocol membership and resolved
bindings; full compiler catalogs retain complete signature maps. The namespace
loading changes inherited from the parent branch do not enable this private
projection for ordinary/full macro environments.

These are fuel counts, not elapsed-time comparisons. The before profiling REPL
uses a 200,000,000 per-input cap to allow completion; the after REPL completes both
requests under a 4,000,000 cap. Invocation-through-decode excludes subsequent
analysis of the expansion and its nested macro calls. The actual four-million
process run establishes the complete-request upper bound. The unattended CLI
compilation default stays at 10,000,000; no allowance is raised.

Reproduce the profiling with `SUSS_MACRO_FUEL_TRACE=1 <executable> repl
--compile-fuel <cap> < results/m11-record-budget/repl-records.sus`. The raw logs
are named in `fuel.json`; the filtered stage logs here include all traced macros,
not only the two record invocations. The default fixtures can also be passed to
`<executable> <fixture>` or to `-e` as the complete source argument.

The test-only correction of the sort error assertion changes the compiler-source
fingerprint to a3a7a52dc6d4259c412ce258322fccabb878f3dcc068825fa5c072adbe13a225.
The measurements above retain their actual executable identity. They are not
relabelled as an artifact from that later source fingerprint. Final verification
must identify its own source separately.
