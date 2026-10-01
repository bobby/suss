# Sequence/list foundation preparation

This is a source dependency audit and an original candidate observation corpus,
not a sequence implementation or acceptance claim. The original48 scalar probes
in tests/oracle/sequence-cases.json match fresh pinned primary execution exactly.
Their native regression fails with unresolved runtime seq before implementation.
Preparation does not alter
the compatibility review overlay or import an upstream form.

The source is pinned ClojureScript c4295f303100bbf5afac449242d30bca1126f1a1,
under its retained EPL-1.0 notices. Core.cljs defines ICounted619,
IEmptyableCollection624, ICollection630, IIndexed641, ASeq647, ISeq650,
INext660, IMeta728, IWithMeta733, IEquiv753, ISeqable763 and ISequential768.
Core.cljs1282 seq,1309 first,1320 rest and1333 next dispatch through these
protocols. IndexedSeq1653, prim-seq1751 and array-seq1759 supply indexed storage
views. List3209, EmptyList3286, list3374, Cons3391 and cons3451 provide persistent
list construction and tails. Exact ranges/hashes must come from the inventory
before any source extraction or review status change.

The candidate probes distinguish nil, undefined and the truthy empty list;
first/rest/next; shared list tails; nil/false elements; live array views after
mutation/growth; UTF-16 code units including surrogate pairs/lone surrogates;
first-class calls and construction evaluation order. These are independently
encoded scalar observations, decoded from actual Wasm in the native regression.
They do not substitute collection printing/equality for a lossless heap decoder.

Implementation needs a real source-backed EmptyList/List/Cons/IndexedSeq shape,
canonical empty-list publication, protocol markers/methods and GC-owned storage.
Internal invocation Args arrays must not masquerade as persistent rest sequences.
Array sequence views must retain their owner and observe current storage. String
indexing/length requires its UTF-16 contract; mutable array property helpers alone
do not establish that behavior. Existing source protocols and nominal mutable
fields are prerequisites, not evidence these core protocols/types are loaded.

Several upstream dependencies remain pending: static List.EMPTY properties,
source empty-list literals, full nominal Object/prototype surfaces, collection
metadata/equality/hash, print/iterator/reduction and complete core loading.
Porting only first/rest/next cannot claim complete list or M4 acceptance. Keep
explicit adaptations small, source-hash-bound and licensed. General variadic
source signatures/list*/spread/apply need the resulting persistent sequence.
Lazy/chunked forcing and complete reduction/transducer arities remain separate
acceptance work, with source order and early termination intact.

Fresh graph78997 completed terminal101:48 primary matches followed by the native
unresolved-seq failure, /private/tmp/suss-sequence-primary-and-native-red.log.
Command: `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-sequence-oracle.sh`.
Next choose the smallest source import and prerequisite adaptation from this
verified failure, retaining all48 independently encoded expectations.
No production behavior, issue closure or milestone completion follows from this
preparation. Issue #9 is manually closed in live GitHub tracking, while documented
remaining compiler gaps persist; do not reopen it or infer full acceptance.


## Canonical empty-list literal prerequisite

The pinned compiler emits `cljs.core.List.EMPTY` for an empty list
(`clojurescript/src/main/clojure/cljs/compiler.cljc:577–580`). The portable HIR
now resolves the canonical core List cell and performs the existing checked named
property read. This introduces no runtime type, singleton registry or ABI change.
A lexical or user namespace List cannot redirect the literal. A captured function
reads the live static property each time, matching the upstream emitter rather
than baking in an allocation or cached value.

The original48 certified sequence observations are preserved. Three candidate
observations test lexical shadowing and live static-property replacement/restoration;
fresh certification now passes for all51; native full corpus remains red on unresolved seq. An original nominal adapter test exercises
compiler property lookup and GC only. It does not implement upstream List or
EmptyList. Source sequence/list import, full51 native corpus and complete
collection behavior remain unfinished. The native corpus now explicitly loads the
retained core artifact before executing; it must fail while seq is unresolved.


## Retained-source dependency audit

[sequence-source-audit.json](sequence-source-audit.json) records20 runtime
source declarations from the pinned inventory, with complete ranges and SHA-256
values. It is an audit artifact, not an import recipe or review overlay. No
compatibility status or imported form count changes in this preparation.

The concrete types retain their upstream protocol methods; do not remove methods
merely to make loading pass. List and Cons depend on sequential equivalence,
ordered hashing and seq-reduce as well as printing/index search. EmptyList adds
empty-ordered-hash and a language Error constructor for pop. IndexedSeq adds
array reduction, IndexedSeqIterator, RSeq and unchecked-max. Public seq also
needs the independently recorded iterable/native-protocol branches and a language
error for unsupported values. Count uses accumulating-seq-count/counted?.

A correct import order must account for these mutual references with explicit
source declarations, then publish the actual EmptyList instance on List.EMPTY.
General variadic rest arguments must become persistent sequences before upstream
list's variadic implementation is claimed. Do not use invocation Args as a fake
list or certify a fixed-arity substitute. Complete equality, hashing, metadata,
reduction, iterators and compiled macros remain part of the target.


Focused execution after rebasing onto reviewed PR1003e5af02 passes all6 core
interface tests, including the new canonical literal regression. It also checks
compile-atomic failure before the core List binding is available and live nil
static-property reads. Final session19497 ended0; log
/private/tmp/suss-sequence-canonical-final.log.

Fresh session18929 certifies51 exact pinned observations, including all3 new
static-property probes, then ends101 at the native unresolved Runtime name seq.
Log /private/tmp/suss-sequence-primary51-native-red.log. No expectation changed
or skip added. This is the baseline for actual source sequence/list import,
not a green native sequence result.
