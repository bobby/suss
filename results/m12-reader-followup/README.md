## UUID full-suite capture and EDN namespace dependency (2026-11-23)

`python3 results/m12-reader-followup/run-suite.py full-suite-after-uuid-and-offset-repair`
completes on d64d424f with all recorded inputs unchanged. Build and suite-lock
return0; the native full suite returns101 after580.924s because four namespace
diagnostics advance beyond unsupported UUID. The complete5002-row capture and
logs are retained; this is a failing baseline comparison, not a passing full suite.

`audit-suite.py full-suite-after-uuid-and-offset-repair` independently compares
that capture with the unchanged pinned primary, count/skip rules and a783ad60
parent. It preserves all4917 strict prior passes and separately all4932
individually comparable prior rows. Counts remain4842 host matches,75 guest
judgments,86 failures,1936 unexecuted, zero skips and72 failed namespaces.
The297 EDN assertions remain unexecuted: after UUID constants read successfully,
namespace loading reaches the missing `clojure.edn` source. Identity similarly
reaches unsupported `#inst`; parse-uuid and uuid? suite namespaces reach missing
`parse-uuid`. No baseline is written by this audit.

The auditor's parent-capture control reproduces the original outcomes. Its
negative control removes one actual Decimal-operand observation and requires
rejection, preserving the distinction between missing observations and passes.
These two controls are synthetic audit tests, not Suss execution evidence.
Actual Decimal operands remain194/200 passing with6 unexecuted; actual BigInt/
Ratio operands remain214/229 passing with15 unexecuted. Missing min-key, some-fn,
real eval and the EDN namespace remain explicit dependencies. A closed issue
label alone does not establish its suite acceptance criteria.

A shipped `clojure.edn` compatibility namespace now requires `suss.edn` and
exports its exact compiled `read-string` function in both phases. It introduces
no test-harness rewrite. `run-test.py clojure-edn-compatibility --test clojure_uuid
clojure_edn_namespace_exposes_the_same_data_reader_in_both_phases` returns0:
one test exercises Runtime and Macro, function identity, UUID/Decimal values,
EOF options, configured/default reader precedence and rejection of reader eval.
The311.143s command records unchanged inputs. This focused result does not
certify execution or passing of the complete EDN suite body; a fresh full-suite
capture follows. Earlier failures remain in their original captures.

## UUID source loading and compiler membership observations (2026-11-23)

`run-source.py typed-offsets-and-source-safe-uuid-table` passes144 focused tests
with all recorded inputs unchanged:30 UUID/record/source-core integration tests,
60 graph/storage/reader-budget tests, the unchanged artifact-size gate, and53
independent decoder/privacy/source-tooling tests. Membership work is361130 and
368956, below the original one-million limit; the four-million record limit also
passes. Artifact growth remains2352 bytes/form and96 extra split-form bytes,
within the unchanged3900/128 limits.

Compiler-generated membership offsets now use a typed long-vector recipe. This
preserves the exact vector/schema and UTF-16 boundaries while avoiding separate
reader-key planning for each structural offset. Wide signed values survive GC
in both phases. Final native key/storage tracing retains the earlier1026-entry
growth boundary. The first attempted index-sharing cache increased costs to
1020485/1167745 and was removed; its exact source patch and failed receipts remain
under `interned-membership-indices`.

Ordinary source-core loading previously threw `Mutable fields are private to
their type` while constructing the UUID parser's37-element vector. A private
primitive array now stores that immutable parser table. Source-core loading and
UUID reading pass in both phases without changing field-access policy. Existing
hash, reduction, vector, source-macro metadata and reader-binding regressions pass.

`run-roundtrip.py uuid-source-roundtrip-and-ci-regressions` passes27 additional
tests. The UUID AST records real `:const`/`:uuid` values; native EDN and emitted
source round-trip losslessly, and the emitted source executes in both phases.
The remaining visitor/projection arms preserve typed UUID bits. The retained
when-let provenance hash is aligned with UUID binding-key hashing; the checker
and its captured upstream/native evidence remain intact.

The original CI45175714660 failures and all exploratory failures remain retained.
Full-suite/affected EDN capture, complete workspace execution, final-head CI and
independent promotion review still need their own results. The prior foundation
run passes15 checks, including364 Python tests, on its recorded snapshot; further
source changes require their final check. No complete BigDecimal or milestone
acceptance follows from these focused gates.

## Compiled UUID integration controls (2026-11-23)

