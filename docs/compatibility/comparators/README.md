# Complete comparator source draft — Refs #18

This isolated draft is based on `95fe332` and retains the complete pinned
`compare:2499`, private `compare-indexed:2525` and private `fn->comparator:2541`
forms. It preserves their exact documentation/metadata and arities 2, 2/4 and 1
(returned closure 2). Original extraction and generated core retain upstream
Rich Hickey/EPL notices and the packaged upstream license. All three new reviews
are `:in-progress`; source authoring is not executed compatibility acceptance.

The hash-bound patches expand defn explicitly. `compare` retains identity,
ordered nil tests, numeric-before-protocol dispatch, unchanged `-compare` return,
and the live `array?`/`type` fallback plus both error branches. An original
private helper re-expresses Closure defaultCompare as binary `>` then `<`, with
fresh left-to-right conversions for each relation. It has no scalar admission
filter and does not subtract operands. Primitive macro tests use immutable
bootstrap/qualified lowering; callable `array?`, `type`, `-compare`, `=`,
`compare`, `str_`, `count` and `nth` remain live. `compare-indexed` keeps count
before contents, length-first ordering, both arities and four-argument recur.
`fn->comparator` preserves live equality/compare reads, unchanged first numeric
return and reverse callback truthiness without a second numeric test.

Error formatting expands the pinned str_ macro, rather than calling variadic
runtime str_: read live single-argument str_ for x and then y, then join the four
original pieces with an empty separator. Joining matters when a rebound str_
returns nil, Number or Object: the two calls finish before those results undergo
join conversion. Literal array construction uses the qualified compiler path.
This is an original adaptation of the algorithm; no Closure source was copied.
Closure dependency provenance remains in the earlier comparator plan and the
historical reference fixtures, not an EPL claim for Closure code.

Two prerequisites are repaired in this draft. Retained runtime str_ uses checked
string-hint ToPrimitive for its single-argument branch, matching singleton Array
join conversion instead of directly calling toString. Both nil-first branches
use immutable nil macro semantics; unused predicate captures are removed while
seq/first/next captures remain. Source Array type lookup observes any own
constructor value (including nil/false) before rooted canonical fallback.
General Function/native-object constructors and nominal constructor overrides
are still pending. None of these changes modifies the frozen Array worktree.

## Reference and execution boundaries

The strict shared candidate corpus and original generated driver are
`tests/oracle/comparator-cases.json` and
`tests/oracle/src/suss_oracle/comparators.cljs`. The two native arity-policy
probes are separate in `comparator-policy-cases.json`; they are not disguised as
pinned matches. The Python harness rejects missing/reordered/duplicate/unknown
IDs, numeric Boolean values, extra keys and malformed source forms. It has no
import-time execution or writes. The runner checks the pin and driver before
compilation. Private-function warnings are expected and retained.

The old 64-case fixture, observations and actual logs are copied unchanged into
`evidence/historical-64/`, with hash/byte receipts. They are historical pinned
reference only, not execution of this source draft. New authoring failures are
retained separately and are not passes. Current gate results are recorded in the
handoff and evidence README after terminal status.

`portable_comparators.rs` is authored, UNCOMPILED/UNEXECUTED. It evaluates the
complete candidate corpus and separate policy cases in both caller phases after
GC, independently decodes signed-zero/infinity/NaN returns and actual Error
layout/descriptor/message, tests recovery and a captured vector across fragments
and GC. The capture remains source-rooted; this is not a sole-host-handle claim.
No bootstrap regeneration, Cargo build, native run, full baseline or CI has been
performed for this draft. Base bootstrap files are intentionally stale.

## Genuine remaining requirements

* Built-in IComparable coverage needs the entire pinned extension at
  core.cljs10813–10853: Symbol/Keyword/Subvec/PersistentVector/MapEntry/BlackNode/
  RedNode. Existing nominal protocol fixtures do not establish those methods.
  Preserve that whole source stanza and its dependencies; BlackNode/RedNode and
  full trees are not implemented here. Direct compare-indexed tests are not
  evidence that public compare dispatches on vectors or nested vectors.
* Function/Error/default nominal ToPrimitive, complete constructor reads,
  inherited/prototype/accessor properties and exact TypeError are missing.
  These matter to comparator error formatting and to mixed operands admitted by
  a live type replacement. Same-reference identity and custom IComparable
  branches bypass conversion/type lookup and do not need all prototype kernels.
* Array comparison specifically requires live own valueOf/toString/join and
  constructor behavior, repeated conversion, nested/cyclic joins, sparse holes,
  mutation and inherited indexed reads. Own/default paths exist; inherited
  indexed reads and accessors remain absent. This is not full Array compatibility.
* Exotic @@toPrimitive and coercion failures must retain original throws and
  exact errors. Neither a generic nominal rejection nor a skipped test proves
  those semantics. Callback physical this, live type/array?/str_ mutation during
  conversion, count/nth replacement order and full reload need additional proof.
* The retained scalar-only prerequisite helper is not wired into compare. It
  cannot be used as a replacement for the imported complete algorithm.

Next unblocked validation: independent source review, scheduled bootstrap
regeneration, focused complete comparator/source/transport tests, fresh pinned
comparison, unchanged required baseline and final-head CI. Implement the
remaining protocol/conversion/property contracts before claiming full #18 or M4;
this draft cannot close either.
