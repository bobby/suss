# Reproducible bootstrap core import

`scripts/core_import.py` selects reviewed declaration IDs from the pinned form
inventory, extracts exact UTF-8 byte ranges, and applies explicit hash-bound
whole-form patches. The recipe is [core-import.json](core-import.json); the first
selection is `runtime:identity:2691`. It retains the upstream body through an
explicit single-arity `defn` to `def`/`fn` bootstrap adaptation. Its original form
and docstring remain in the extracted source. This is source adaptation, not a
claim that the upstream `defn` macro has been compiled or bootstrapped.

The generated [artifact directory](../../runtime/core-import/) contains original
form copies with upstream notices, the adapted `suss/core.sus` source, byte-preserved
upstream `LICENSE` and `epl-v10.html`, and a deterministic `manifest.json`. Ship
this directory together; generated ClojureScript-derived source remains under
EPL-1.0, separately from the repository's Rust license.

The manifest records the exact source commit/path/range/context, source-file and
form hashes, extracted and adapted hashes, patch path/hash, recipe/inventory/review
hashes, extraction/scanner/validator tool hashes and every generated file's hash.
No timestamp or local absolute path enters the output. Reader branches and enclosing executable containers are retained
as inventory context. Selected declarations must have empty context: the importer
rejects reader branches and lexical/dynamic containers until context-preserving
import is implemented; the extractor never removes forms
because they contain JS interop. The recipe order is explicit dependency order.

```sh
python3 scripts/core_import.py
python3 scripts/core_import.py --check
scripts/verify-core-import.sh
scripts/test-core-import-oracle.sh
```

The verifier recomputes artifacts from a clean pinned source and the validated
review overlay. It rejects stale inventory/reviews/source hashes, unreviewed or
excluded selections, duplicate IDs/JSON keys, phase mismatch, unsupported declaration context, missing notices,
changed upstream license bytes, extra generated files and modified/missing outputs.
Patches must match the reviewed adaptation path, name the exact original hash,
state a rationale and replace exactly one declaration with the same name. This
checks provenance and shape; it does not prove semantic equivalence.

Native `core_import` tests load the generated source into the canonical `suss.core`
namespace and execute independently decoded results. They cover aliases, old
captured function values, live redefinition, exact identity of scalars/functions/
nominal objects through GC, ordered once-only arguments and wrong-arity recovery.
The development-only oracle executes the pinned upstream `identity` definition,
using the same 14-case scalar corpus as native execution and strict lossless
transport for binary64, nil/booleans and UTF-16. JVM/Node are needed only for this
reference runner, not extraction, verification or Suss execution.

This is prerequisite evidence for issue #16, not completed M4-01 acceptance. Only
one runtime form is selected; general core dependency resolution, complete form
review, upstream macro compilation, namespace privacy/doc metadata, phase bootstrap,
collection foundations and production automatic core loading remain unfinished.
The default CLI/REPL still uses prototype paths. The manifest is byte provenance;
executing evidence remains separate. Future source/tool/review changes require
regeneration and renewed semantic review.
