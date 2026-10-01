# Retained sequence and list foundation

This is partial source-backed collection/core progress, not complete M4 or public
collection acceptance. All methods of pinned List, EmptyList, Cons and IndexedSeq
are retained. Actual descriptor-backed GC objects implement shared persistent
tails, canonical empty lists, array views and UTF-16 sequences. Pending method
helpers remain explicitly declared and uninitialized; loading a type does not
certify printing, sequential equivalence/hash, reduction, reversal or iteration.

The reference is ClojureScript c4295f303100bbf5afac449242d30bca1126f1a1.
The [import recipe](../compatibility/core-import.json) selects68 forms and generates
72 licensed/hash-bound artifacts. Eighteen additional partial reviews bring the
overlay to157 in progress/908 unassessed. Original upstream forms, complete method
bodies, metadata/docstrings and EPL notices/licenses are retained in extraction.
Explicit patches replace bounded defn bootstrap, host Error allocation and the
fallback's discarded Array.push call with numeric GC array append. The executable
empty-ordered-hash bootstrap omits its unsupported private attribute; original
metadata remains retained and privacy/runtime Var metadata are still unfinished.

## Source loading and storage

[The20-declaration audit](sequence-source-audit.json) records complete inventory
ranges and hashes without changing compatibility statuses. Original loader
forward declarations break mutual references and explicitly name pending helpers.
The two standalone upstream setup forms publish a real EmptyList on List.EMPTY
and extend nil's ICounted implementation. Their complete source bounds/hashes and
EPL notice are checked by scripts/sequence_provenance.py and CI. Loader input hashes
and order are included in the generated manifest. These are setup statements,
separate from the68 selected declaration forms.

The shared ten-type GC layout and runtime ABI version do not change. Source types
use the existing nominal descriptor/field arrays, with original mutable hash slots.
IndexedSeq retains its array/string owner and offset; mutation/growth and UTF-16
code units remain observable after GC. No process registry or persistent-map
substitute owns sequences.

## Empty literals, rest values and errors

Pinned compiler.cljc577–580 emits cljs.core.List.EMPTY for (). Portable HIR reads
canonical suss.core/List and its checked EMPTY property on each evaluation. Lexical
or user List bindings cannot redirect it; captured functions observe live property
changes. Six focused core interface tests include alias/GC/redefinition and
compile-atomic failure before List is loaded. The original adapter test verifies
compiler lookup only; actual collection evidence uses the retained types.

Pinned compiler.cljc987–1001/1058–1074 constructs nonempty variadic rest with
new cljs.core.IndexedSeq over copied arguments, offset0 and nil metadata. Compiler
HIR/verified IR distinguish fixed and variadic methods; dispatch selects exact
fixed arities before a variadic entry. Nonempty rest wraps a fresh owned source
array in the live IndexedSeq class. Empty rest is nil and does not read/construct
the class. Internal invocation Args never serves as a language persistent sequence.
Named self calls and recur retain the ordinary loop/capture contracts. General
apply/applyTo, destructuring and full compiled macros remain unfinished.

The upstream list algorithm executes over those real rest values and constructs
actual List nodes through -conj. Both source branches are retained; the numeric
append patch does not certify the fallback through general apply, which is still
pending. Rebinding array-seq does not affect the pinned variadic wrapper; rebinding
IndexedSeq does. Fresh probes distinguish those boundaries.

A private suss.bootstrap/error adapter creates the existing descriptor-backed
language Error payload with a checked scalar UTF-16 message. Retained empty-pop
and out-of-range indexed access throw language exceptions, preserve messages and
recover after GC. This is not full public JS Error constructor/class/stack/printing
interoperability. The source seq adapter retains direct protocol, native array,
UTF-16 and native-protocol paths; host JS Symbol.iterator is outside this adapter.
The pending str_ dependency means unsupported-value formatting is not yet certified.

## Executing evidence

All original51 certified source/expectation values remain unchanged. Eighteen
added observations bring the fresh corpus to69, with exact primary/native agreement
(session9486; /private/tmp/suss-sequence-primary69-class-final.log). Four native
sequence tests pass (98359; /private/tmp/suss-sequence-source-final.log), including
GC, shared tails, metadata, captured functions, fixed/variadic signatures, named
self/recur, live class lookup, ordered arguments, compile-atomic malformed signatures,
runtime arity effects and strict decoded Error messages. These scalar observations
are independently decoded from actual validated Wasm; they do not replace a full
lossless collection decoder or certify equality/printing.

Python80/import72/reviews157+908 and both licensed setup forms pass. Final fresh72919 again certifies69 primary/native matches with all4 native tests.
Closure41525 passes14 tests including malformed variadic IR rejection. Final full
workspace6028 ended0: all required workspace suites/doc tests pass. Independent
PR review/fixes/exact reviewed-head CI remain required before readiness.

Initial loading failed on unsupported private metadata; the explicit source-bound
patch records that limitation. A fixture used nonexistent StructRef.get and was
corrected to strict field decoding. The initial67-case oracle failed because a
new candidate wrongly expected live array-seq to supply rest. The pin actually
constructs IndexedSeq directly; only that new expected value was corrected, its
probe renamed, and two class/empty-rest probes added before fresh69 certification.
The untouched original51 are not retargeted or skipped. First full72012 ended101
on a former unsupported-variadic test expecting the ampersand span. Its standalone
fixture still lacks IndexedSeq; the regression now explicitly requires that located
missing-class diagnostic. All14 closure tests pass. The pinned unchecked-max
expansion binds each operand once; the IndexedSeq count patch now retains that
binding/branch behavior before final fresh69 and final baseline. Setup provenance initially
listed source-order statements while the loader used explicit initializer order;
its metadata was corrected and all80 Python tests now pass.

Next complete pending source method dependencies: ordered hashing/equality,
reduction/reduced/early termination, printing/index search, iterators/reversal,
source general apply and metadata/private/bootstrap loading. Lazy/chunked effects,
vector/HAMT/sorted collections and transients remain acceptance work. No issue or
milestone closure follows from this foundation.
