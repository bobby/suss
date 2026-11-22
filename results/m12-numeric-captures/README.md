# Numeric capture storage probes

Original Suss compiler work; no upstream implementation body is copied. The
execution fixture follows the existing repository's both-phase/actual-AOT
harness. `oracle.clj` executes 18 independently asserted Clojure 1.12.6 values.
Only `catch :default` is adapted to JVM `catch Throwable`. The JVM JAR hashes
and command are in `oracle-command.json`; Clojure is a development oracle,
licensed EPL-1.0, with no shipped JVM dependency.

The private final closure subtype can contain an immutable i64/f64 field when
all methods use that capture only in inline numeric operations. Generic entry
parameters, source classes and checked source overloads stay unchanged. Values
returned, thrown, used for identity, captured again or consumed by unknown
operations retain object fields. CFG argument aliases are followed through
joins before deciding. Mixed-use methods share the conservative layout.
Unknown/guarded representations and resumable makers retain object fields.
Closures still allocate where the existing local-call pass cannot remove them;
this change does not implement letfn or eliminate recursive closure objects.

The first source snapshot is retained in `initial-emit.patch`,
`initial-captures.rs` and `initial-test.rs`. Its two code-generation tests fail:
the parent inference's literal-only escape pruning prevented a literal capture
from being selected raw, and the test's struct filter included unrelated shared
types. The correction supplies admitted raw capture uses to inference and
filters the actual shared closure supertype. Original output is in `initial.log`.

The first JVM attempt also retains a failed assertion: a literal long captured
by the JVM compiler is primitive and is boxed anew on each return. Shared-box
controls now explicitly capture `(identity 3000000000)`, an Object, and the
fresh JVM assertions establish its shared identity. See `initial-oracle.*` and
`initial-oracle-command.json`; no failed observation is recast as a pass.

`run-focused.py` records source hashes and terminal Cargo results.
`run-before-codegen.py` links the same raw-field regression against the frozen
5b40654f compiler libraries and requires the expected regression failure.
The corrected schema run still exposed two fixture assumptions: bare `=`
needed a declared cell in the standalone environment, and source signature
wrappers can introduce additional closures. `corrected.log` and its exact
source snapshots remain. The final schema targets the actual closure supertype
and checks a numeric field exists in each positive case; simple object-return
and join cases require object fields. A separate maker check requires exactly
one struct allocation, the closure itself. A later fixture compile attempted
to reference a private NUMBER constant; `private-constant-focused.*` records
that failure before the maker check was expressed without private constants.

Object-use demands propagate backward once over CFG argument aliases for all
captures in a method. This avoids a full graph scan per capture. Source layout
selection is conservative across all methods of each actual IR definition;
source signature wrappers can hold additional method closures.

Broader gates, benchmark measurements and their source reconstruction must be
inspected separately before claiming their results.

## Executed results

The five-target Cargo command passes 24 tests, zero failed/ignored: the new
three code-generation checks, both execution fixtures, captured-call controls,
14 closure tests and three primitive-entry tests. The 18 JVM pairs execute in
Runtime, Macro and actual AOT (54 observations). `focused-24-command.json` is
the terminal command/source receipt; `focused-24-summary.json` records the
captured terminal summary. The broad gate initially reused the success log
path; its live inode was renamed `broad-focused.log`, and future runner log
names are now distinct. The original failing logs remain untouched.

`source-check.py` verifies five exact inputs against reconstruction from
5b40654f. The before code-generation regression fails against that compiler's
immutable libraries, with the source/library hashes and command retained.
`measure.py` verifies the source both before and after measuring identical
benchmark forms with immutable before/after executable copies and component
hashes. The fresh JVM benchmark checks all three functions at 10, 1M and 2M.
All 24 RSS and 12 perf runs plus 12 warm runs assert their sums.

The numeric-only capture loop drops from about 920 to 904 estimated bytes per
iteration, and 8547.45 to 8463.61 instructions. The maker's one numeric box is
removed; named closure and source-signature wrapper allocations still dominate.
The object-return capture control stays about 920 bytes/8661 instructions;
the direct arithmetic control stays below RSS resolution/about 18 instructions.
These are instruction and allocation measurements, not quiet walltimes and
not removal of the closure itself.

`run-gates.py` runs 39 focused targets, Java-free bootstrap, Binaryen 133,
reviewed-suite comparison and the exact unfiltered workspace command with
stable Rust, two jobs/threads and a 4G memory scope. Inspect terminal receipts;
a started gate is not a pass. `commands.json` is written only after each stage
terminates. The code inputs must stay frozen while this process is live.

The frozen capture gate now records177 passing tests across39 targets, zero
failed/ignored, bootstrap4, Binaryen2 and reviewed-suite comparison1. The actual
2758 observations retain2407 pass/305 error/46 fail, losslessly compressed with
an uncompressed hash. Its unfiltered workspace stage is still active separately.
