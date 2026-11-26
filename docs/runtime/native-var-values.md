# Native Var and namespace values

A native Var refers to its owning Session's actual phase-qualified binding cell.
A compiler-owned hidden cell interns the Var object. Namespace catalogs validate
the actual registered Namespace, enumerate its declared/bound public cells, and
reuse those Vars. Definitions intern their Var during native eval analysis;
new declaration metadata is published after successful initialization. A failed
initializer leaves already-published runtime effects intact and does not publish
its proposed root or metadata. Untaken and uncalled definitions retain the analysis-time Var.

ABI 13 adds a third GC root to the private binding cell: the genuine nominal
`VarUnbound` object, whose public final `v` field is the owning Var. The first two
fields remain the root value and flags. Dereference and ordinary source reads of
an unbound Var return the same object. It implements IFn, has identity equality
and hashing, and prints `Unbound: #'namespace/name`; invoking it throws the
platform language error with Clojure's unbound-function message. It has no Fn
marker. Temporary root replacement/restoration retains this Unbound identity.
Compiler-only cells retain their undefined marker, and dynamic snapshots keep
boundness separate from the value.

`alter-var-root` applies the function to the physical root and its arguments,
then writes only after successful return. Dynamic entries remain separate. As
in Clojure's `Var.alterRoot`, this operation preserves the macro flag; an ordinary
root bind clears it. An actual Unbound result remains unbound. Validator/watch
APIs are outside this adapter.

Cross-Store transport retains actual source roots through authenticated GC
capabilities. Vars, Namespaces, Unbound values and state arrays forward their
operations to their owning Store. Functions preserve captures and actual
metadata roots; immutable metadata graphs preserve object sharing. Returning a
value to its source unwraps the original object. The ordinary display/syntax
readers keep their admission rules. Direct eval of Unbound returns itself;
embedding it in compiled collection data is rejected, matching the primary.

The first core catalog uses a bounded canonical-keyword root cache and direct
canonical data factories. UTF-16 hashing uses an i32 Murmur fast path guarded by
the captured seed and mixing-helper identities; replaced helpers execute the
original source algorithm. Ordinary operation allowances remain unchanged.
Pure components receive no hidden namespace/compiler service and report an
explicit missing-capability error.

Source-only macro exports are materialized in the caller's canonical catalog
without a prior Var reference. Staged declarations admit their actual Var but
publish the isolated transformer only after source linking succeeds. Namespace
imports execute in the isolated Macro host; each completed macro definition
publishes its real root and metadata, and explicit reload preserves the caller's
Var identity and any previously captured function. Completed dependencies
advance every active source-declaration journal, so rejecting later caller
source cannot restore an old root or metadata over that import. Unlinked
compiler metadata writes are restored only while they remain current; later
metadata effects remain intact. Public namespace load/reload uses the same
source preparation and linking boundaries as compiled source input.

Live Cons/LazySeq values retain the actual source fields and realization cache.
Ordinary data reads force them through the captured Object method dispatcher,
including its receiver and exception restoration. Function namespace context
retains the actual Namespace object rather than a name-based replacement.
Canonical class/EMPTY/method roots use private GC handles. New reader bridges
capture current hash/equality callbacks to preserve reader occurrence effects;
no helper lookup fragments are installed. See the source-import checkpoint
for exact positive and negative receipts and the remaining validation gates.
