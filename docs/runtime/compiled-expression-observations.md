# Independent compiled expression observations

The replacement expression acceptance harness now executes in
`crates/suss-compile/tests/portable_expression_conformance.rs`. It retains all
201 expressions and expected values in the reviewed compatibility catalog.
The original prototype harness, corpus, catalog and empty failure baseline are
unchanged. The first aligned-head run compiled and executed all201 cases:182 passed and19 failed, with zero skips. Every failure was an unresolved Runtime core name: abs, max, min, mod, even?, odd?, peek or pop. Inputs and expectations remain unchanged. Those failures were retained and subsequently repaired by the reviewed mod prerequisite and nine retained core definitions, as recorded below.

Each case uses fresh compiled Runtime and Macro sessions. The input executes
through `Session::eval_with_macros`; its rooted result survives a forced GC
before host observation. The observer neither invokes guest equality nor
prints and rereads the result. Compilation, explicit generated-fragment
validation, dependency, language exception, trap, host, ownership, GC, decode
and value failures remain visible. Uncaught language exceptions retain their
actual rooted payload when its layout can be decoded; an unsupported payload
remains an explicit decode diagnostic.

`tests/support/portable_decode.rs` reads ABI2 storage independently of the
production form bridge and prototype class indices. It captures descriptors
from known canonical instances before user code, retains those roots and checks
session ownership. It observes binary64 bits, UTF-16 units, nil/booleans,
keywords/symbols, vectors, List/EmptyList/Cons, IndexedSeq, MapEntry, array/hash
maps and hash sets. Vector tries and HAMT nodes are traversed directly. Unknown
nominal objects, malformed fields/counts/flags, foreign or reset roots and
exhausted bounds fail observation. ChunkedSeq reads canonical vector/chunk storage, bounded indices and offsets,
including Cons tails across chunks. Other sequence representations and generic
objects remain unsupported; no opaque value matches an expectation.

Sixteen decoder regressions passed on the earlier source head330e7507, with
zero failures, ignores or filters. The first collection run recorded eleven
passes and five unsupported-layout failures. After implementing storage reads,
three large fixture builders exhausted the interactive10M fuel budget before
decoding; these fixtures now use explicit bounded1B fuel without changing their
inputs or assertions. The final same16 run passed in2.72seconds. The handoff
records those failures and logs. These results do not establish all201 corpus
acceptance or current-head acceptance after alignment tob48e8bf.

`tests/support/portable_compare.rs` compares host observations with the reviewed
expected values. It rejects integer rounding beyond2^53, distinguishes signed
zero, preserves UTF-16, compares canonical NaN representations explicitly and
consumes unordered map/set matches once. Sequential values compare in order
across vector/list observations. Unsupported expected types do not match.
The comparison, failure-stage and actual exception/source-failure checks all passed in the first compiled run. The entire harness result was3pass/1fail because the all201 acceptance check correctly rejected19 unresolved names.

For an actual corpus run, optional `SUSS_COMPILED_CORPUS_EVIDENCE` names a
scratch JSON output path. The harness records every input/expectation and its
actual observation or failure stage/detail before the allpass assertion. It
never reads that file as a failure baseline. A failing case cannot become a
passing acceptance result by recording it.

```sh
SUSS_COMPILED_CORPUS_EVIDENCE=/private/tmp/suss-compiled-corpus-observed.json \
  cargo test -p suss-compile --locked \
  --test portable_value_decoder --test portable_expression_conformance \
  -- --test-threads=2 --nocapture
```

The initial combined command stopped after the failing expression harness, so the decoder ran separately. All original16 passed; a new large-vector rest/Cons-tail check failed with unsupported layout before the ChunkedSeq repair. The same17 then passed unchanged in2.66seconds with zero failures, ignores or filters. Scratch corpus observations are retained separately from the unchanged reviewed catalog. Public
expression/cache APIs and the component-target CLI still use prototype paths.
Production evaluator retirement, source schema/cache/lifecycle acceptance and
original M3 issues remain incomplete. Independent review, the unfiltered full
workspace baseline and final-head CI are required before PR readiness.

After alignment to reviewed/rebased PR #204 head b210bbb and generation of
both bootstrap phase pairs, the unchanged compiled corpus executed **201 passing,
0 failing, 0 skipped**. All four expression-harness tests and all eighteen
independent decoder regressions passed with no ignores or filters. The eighteenth
check rejects malformed ChunkedSeq storage and verifies that an observed suffix
does not decode opaque values in an unused prefix. Actual lossless observations
are retained in the scratch evidence file, not substituted into the catalog.

