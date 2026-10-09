# Bounded MLIR evaluation (#188)

The completed post-#228 rerun at PR #227 head `b82ad94` passed all nine
execution gates: 18 Rust tests, genuine source-v2 original/mutation WasmGC,
seven pinned/native effects examples and 92 paired GC executions. The required
workspace baseline passed 1,550 tests with zero failures and 41 existing ignores;
all ten exact-head CI checks succeeded before user merge as `f4b13f3`.
See the [post-#228 acceptance/evidence audit](evidence/post228/README.md),
[decision](decision.md), [clean reproduction](reproduce.md) and [portable evidence](evidence/).
Recommend deferring adoption. No new execution is inferred from evidence packaging;
this documentation update still has its own review/baseline/CI gates. Historical
stages below retain failures and their superseding results.

This experiment leaves shipped compilation paths unchanged. Toolchain acquisition
is pinned in `toolchain-lock.json`; its archive was SHA-256 verified and extracted
on macOS ARM64. `mlir-opt --version` reports LLVM 23.1.3, and `--show-dialects`
includes wasmssa. Headers, libraries, `mlir-tblgen`, and MLIR CMake configuration
are present. These checks establish tooling availability, not WasmGC compatibility.

The unpacked SDK occupies 7.6 GiB. The package uses a 1 GiB Zstandard window;
inspection/extraction used `zstd --memory=1024MB -dc ARCHIVE | tar ...` with
pipe failure propagation. The platform tar could not decode the archive with its
default window. No LLVM source build was needed.

## Executed dialect and source-schema checks

The registered ODS/TableGen Suss dialect builds against the SDK. Closure types
use the generated checked parser (`getChecked`), require exactly one result,
and admit only f64 or recursively valid closure inputs, results and captures.
Bodies require exactly one block before access; recursive operand provenance
checks enforce explicit captures through nested foreign regions. Independent
repair/type review found no remaining material flaw; the type reviewer also
executed eight bounded probes.

The current command
`python3 experiments/mlir/fixtures/check.py PATH/TO/suss-mlir-opt` passes four
positive print/parse fixtures, 60 ordinary negative fixtures (29 source-schema
cases), and the recursive foreign-region isolation rejection. The 31 ordinary
type/operation negatives plus that isolation probe comprise the earlier 32-check
verifier gate. The combined runner additionally compares all selected-facts
fields and their attachments/locations across print/parse, verifies metadata and
presence mutation storage, and rejects oversized/deeply nested analysis payloads.
The one-worker opt rebuild exited 0. Focused CTest
`ctest --test-dir BUILD -R '^suss-dialect-fixtures$' --output-on-failure --parallel 1`
passed 1/1. Retained logs are
`/private/tmp/suss-analysis-validator-build.log` and
`/private/tmp/suss-analysis-validator-fixtures.log`.

`src/Analysis.h` validates the exact `suss.source-analysis.draft.v1` envelope
from [the source-analysis draft](bridge/analysis-schema.md). `suss.analysis` is a
JSON string attachment admitted only for const/literal-f64 and closure/closure
compatible tags. A module's `suss.source_analysis` string is a separate selected
source-tree sidecar. Legacy unversioned dictionaries reject. Validation covers
unknown/missing/duplicate decoded keys, lossless tagged forms and metadata,
integer spans, optional false/nil/absence, ordered embedded source children,
declaration/reference consistency, unique declarations, lexical child/method scopes,
local symbol resolution, shadows, captures and parameter/closure arity facts.
`binding:N` declaration identities, numeric `hirBindingId` and runtime graph SSA
IDs remain distinct; source facts are never inferred from MLIR SSA or signatures.

These source fixtures are explicitly **synthetic schema/storage witnesses**.
Complete payload preservation is demonstrated, but shape and attachment-kind
validation do not authenticate correspondence with actual analyzed HIR. The
source-analysis author independently reviewed the C++ validator and fixtures;
semantic depth accounting and local-name/declaration findings were repaired,
with no further material static defect reported at that stage. A second independent
review reproduced duplicate declarations, unbound method arguments, local-name
resolution disagreement, closure arity disagreement and literal resolution. Those
were repaired with focused negatives; nested child scopes and lexical shadows
are also checked. The expanded gate passes. Mencius independently rereviewed the final scoped
repair and reran its original malformed probes and full fixture gate; all
reported negatives reject and no further material bounded schema finding remains.
Its retained log is `/private/tmp/suss-analysis-independent-final.log`.
Rust `compare_transport` now has an authored recursive unique-key JSON visitor,
including escaped-equivalent duplicate keys, with three focused parser-only unit
tests (including recursion overflow). Independent static review found no concrete
material flaw; its recursion-test coverage note was addressed. Rustfmt syntax
parsing passed without editing the module's formatting;
Rust has now compiled and its parser-only unit tests passed in the third isolated
run. The later fourth isolated unit run passes all 15 tests, including the repaired
genuine source/global case. Complete genuine facts now also survive two registered
MLIR parse/print rounds; the later 18-test gate and actual source-v2 execution passed before-main.
Current-base rebuilding and the 18-test/v2 gates now pass. The draft's diagnostic initializer probe is a bounded
single-expression workaround, not a public analysis-only compiler API. Review
also repaired its plain analyze path to select an existing source child rather
than a source-less compiler fragment wrapper; the historical failing source-analysis gate below is superseded by the
repaired before-main 18-test and source-v2 evidence.

## Executed runtime-graph exporter checks

The separate exporter was built with pinned Clang 23 and ld64.lld in
`/private/tmp/suss-m4-mlir-toolchain/build-export-pinned`, using one CMake worker.
Its current-source rebuild exited 0. The actual exported original and separately
mutated MLIR graphs pass the bounded exporter checker, including exact JSON,
three raw binary64 storage probes and nine rejection probes that preserve the
output sentinel. The checker has per-process timeouts. Commands and retained
results are in `export-{configure,build,check}.log` under the toolchain directory.
The exporter was refreshed again after the final schema-header repairs, with
one worker and exit 0; its nine-rejection checker also exited 0. These final logs
are `/private/tmp/suss-analysis-export-final-build.log` and
`/private/tmp/suss-analysis-export-final-check.log`.
The latest gate was independently rerun successfully:

```
python3 experiments/mlir/bridge/export-fixtures/check-export.py \
  /private/tmp/suss-m4-mlir-toolchain/build-export-pinned/suss-mlir-export
```

Independent review found and reproduced an exporter boundary mismatch: a numeric
producer and numeric caller were accepted although the Rust harness requires a
published closure. The exporter now explicitly requires the producer to return a
closure; a valid-dialect numeric-producer regression rejects. No further material
exporter or Rust-bridge static defect was found in the bounded supported subset.
The bridge publishes the producer closure in a runtime binding cell; caller
parameter 0 explicitly becomes a live GlobalRead of that cell. This wrapper
convention is an evaluation harness, not source-language global lowering.

Runtime graph export still rejects source-analysis payload attributes. Its v1
policy explicitly omits all MLIR locations; the location-omission probe produces
an unchanged runtime graph. This is not source-location preservation evidence.
The opt-in source-v2 exporter separately carries the complete genuine module
facts; see [source profile](bridge/source-schema.md). C++ v2 checks pass, but its
new Rust carrier/identity checks pass18 units and actual before-main v2 execution;
current-base rebuild/reverification now passes. Numeric/effects
execution alone does not prove executable source preservation.

## Actual isolated Rust and Wasmtime evidence

The coordinator built the standalone locked bridge after the shared build lane
released. The numeric command consumes `bridge/export-fixtures/exported-original.json`
and `exported-mutated.json` from actual registered/verified MLIR exports. Its
retained output `/private/tmp/suss-m4-mlir-numeric-native.log` reports
`abi_rejection_before_initializer=passed`, original bits `4022000000000000` (9),
and mutation bits `402a000000000000` (13). The mutation therefore changed the
actual Wasmtime result, rather than only an expected value or graph model. The
initializer gate includes incompatible ABI/compiler rejection before the marker
and a compatible positive control; it is a separate synthetic probe, not an
exhaustive malformed-artifact gate.

All seven actual `--effects` executions matched independent expected result bits
and ordered cell snapshot bits: success, alternative, handled-throw,
cleanup-overrides-success, cleanup-overrides-handler, captured-f64 and nested-join.
Retained checker output: `/private/tmp/suss-m4-mlir-effects-native.log`.
This exercises reconstructed public IR, `compile_ir`, manifest validation,
Wasmtime GC/EH, live binding cells and GC in the bounded synthetic effects lane.
It is not genuine HIR/source transport or complete language effects support.

Failures remain part of the evidence. The second isolated unit run finished
9 passed / 2 failed, including acceptance of an unknown field on a scalar type.
Tagged serde unit variants ignored extras despite `deny_unknown_fields`; scalar
variants now use empty structs without changing JSON. The third run compiled
and finished **12 passed / 1 failed**, zero ignored/filtered, 0.25s. All effects
and numeric scalar-rejection tests passed. The remaining failure is
`analysis::tests::representative_source_roundtrip_and_mutation`, with
`global/field resolution unsupported`. This historical failure is superseded by
the repaired fourth run: 15 passed, zero failures/ignores/filters. Logs: `/private/tmp/suss-m4-mlir-bridge-second-tests.log` and
`/private/tmp/suss-m4-mlir-bridge-third-tests.log`. This is not an all-tests pass.

## Remaining acceptance

Pinned upstream `mlir-opt` probes in `feature-probes/observations.json` accept
a numeric constant and reject the tested eqref type, struct.new and throw
spellings. These concrete direct-dialect probes and the hashed operation source
identify missing routes for this slice; they do not establish impossibility of
a custom MLIR dialect/lowering or exhaust every extension. Production emission
and the shared runtime ABI remain unchanged.

Source-v2 and same-example emission/GC comparison now execute successfully before
main rebase. Current-base rebuilding and all 92 pairs pass; portable evidence and the
[deferral recommendation](decision.md) now record those results. Still required:
final independent review, full baseline and final-head CI. Complete genuine facts
storage and the source unit now pass. Fresh pinned source observations and existing
Suss CLI both return original9/mutation13; the source graph remains hand-authored,
not an automatic HIR-to-MLIR compiler. Numeric fragments/captured closure/GC,
the bounded initializer gate and seven effects/cleanup cases have actual evidence.
No completed #188 acceptance, adoption or general compiler/scheduler claim is made.

## Historical export-stage measurement

`python3 experiments/mlir/scripts/measure-export.py EXPORTER producer-caller.mlir exported-original.json` completed successfully. Three warmups and twenty timed launches all produced the exact independently reviewed graph. Median 16.612 ms, minimum 12.113 ms, maximum 18.330 ms on this macOS ARM64 host; exporter binary size 3,787,696 bytes. Raw timing samples, platform and exporter/fixture/expected-graph SHA-256 hashes are retained in `/private/tmp/suss-m4-mlir-export-measurement.json`. Timing includes process launch, MLIR parsing/verification and graph export, and ends before this script validates the output. It excludes LLVM/Rust builds, Wasm emission and Wasmtime execution. Shared-host scheduling noise is not isolated. This is a bounded stage observation, not the required end-to-end same-example native compiler comparison; the later current-main paired comparison now passes and is reported in the decision.

Independent ABI-probe static review found no material flaw in the pinned encoder, current prelude type/index assumptions, verify-before-instantiation order, negative counter checks or positive control. The synthetic initializer does not establish rejection of actual exported graphs or malformed prelude/manifest coverage. The current bounded native initializer gate now passes as recorded above; broader
general malformed-artifact coverage is outside this bounded probe; the explicit source-v2 integration now passes.

Exporter runtime linkage inspection: `otool -L /private/tmp/suss-m4-mlir-toolchain/build-export-pinned/suss-mlir-export` lists only macOS system libSystem, libz and libc++. No SDK LLVM/MLIR dynamic library is listed. The measured 3,787,696-byte executable contains the linked LLVM/MLIR code needed for this slice; the 7.6 GiB SDK is a build dependency rather than a demonstrated runtime distribution requirement. This is specific to the pinned macOS ARM64 build; no Linux/other-platform distribution or complete release-package size is established. Upstream LLVM Apache-2.0 with LLVM-exception notices remain required for a distributed linked artifact; no exporter binary is shipped by this experiment.
