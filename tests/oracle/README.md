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

`reference.json` records an actual Node v24.5.0 execution with Java 21.0.2 and
Clojure CLI 1.12.1.1550. It is a reference-side transport fixture, **not a Suss
pass or differential baseline**. `scripts/oracle_transport.py` validates exact
schema, case identities, tags, ordered effects and selected numeric/string
boundaries. Run its malformed transport regressions with the ordinary Python
test suite. Regenerating the snapshot requires reviewing the resulting diff;
the runner writes only `out/observations.json`.

These source files and transport tests are original repository code. The
development compiler uses the existing pinned upstream submodule, whose source
and EPL notices remain intact; no upstream implementation has been copied into
the serializer. Java and Node are development tools, never shipped dependencies.

M1-02 still requires a shared source corpus, independent Suss lossless decoding,
comparison rules (including unordered maps/sets and NaN behavior), exact failure
stages/diagnostics, and differential regressions. The current production
integer/UTF-8 representation cannot claim the binary64/UTF-16 contract from these
reference results. CI currently checks the transport fixture and its regressions;
it does not build or run the JVM/Node oracle.
