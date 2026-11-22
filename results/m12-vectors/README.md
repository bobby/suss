# Guarded literal-vector storage

The scalar path preserves already selected source overloads and declared generic
lookup parameters. It duplicates the admitted continuation after source typing,
replacing private in-bounds vector projections with their evaluated elements.
The original constructor and lookup path remain on a guard miss. Element effects
run once in their original order. Undefined projected elements take the original
path. The guard checks current canonical vector registration on every call and
caches only successful immutable constructor/schema admissions at the fragment
site. No shared ABI types or runtime-global layouts change.

The bounded pass handles one eligible literal vector of 1–31 elements per
function, with constant in-bounds projections and no escaping vector use.
Unknown/out-of-bounds indices retain materialization. Earlier CFG boundaries
must dominate the constructor; copied continuations retain generic SSA types.
Copying shares an allowance derived from the original graph size. Normalized
async/region/future bodies remain conservative. MapEntries and `letfn` are
separate limitations, with unchanged MapEntry measurements retained below.

`source-check.py` reconstructs the eight compiler/test inputs from the committed
base and `source.patch`, then checks both reconstructed and current bytes.
Historical uncached sources can be reconstructed with `--manifest
uncached-source-inputs.json --patch uncached-source.patch --reconstructed-only`.
`cached-measured-source-inputs.json` and its patch freeze the measured source.
The additional constructor-rebinding control changes only the CLI test file;
`added-control-source-proof.json` verifies unchanged production hashes and the
same actual compiler binary. No timing claim follows from source reconstruction.

The cached 18-case fixture passes both execution tests in Runtime/Macro and
actual AOT, plus both IR/encoding tests: four tests, zero failed or ignored.
`vector-cached-focused.log` retains that result. Seventeen exact value pairs in
`cases.json` match fresh JVM Clojure1.12.6 (`oracle.clj`, `assertions.clj` and
logs): Object casts after destructuring, NaN arithmetic, signed zero, overflow,
argument effects/throws, root rebinding, escaping values, closures and identity
of an aliased atom. The nineteenth constructor-control case was subsequently
added from the frozen Suss binary. Both vector execution tests also pass that
expanded fixture:57 Runtime/Macro/AOT observations. `focused-progress.log` and
its JSON receipt retain this partial gate snapshot; it is not a terminal result
for the broader36-target run.

The NaN alias probe is a known **JVM difference**, not a conformance success.
JVM gives `[true false true]` for `[(= a a) (== a a) (identical? a a)]` with one
NaN binding; the frozen Suss compiler and its documented scalar predicates give
`[false false false]`. The initial failed tests/oracle and baseline executable
hash remain retained. The separate boxed-path control checks that scalar storage
does not change that existing behavior; it does not change the JVM expectation.

A separate initial failure exposed `nth` root rebinding in sessions: frozen
Runtime and Macro both return14 for a vector destructuring probe, versus JVM's3.
The shipped core definition now gets the vector indexed path in sessions too,
while lexical/unknown definitions retain ordinary calls. Clojure's actual
`:inline`/`:inline-arities` metadata, pinned source/JAR hashes and EPL1.0 license
are recorded in `nth-provenance.json`. No JVM dependency or copied core body is
introduced. The initial failing gate and independent session probe remain.
MapEntry's analogous redefinition probe still differs and is retained separately.

`measure.py` compiles identical four-case benchmark source with the frozen
f5056ffe compiler and the measured vector compiler. Both native GC modes are
warmed. Every RSS/perf output sum is asserted (48 measured runs,16 warm checks).
Two null-GC RSS slopes at1M/2M iterations estimate vector allocation at
167.96/168.94 bytes per iteration before and below resolution after. User-space
instruction deltas at100K/200K fall from1628.61 to449.00. Direct and closure
controls remain below RSS resolution at about28 instructions; MapEntry remains
about136 bytes and5579 instructions. These are allocation/instruction samples,
**not quiet walltimes**. Raw commands, output and compiler/component hashes are
in `measurement.json` and the before/after logs. The first uncached guard also
removed vector allocation but increased instructions to2771.08; its unsuccessful
performance result, exact source and binaries remain retained.

Run `python3 results/m12-vectors/run-gates.py` with stable Rust, two Cargo
jobs/threads, MemoryMax4G, the isolated `m12-aggregate-target`, outer `target/suss`
and Binaryen133. It selects36 focused targets, then Java-free bootstrap
reproduction, Binaryen execution and the reviewed suite comparison. Each actual
exit status is retained; inspect completion before treating this broader gate as
passed. The old unfiltered workspace run continues separately on frozen f5056ffe.
These bounded observations do not establish complete numeric/escape coverage.
