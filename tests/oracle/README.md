# Development ClojureScript oracle

Run `scripts/test-oracle.sh` from any directory with Clojure CLI, Java and Node
installed. The runner requires a clean ClojureScript submodule at
`c4295f303100bbf5afac449242d30bca1126f1a1` (1.12.134); it compiles that source,
not a newer Maven ClojureScript release. Clojure 1.12.1 is explicitly pinned.
`-Srepro` excludes user dependency configuration. Maven/configuration caches use
task-specific `/tmp` directories; output is ignored under `out/`.

The original serializer uses DataView binary64 bits and JavaScript UTF-16 code
units. JSON never carries numbers as decimal floats or strings as scalar Unicode
text. Tagged maps preserve key/value pairs; sets, sequences and vectors retain
their collection kinds. Unknown values fail rather than becoming opaque success.
Transport schema 2 includes ordered effects and either a value or the exact
thrown value plus exception data and message. ExceptionInfo is tagged with data,
message and cause; unknown objects still fail. Schema 1 is rejected. Serialization happens in the observation try block, so a serializer
failure can appear as an exception observation; the strict corpus validator
rejects that substitution for the required value cases.

`cases.json` is the common source corpus. `scripts/oracle_cases.py` generates
development ClojureScript thunks under the ignored output directory; Rust reads
the same expressions and executes compiled core modules. The only different
code is each runner's observation wrapper. The Suss wrapper catches language throws with a separate Boolean discriminator
(so thrown nil/false cannot become a normal return), then returns outcome and trace for independent host GC decoding; it does not call guest equality,
sequence operations or printing to decide a result. Exact prototype integers
are converted to binary64 only if that conversion loses no bits. Inexact
integers and ratios are decoder failures, never silently rounded.

`reference.json` records an actual Node v24.5.0 execution with Java 21.0.2 and
Clojure CLI 1.12.1.1550. It is a reference-side transport fixture, **not a Suss
pass or differential baseline**. `scripts/oracle_transport.py` validates exact
schema, case identities, tags, ordered effects and selected numeric/string
boundaries. Run its malformed transport regressions with the ordinary Python
test suite. Regenerating the snapshot requires reviewing the resulting diff;
the runner writes only `out/observations.json`. The full command uses that fresh
Node result for the Suss differential test; ordinary Cargo/CI tests use the
checked-in reference fixture and need neither Java nor Node.

`scripts/oracle_compare.py` compares tagged values and exact effect order.
Collection kinds remain distinct; map/set iteration order is ignored, with
one-to-one element matching. Numeric bits are exact, including signed zero and
NaN payloads, except the two canonical arithmetic NaN signs are accepted as
permitted by WebAssembly. Raw bits remain in the observations, and storage/
boundary tests require exact NaN bits. This is observation comparison, not guest
numeric equality.
Malformed transport and absent cases fail. `known-failures.json` separately
records seven reviewed failures with exact stages/diagnostics or expected/actual
observations. A changed failure, new failure or unexpected pass fails the suite.
The present corpus reports **9 passing, 7 failing, 0 skipped**. A stable baseline
does not establish compatibility or satisfy the future numeric/string ABI.

Failures: large integer arithmetic returns 1 instead of the binary64-rounded
9007199254740992; unary subtraction loses negative zero; surrogate escape forms
fail parsing; quoted sequence and variadic rest values decode as vectors; and
`ex-info` is undefined. The old 201-case legacy baseline is unchanged. Manual
observation capture uses the explicitly ignored `record_observations` Cargo test
with `SUSS_ORACLE_OUTPUT`; it never updates expected failures automatically.

These source files and transport tests are original repository code. The
development compiler uses the existing pinned upstream submodule, whose source
and EPL notices remain intact; no upstream implementation has been copied into
the serializer. Java and Node are development tools, never shipped dependencies.

Caught strings, nil, false, zero and maps now have executing host-decoder
regressions. Their exception data/message are nil according to the pinned
ClojureScript non-Error contract, and their exact thrown values and partial
traces remain intact. Finally cleanup is verified on normal return, body throw,
catch-body throw and cleanup throw. WebAssembly traps remain failures distinct
from caught language throws; fuel/time failures are never language exceptions.

M1-02's bounded evidence-harness acceptance is complete after the merged
implementation audit in [acceptance-m0-m1.md](../../docs/roadmap/acceptance-m0-m1.md).
The named differential cases exist and their exact failures remain visible; this
does not certify compatibility or comprehensive semantic/arity coverage. Those
repairs remain M2/M4/M7 work. The GitHub issue remains open until the reconciliation
PR merges with its closing link. ExceptionInfo itself still fails at compilation;
implementing its data/message/cause and descriptor behavior belongs with M2's runtime exception foundation.
On a Wasm trap or failed decoding, the prototype cannot recover the partial
trace; that limitation is explicit. The current production integer/UTF-8
representation cannot claim the binary64/UTF-16 contract from these reference
results. CI checks transport/comparator regressions and executes the shared Suss
corpus against the reference snapshot; it does not build or run the JVM/Node
oracle. The ordinary 201-case legacy baseline remains separate and unchanged.

## Portable reader boundary

`scripts/test-reader-oracle.sh` executes a separate original 14-case scalar
corpus in the pinned compiler's tools.reader 1.3.6 dependency and Node, comparing
binary64 bits and UTF-16 units exactly. Rust reads the same sources and transfers
parsed scalars into generated ABI runtime objects, inspecting them after GC.
These are reader-boundary observations, not additional compiler compatibility
passes; the shared full-source corpus remains 9 passing/7 exact failures/0 skips.
The legacy compiler still uses its EDN reader. See [portable forms](../../docs/runtime/reader-forms.md).

## Portable compiler bootstrap

`scripts/test-portable-pipeline-oracle.sh` compiles and executes the separate
original 194-case `portable-cases.json` corpus in pinned ClojureScript/Node, checks
strict typed observations, then executes generated shared-ABI fragments in Rust.
It uses an ignored generated `.cljc` fixture so reader conditionals are allowed.
This source oracle covers the current scalar/let/do/if/numeric bootstrap only.
The 16-case legacy source corpus and 14-case reader corpus retain their separate
counts and purposes; known failures are unchanged. See the
[compiler contract](../../docs/runtime/portable-pipeline.md) for remaining work.

## Primitive numeric helper matrix

`sh scripts/test-numeric-oracle.sh` executes 1,024 unique binary64 bit patterns
through fresh pinned ClojureScript NumberToString/StringToNumber arithmetic, then
executes the embedded Wasm helper through the generated GC runtime and independently
inspects UTF-16 and f64 results. `numeric-cases.json` retains boundary cases and
deterministic sampled inputs with exact tagged expectations. Strict transport
rejects changed pins, duplicate/missing samples, boolean/schema/unit confusion and
wrong value tags. This bounded matrix does not certify every number or core API.
The source corpus additionally checks parsing grammar, whitespace, radix rounding,
UTF-16 concatenation and dynamic arithmetic. No JVM/Node enters the shipped path.
See [numeric build provenance](../../runtime/numeric/README.md).
