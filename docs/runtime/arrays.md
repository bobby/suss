# Mutable array foundation in progress

The portable compiler now provides GC-owned mutable arrays as a prerequisite for
the pinned IndexedSeq, list, variadic rest and collection implementations. Source
arrays have their own private descriptor identity; invocation Args arrays remain
internal buffers. This foundation does not implement persistent collections or
complete M2/M4 acceptance.

The pinned reference is c4295f303100bbf5afac449242d30bca1126f1a1. Runtime declarations
are core.cljs251 array?,452 make-array,468 aclone,477 array,538 aget,545 aset and553
alength. Corresponding macro expansions are core.cljc1043 aget,1056 aset,2635 array,
2644 make-array and2748 alength. These twelve declarations have hash-bound partial
reviews. Original Rust implements storage and bounded macro expansion; no upstream
form is copied. Upstream source/copyright/EPL notices remain in the pinned submodule.
The existing four-form source-import artifact and its licenses are unchanged;
its manifest records the updated review overlay hash.

## Storage and execution

A source array uses the unchanged shared ten-type ABI: a UserObject with a private,
rooted descriptor and a field buffer that owns its current mutable element array.
Growth publishes a copied replacement buffer inside the same owner, preserving
array identity and aliases. Construction copies invocation Args instead of exposing
them to source mutation. Shallow clones have independent owners/buffers and retain
the same element values. Assigned values and nested functions remain GC-reachable
through the owning array; no global object registry retains replaced arrays.
One private descriptor global appends after the earlier globals. No ABI version,
recursive prelude or numeric helper/global indices change.

Numeric indexed reads return internal undefined for missing slots, including
negative/fractional missing keys. Nonnegative integer indexed writes return the
assigned value, grow length when necessary and fill intervening holes with undefined.
Nested indexing, shallow cloning, array?/length, first-class runtime functions and
native protocol array-kind dispatch execute through checked storage intrinsics.
Named/coerced host property keys and negative/fractional property writes remain an
explicit typed unsupported boundary, not a completed JS property contract. String
indexed/length access now executes through these intrinsics using UTF-16 units;
see [indexed strings](indexed-strings.md). String writes remain unsupported.

Array creation/growth is bounded to 1,000,000 elements in this bootstrap; a
multidimensional allocation also bounds its total element cells to that limit.
Dimension argument buffers are bounded to 64 entries. Excesses and invalid storage
raise typed language errors before allocation/cast/access, rather than Wasm traps.
Literal compilation has the corresponding size limit. These are explicit resource
bounds, not comparisons against the primary language's maximum array capacity.
An empty outer dimension does not validate or allocate inaccessible inner leaves.

## Macro and runtime distinctions

The make-array macro ignores its compatibility type operand. It evaluates
more-sizes first, then the outer size, matching its pinned list/let expansion.
First-class function calls evaluate their arguments in ordinary source order.
Numeric literal outer sizes expand with the pin's take convention: positive
fractions round up, negative sizes yield an empty array, and literal single-dimension
storage is nil-filled. Dynamic sizes and first-class calls use checked integer
sizes and undefined holes. undefined? distinguishes nil fill from holes.

Nested aget/aset macro lowering executes intermediate reads before subsequent
index/value expressions; exceptions therefore stop later effects. The final aset
value evaluates before an invalid-target assignment error, as established by fresh
primary execution. First-class calls evaluate all arguments before traversal.
Unqualified user runtime definitions hide the auto-referred array macros; explicit
core qualification remains independent of the user var. Lexical locals, aliases,
refers/exclusions and phase identities retain the existing namespace machinery.
These are bounded bootstrap adaptations, not a claim of compiled upstream macros.

## Evidence and remaining work

The independent scalar corpus tests/oracle/array-cases.json covers 56 cases: mixed
storage/missing slots/identity, mutation/growth/undefined holes, shallow clones,
nested dimensions, first-class functions, native membership/dispatch, ordered
effects, errors, macro/runtime var distinctions and retained function values.
Expected values use exact binary64/Boolean tags, independently decoded from actual
Wasm execution. JVM/Node are development-only primary tools.

```sh
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target sh scripts/test-array-oracle.sh
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test -p suss-cli --test portable_arrays -p suss-compile --test portable_arrays --test runtime_abi --locked -- --test-threads=2
```

The initial 40 fresh primary observations passed while all six original Suss feature
regressions failed with located unresolved array/alength names. Original 40 then
passed against the runtime implementation. All 56 fresh primary observations match actual Wasm execution. Ten native tests
and 18 ABI tests pass; the full locked workspace baseline passes. Additional ABI tests independently verify Args isolation, GC ownership,
growth/clone identity and typed invalid-schema/input/capacity failures. Native
checks retain arrays and old core function values across fragments/rebinding/GC;
compiler checks reject forged arities/types/non-dominating operands. An independent
review regression executes alias-qualified macros, owner-preserving growth and
shallow-clone nested sharing after dropping var roots and forcing GC.

Full upstream dependency/macro/core extraction, compiler checked-array options,
source array literals, general host properties and complete sequence/collection
integration remain unfinished. Review statuses remain in progress, not exclusions
or compatibility completions. Persistent collection acceptance and issues #9/#11/
#17 remain open. Next port source-backed sequence/list foundations and variadic
rest/apply using these validated storage and dispatch boundaries.