The numeric/stack edge test passed in both phases, and **39 freshly executed
pinned ClojureScript observations matched exactly**. The separate parity-error
message regression failed with an actual language exception; valid corpus and
edge cases do not certify error formatting. No assertion was relaxed.

A tenth retained dependency, private runtime `str_`, now executes the seventeen
runtime-helper expectations in both phases, including binary64 formatting
boundaries, negative zero, nil-first behavior and nominal keyword conversion.
Both phase images regenerated after a preserved initial unresolved-`boolean?`
generation failure; the adapter now captures existing scalar predicates instead.
Strict extraction verifies 288 files, all fourteen importer checks pass and all
178 Python checks pass. The separate compiler `str_` macro and complete printing/
host coercion remain unfinished.

The unchanged parity assertion still failed after that repair. A focused probe
then confirmed the original parity error's **actual message** through
`ex-message`, followed by a failure reading `.message` on a known typed error.
The remaining bug is the absent named-property read for the ABI Error layout,
not scalar formatting. The descriptor-checked immutable message-property read now executes successfully:
actual typed Error messages survive GC in both phases, including a lone surrogate
held across fragments. A raw ABI regression copies every descriptor field and
proves that a copied descriptor is rejected with a language exception, not a trap;
the genuine descriptor remains usable after GC. All 49 runtime ABI tests pass.

A further negative regression reproduced a cast trap in the private string
concatenation intrinsic. It now checks both string operands before casting and
raises a typed language error. The unchanged regression passes for a bad right
operand, a bad left operand and two nil operands in both phases, with both effects
observed exactly once in source order. All five core-dependency regressions pass,
including the original parity message assertion and the 17 internal-string cases.

After both images regenerated, all four expression-harness tests and all 18
decoder regressions pass again; the unchanged 201-case all-pass assertion retains
every original input and expectation. All 178 Python checks pass. A fresh build
and execution of the pinned ClojureScript/Node oracle now matches **all 56
observations exactly**. With Java and Node absent, two fresh builds reproduce
BOTH phase Wasm/JSON pairs byte-exact; compiler identity/invalidation checks and
all four executing bootstrap tests pass. Earlier failing logs remain retained;
no assertion was weakened or failure accepted as a baseline.

Public expression/cache APIs and evaluator retirement are separate remaining
M3 requirements. This executing Session corpus does not establish migration of
those public APIs or completion of source schemas and asynchronous lifecycle
acceptance. Independent review, the exact full workspace baseline and final-head
CI remain required before a new PR is ready.


Independent PR #207 review reproduced five acceptance defects in the observer:
a vector count exceeding its trie capacity, a mutable unary f64 structure,
an immutable I16 string array, immutable I16 IndexedSeq backing, and a foreign
owner structure carrying a genuine nominal descriptor. The original eighteen
decoder regressions passed while all four initial new negatives failed; the
IndexedSeq negative failed separately. These were acceptance failures, not
fixture construction errors. No expected value or original assertion changed.

The observer now checks boxed-number immutability and layout, language string
storage mutability, exact canonical owner Wasm type alongside descriptor identity,
and vector count against trie capacity before traversal. Equivalent canonical
Wasm types remain valid. The same new negatives pass alongside all existing
checks. The affected run passes 85 tests across four groups: seven core-dependency,
six expression-harness, twenty-three decoder and forty-nine runtime-ABI tests,
with no failures, ignores or filters. The two internal raw owner/storage checks
execute in each integration binary importing the observer. Actual scratch JSON
contains all 201 passing observations with every original catalog input and
expectation unchanged. Final-head full CI remains required before readiness;
public compiler migration and original M3 requirements remain unfinished.


The first full CI run after those repairs failed the unchanged complete-core
namespace acceptance test: the compact compiler graph reached its validation
work bound before guest construction. Its 61,891 recipes had 514,310 dependency
edges. Validation charged each queued edge and then charged an additional visit
even when its shared target was already fully validated. The repair skips that
redundant validation charge while retaining edge charges and unique-node entry/
finish charges. Both fixed work limits, all records and the separate cold reader
occurrence accounting remain unchanged.

A new tight-budget shared-DAG test first failed with the original algorithm and
now passes. All seven sharing tests pass, including unchanged reader callback
counts/order, cold retained-metadata budgets, pre-callback failures and recovery.
The original complete-core namespace test passes unchanged in both caller phases
in 71.44 seconds. Both phase pairs regenerate and reproduce twice byte-exact
without Java/Node; all four executing bootstrap checks pass. The failed full CI
log remains evidence of a failure, not an accepted baseline. A passing exact
final-head full run and CI are still required before readiness.