`run-source.py after-uuid-duplicate-format-and-macro-registration` passes all85
reader-crate tests and all8 compiled integration tests, with every recorded input
unchanged. The172 original UUID expression controls,235 runtime parser/error
controls and12 signed-boundary/collection/quotation controls execute against fresh
JVM Clojure1.12.6 results in both Runtime and Macro. Actual CLI AOT compiles and
executes the184 expression/boundary controls. Canonical128-bit fields survive GC
in both phases with public equality/hash/printing disabled; malformed/foreign
storage remains rejected by the independent decoder. Field/prototype sealing and
private helper guards pass too.

The source reader retains a typed UUID scalar; quotes, source emission, native
EDN, macro data, raw source keys and analyzer graphs preserve signed halves.
Original core code selects explicit64-bit operations without changing the32-bit
operations used by existing ClojureScript ports. The full positive signed-long
parser range is retained. UUID duplicate diagnostics use canonical `str`, matching
the JVM. Whole upstream class forms, adaptation patch, predicate source, EPL
notice and exact block hashes are retained by `provenance.py --check` and
`docs/compatibility/bootstrap/clojure-uuid.json`.

Earlier compilation, fixture,32-bit truncation, parser-boundary and duplicate
message failures remain in their separate terminal receipts and logs. None is
counted as a pass. New focused integration evidence does not establish the
complete affected EDN body, unchanged full-suite comparison, record/AOT budgets,
full workspace baseline, final-head CI or independent promotion review. Those
checks still need execution. Regex, instants and real runtime eval remain separate
numeric-suite dependencies; complete BigDecimal acceptance is unchanged.

# UUID and regex reader dependency observations

## Native tooling EDN UUID observations (2026-11-23)

`run-focused.py native-edn-uuid-after-source-projection` passes all four focused
UUID tests. Native EDN now retains typed128-bit values, canonical printing/key
duplicate checks, configured-reader precedence, builtin-before-default behavior
and lossless source-data projection. Serde rejects unsupported UUID conversion
rather than silently turning it into a string. Original missing match-arm
compilation failure is retained in `native-edn-uuid-gates`.

`run-reader.py native-edn-all-reader-gates` passes all84 reader-crate tests with
zero failures/ignored tests and recorded inputs unchanged. This is native tooling
EDN evidence only. Source reader selection, compiled Runtime/Macro UUID values,
public predicates/functions, macro transport, retained-value/GC and actual AOT
still require integration. Earlier native-parser-only results retain their own
source snapshots; none establishes complete UUID or BigDecimal acceptance.

`python3 results/m12-reader-followup/oracle.py` records204 fresh pinned JVM
Clojure1.12.6 expression controls. UUID inputs include short, upper-case,
plus-prefixed, over-width and Unicode hexadecimal groups; regex observations
retain raw pattern text and identity semantics. These are reference results,
not claims that Suss implements either runtime value yet.

`run-before.py` executes a copied pre-repair source2254f70c CLI with binary
SHA256969bbcd27acb249c24c2d52ed43be6727c7f1e3685a508883fca39fdd3effff3.
All13 selected native/AOT commands fail explicitly: unknown uuid tag or unsupported
regex dispatch. Input text and the exact commands/output are retained. They are
reader/compilation failures, not executions of macro or UUID runtime values.

`oracle-uuid-parser.py` independently records235 UTF-16 UUID input cases from
actual `java.util.UUID/fromString`:179 accepted and56 rejected. Complete signed
halves, canonical text, hashCode and error-message UTF-16 units are retained.
The same JVM records Character.digit(char,16) for all65536 units. Source/JAR
hashes, OpenJDK bytecode and commands qualify these primary observations. The
initial giant inlined fixture exceeded the JVM method-size limit; its source,
terminal failure and command remain under `uuid-parser-oracle-too-large`.
The subsequent recording reads the same data as EDN rather than embedding it
in one bytecode method; no primary cases were dropped.

`run-focused.py uuid-parser-gates` passes all three focused Rust tests with
unchanged inputs. The original parser retains immutable128-bit data and lossless
UTF-16 errors, masks accepted groups only after checked signed-long parsing,
compares signed halves, and exactly matches the full BMP hexadecimal scan.
It has no Java dependency. This establishes a native parser/data helper only:
compiler forms, native EDN, Runtime/Macro values, readers, public functions,
macro transport, retained-value/GC checks, AOT and affected suite execution still
need integration. No issue acceptance, workspace or CI pass follows from these
three focused tests. Original BigDecimal and suite baselines remain unchanged.
