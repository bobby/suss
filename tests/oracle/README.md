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
Each observation includes ordered effects and either a value or exception data
and message. Serialization happens in the observation try block, so a serializer
failure can appear as an exception observation; the strict corpus validator
rejects that substitution for the required value cases.

`cases.json` is the common source corpus. `scripts/oracle_cases.py` generates
development ClojureScript thunks under the ignored output directory; Rust reads
the same expressions and executes compiled core modules. The only different
code is each runner's observation wrapper. The Suss wrapper returns the result
and trace for independent host GC decoding; it does not call guest equality,
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
NaN payloads; this is observation comparison, not guest numeric equality.
Malformed transport and absent cases fail. `known-failures.json` separately
records seven reviewed failures with exact stages/diagnostics or expected/actual
observations. A changed failure, new failure or unexpected pass fails the suite.
The present corpus reports **5 passing, 7 failing, 0 skipped**. A stable baseline
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

M1-02 still requires successful language-exception data/message and effects
observation (current `ex-info` fails at compilation), broader semantic/arity
regressions and repairs of the recorded incompatibilities. On a trap or failure,
the prototype cannot independently recover the partial trace; that limitation
is explicit. The current production
integer/UTF-8 representation cannot claim the binary64/UTF-16 contract from these
reference results. CI checks transport/comparator regressions and executes the
shared Suss corpus against the reference snapshot; it does not build or run the
JVM/Node oracle.
