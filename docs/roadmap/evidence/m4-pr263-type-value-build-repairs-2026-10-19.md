# Type-value build diagnostics, bounded repairs and marker erratum

[Exact source patches, reviews, raw diagnostics and corrected marker data](m4-pr263-type-value-build-repairs-2026-10-19.tar.gz),
SHA256 `846b8c73c1ecdc6140ec36deac499ea1f16a0e7b15c00e3cba27e2da1077e728`. Internal payload inventory is SHA256SUMS.json.

## Actual build failure

Source `edc2e5357d1ece51e070308424948093db19558e`, CI39295062658,
foundation118205889534: suss-compile library compilation fails before normal
bootstrap generation and before tests. E0425 at type_key_history.rs98: missing
current holder variable; E0277 at completed_type_values.rs350: dyn FnMut passed
to a Sized generic. No new bootstrap or test outcome is inferred. Earlier
source review did not establish Rust type correctness; preserve this raw failure.

The retained-key helper now receives the actual current Info shared reference
from its sole caller, whose history it already traverses. No cloned/fabricated
holder or weakened owner check is introduced. A sized forwarding closure
adapts the existing dynamic debit callback, forwarding each charge and error
exactly once. Rates, spans, validation order and request scopes are unchanged.
Independent review approves patch54799d4d; syntax/apply checks alone do not prove
compilation.

## Avoiding actual unused work

Removing an absent name from a shared lexical map formerly copied the complete
map before discovering nothing could be removed. The reviewed COW change
prepays a native membership probe and retains the original map when absent.
Unique maps keep the original direct removal work; present shared entries pay
the extra probe plus complete original copy/removal fees. Weak/saved owners
retain their exact entries. No semantic source query or authority guard is
skipped; no rate, default, Runtime allowance or strict control is changed.

Two authored tests compare actual old/new helper work and entry/owner identity
for shared/weak/unique cases, plus every reached small debit failure and retry.
They are unexecuted. Present-key removal can cost more; core_import17 benefit
and dominance are unmeasured. The earlier source-info query-elision candidate
was rejected because it could suppress source errors; it is not applied.
Independent review approves only patch11d208f7 and the exact current postimage.

## Marker extraction erratum

The [earlier immutable archive](m4-pr263-completed-type-values-2026-10-19.md)
contains a mislabeled derived native320/namespace_identity.json that duplicates
the callable derivative. The substring NAMESPACE_IDENTITY also matched
CALLABLE_NAMESPACE_IDENTITY. Carver's review caught the error. The raw log
and published native classifications/counts were correct and remain unchanged.

Corrected extraction matches the exact standalone marker after the timestamp,
asserts unique labels and distinct result shapes, and preserves the original
mislabeled file. Namespace observations contain two compilation errors and
unexecuted semantic rows; callable observations contain six decoded requests,
matching first calls and mismatching same-revision calls2/3 in both phases.
Corrected namespace JSON SHA256 is
`af329fb38d9c078c874a78677eed224ba77d0c3777a13dd84d083c73696cd873`;
callable JSON SHA256 is
`cc17f344248702ad3f6758bc8ddc549b2299e8aba3795cf30d78442cfe39ad28`.
The original raw log SHA256 is
`67fe529a6c5a2445c770506f4cd7f4dae765511d71267e3db8ac0148973902e5`.
Corrected data, original derivative and receipt are included here.

## Required verification

No local Cargo/Suss VM ran while the parent66a55ce exact workspace producer
remains live. Shipped BOOT files still belong to320; regenerate both normal
phases twice from these sources, bind and independently review the complete
tuples before adoption. Then run Java/Node-free bootstrap verification/execution,
focused type_value_regression_tests/type_key_history/analyzer_literal_/
absent_remove_tests/ownership_tests and unchanged source_roots/core_import/
compiled_macro_source_tags, followed by the exact required workspace baseline.
The full strict source-child five-variant operation and core17 defaults remain
unchanged; real public alias, cost and full-suite results remain unproved.
