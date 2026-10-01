# Bootstrap control macros for retained core source

The bounded compiler bootstrap now expands when, when-not, if-not, and, or and
cond into existing checked HIR if/do/let nodes. The pinned sequence functions
and collection methods require these forms. This is a prerequisite for importing
that source, not compiled macro runtime or persistent collection acceptance.

Tests evaluate once in source order. When bodies form an implicit do; unselected
bodies produce nil. And/or bind preceding operands to fresh binding IDs, preserve
the actual short circuit value and avoid capturing source locals. Final branches
inherit their caller's tail context; tests and preceding operands do not. Cond
requires pairs, tries them in order and yields nil when no pair matches. The
empty and/or forms yield true/nil respectively. Literal keyword tests are folded
to truthy Boolean conditions, including :else; materialized keyword values remain
unimplemented. Runtime namespace definitions and lexical functions can hide
automatic macros; explicit core qualification remains available. Core aliases,
exclusions and separate macro/runtime phases use the existing resolver.

Source provenance is ClojureScript c4295f303100bbf5afac449242d30bca1126f1a1,
core.cljc cond159, if-not359, when392, when-not427, and901 and or912. Six hash-bound
partial reviews retain the recorded :cljs reader contexts where present. This
original Rust lowering copies no upstream forms; original source/notices remain
in the pinned submodule. Licensed generated core source retains its25 files with
an updated review overlay hash. The bootstrap has a256-operand expansion limit
with located diagnostics; this is not the reference language's arity limit.

A separate59-case corpus is freshly compiled/executed by the pinned development
runner, then independently decoded from actual native Wasm. Exact binary64,
Boolean/nil sentinels and UTF-16 results are checked; unknown values fail. GC runs
between cases. Effects, short circuiting, exception/finally cleanup, nominal
identity, lexical/runtime shadowing, core qualification and tail recur execute.
Located malformed arity/odd-pair/non-tail recur/resource errors preserve session
compile atomicity. Additional native tests check aliases, exclusions and both
phase compilation paths. JVM/Node are development tools only.

```sh
CARGO_BUILD_JOBS=2 sh scripts/test-control-flow-oracle.sh
```

Complete compiled macro execution with &form/&env, source metadata, syntax quote,
gensyms, binding/destructuring macros and full portable keyword/data values remain
unfinished. A user runtime not definition does not alter this qualified primitive
if-not lowering, as independently observed. No new runtime ABI layout, helper
index, global or dependency is introduced. Next retain the real sequence/list
source, including canonical empty-list literals and its hashing/reduction/rest
dependencies. No issue or milestone closure follows from these partial reviews.

Independent PR review added five exact primary/native probes: a callable nominal
field hides an automatic macro; closures retain source captures through nested
control temporaries; arbitrary keyword conditions are truthy; selected if-not
branches preserve loop tail recurrence; thrown cond tests preserve catch/finally
source order. The original54 observations remain unchanged.

Bounded symbol-binding if-let now preserves a fresh test temporary, binding scope
only in the consequent, outer initializer/else scope, reader metadata and tail
context. Eighteen added observations bring control77, original59 unchanged. Native
alias/exclusion/both-phase and malformed located compile-atomic guards pass. Full
destructuring and compiled upstream macros remain unfinished. See
[reduction/if-let evidence](sequence-reduction.md).

Independent PR103 review preserves all77 certified observations and adds four
fresh pinned/native observations for nested initializer scope, consequent and
else captures, and effect order:81 total. A compiler HIR regression verifies
reader metadata, exact source spans and distinct binding identities. These checks
certify the bounded symbol-binding adapter; compiled upstream macros and
destructuring remain unfinished.
