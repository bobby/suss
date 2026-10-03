# Actual analyzed source records

`Hir::source` retains an immutable `SourceAnalysis` for syntax actually analyzed
through the source pipeline: the reader or expansion form, three-way analysis
context, phase, namespace at entry and optional supplied source origin. This is
original compiler code; it imports no upstream implementation and adds no shipped
Java or JavaScript dependency.

Compiler-only lowering nodes explicitly have no source record. A function's
physical recurrence loop, receiver access, generated protocol helpers and private
runtime operations must not be presented as user syntax. A macro expansion keeps
the expanded form's record rather than overwriting it with the original call.
Expansion spans remain call-site provenance; they do not certify that the generated
syntax occurs in the original source text. Owned forms without supplied source
retain unknown origin. No source is evaluated to construct these records.

Executing binding regressions inspect actual `do` and arithmetic initializer
syntax and child records while independently checking that initializer effects
run once in Runtime and Macro Stores. Another regression executes an expanded
initializer, collects GC and decodes its number, then checks that the source record
contains the actual numeric expansion rather than the original macro call.

Validation: 9 passed, zero failures/ignores across binding, context and function
scope regressions; 38 passed, zero failures/ignores across compiler pipeline,
closures, nominal and exception tests. Full validation of this new branch remains
pending. These records are preparation for rich compiled `&env`; they do not yet
provide a complete source AST schema, inference, canonical environment transport
or source macro `&env`. All original M3 acceptance remains open.

Independent PR review found that captured factory closures still looked up live
constructor globals. Metadata-bearing symbols and collections could consequently
become a different nominal class after redefinition. Native construction now uses
the captured canonical constructors for metadata, vector tries and map nodes.
Identifier key equality ignores metadata and live class bindings. HAMT hash
conversion truncates modulo 2^32 rather than saturating a Rust integer cast; small
array-map construction does not invoke key hash protocols. Map/vector entry counts
are bounded to 65,536, collision comparisons to 1,048,576, and hash paths to 32 bits.

Data classification now verifies the captured descriptor by reference identity
after its ID lookup. A copied descriptor carrying the same ID is rejected for
sets, KeySeq and all previously supported nominal data classes after GC. Private
catch names also avoid fields and user catch aliases, preserving the distinct
payload and user binding records. Review regressions failed before these repairs;
the handoff records their separate results and final validation. These are native
transport repairs, not complete portable environment or bootstrap acceptance.

Source records now also retain immutable pre-analysis namespace catalogs, lexical
locals, fields, function scopes, actual name hints and resolved declaration facts.
A local namespace snapshot cache uses the environment's mutation generation only
within the current analyzer; it is not a cross-compilation artifact cache key.
Bindings and function scopes retain their actual declaration context. Source
function names record real variadic signatures before body analysis. A method
receiver retains its declaration namespace. Physical binding IDs stay unchanged.

Catch analysis exposes the real private payload parameter and the user catch alias
as separate source roles. The user alias remains physically the same catch ID,
but its logical role is `let`. Compiler-owned names avoid collisions with existing
source locals. An executing regression checks that the payload access uses that
real ID, catches restore scope correctly, and an initializer's retained namespace
and resolved declaration still describe the original value after redefinition.

Native form transport now allocates scalars and source arrays through the shared
runtime and constructs identifiers/lists through the compiler's actual `new`
path. Native bulk builders use captured canonical vector/map/node constructors,
including equal duplicate keys with the first key and last value retained. HAMT
keys use captured core hash/key comparison helpers; small array maps compare keys
without calling their hash protocols. `quote` constructs reader data
directly instead of compiling a fragment for each transported form. It uses the
compiler's existing bounded metadata normalization and identifier hash; it does
not evaluate metadata expressions or resolve source identifiers. All values are
rooted in their owning Store, including shared values, and foreign/reset handles
are rejected.

Four executing builder regressions check exact numeric bits and UTF-16 units,
shared identity after GC, duplicate keys, vector trie boundaries, Store/reset
checks, traversal limits and unchanged resident fragment/byte counts and external
handle counts. Transport is bounded to 4,096 form nodes, 64 levels and 1,048,576
UTF-16 units. Persistent set data initially failed explicitly; retained canonical
set support now has the separate evidence recorded in the compatibility inventory.
Unresolved reader prefixes still fail explicitly. The second-level vector stress fixture uses an
explicit 100,000,000-fuel allowance after the default allowance was exhausted.
That failed attempt remains failed validation; no fuel trap is called cancellation.

`sh scripts/test-rich-environment-oracle.sh` forces fresh development-only
ClojureScript compilation and runs the resulting Node artifact. Its strict
checker verifies sixteen ordered upstream binding/context/scope observations and
thirteen executed local counts. Only actual compiler-owned catch names are
normalized to `<private-catch>` using their role. User names and other facts stay
unchanged. These observations retain upstream `js` initializer representations;
Suss must describe its actual native arithmetic and receiver lowering rather than
fabricating a JavaScript AST. This oracle is upstream evidence, not a native
rich-environment acceptance test. No shipped Java/JavaScript dependency is added.

Current focused validation: sixteen tests pass across five native groups
(context, bindings, builders, function scopes and method roles); the earlier direct
form/metadata run passes nineteen across three groups. The required full workspace
baseline passes 981 tests with zero failures and 17 existing ignores across 101
groups, including all 47 runtime ABI tests and final reader doctests. Memoized
canonical environment graph construction, complete source
AST/inference, declaring/passing source `&env`, and all original M3 acceptance
remain unfinished. Keep this implementation branch together until transport is
reviewable; independent PR review and successful final-head CI are still required.

Source function facts now retain the actual validated methods before runtime
wrapper generation and duplicate fixed-arity elimination. Each method retains
its source parameter names, reader metadata and spans, variadic flag, and analyzed
body. Nested source functions have separate records. Queued native graph transport
shares one rooted `:suss/source-function` record between source ASTs and completed
catalog declarations; aliases do not acquire source function records merely
because their runtime value is callable. These fields are explicit compiler
extensions, not a completed portable `:fn-var` or method AST schema.

The focused executing regression checks fixed, variadic, duplicate-arity and
alias declarations, parameter hints, identity after GC, and actual invocation
results in both Stores. It fails on the reviewed prerequisite head and passes
with source method retention. Full validation of this continuation is pending.

The development-only `sh scripts/test-analysis-tags-oracle.sh` forces pinned
ClojureScript analysis, runs the Node artifact, and compares 32 observations and
11 executed scalar projections. The recorded corpus preserves field presence,
including present nil `:inferred-ret-tag`, distinct from absent fields. Arithmetic
`(+ "a" 1)` has inferred tag `number` while executing to the string `"a1"`.
A literal false condition still has a union result tag in the pinned analyzer,
and a caught value does not contribute to the observed `try` body tag. A function
var can have no tag; unknown invocation has `any`; a hinted local keeps its hint
when runtime storage differs. Portable inference must follow these observed
source rules rather than substitute physical HIR types. This reference evidence
does not certify native portable inference, full source ASTs, implicit `&env`,
bootstrap, or M3 acceptance. Helpers are original repository code; the development
compiler uses the existing pinned source and retained EPL notices.
