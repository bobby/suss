# Comparison primitives and bounded macros

Original checked comparison primitives now implement separate runtime and bounded
macro behavior through verified HIR/IR and the shared ten-type GC ABI. Runtime <, <=, >,
>= and numeric == are first-class functions with one/two/variadic signatures;
caller operands evaluate before invocation. Relational strings compare UTF-16
units when both primitives are strings, otherwise numeric coercion applies.
Runtime numeric == documents non-number behavior as undefined; full collection
IEquiv/equality remains separate work.

Pinned core.cljc comparison macros1164–1187 erase unary operand syntax and expand
variadic chains through short-circuiting binary comparisons. Middle syntax can
appear again in a later comparison. Macro == uses strict primitive identity;
its nil/undefined, Boolean/number and object-reference behavior is not the
runtime numeric equivalence implementation. Lexical callable bindings hide macro
lookup. Namespace/qualified macro versus runtime-var replacement needs explicit
source-backed execution, not inference from symbol spelling.

The separate123-case primary corpus verifies primitive scalar/UTF-16 order,
signed zero, NaN, infinity, nil/undefined, unary effects, repeated middle effects,
short-circuit chains, runtime lookup, qualified macros and lexical shadowing.
Native execution now passes this corpus, preserving the original119 observations
and adding four independent thrown-operand/finally/short-circuit probes. Additional regressions cover located
macro arity/resource errors, universal runtime arity rejection, erased unary
throws, retained values through GC, lexical and namespace resolution (including
explicit user refers retaining automatic core macros while provider-qualified
aliases invoke user vars), a300-operand
runtime call, UTF-16 loop fuel recovery and typed unsupported-object coercion.
Public HIR/IR guards reject wrong arity/result types and non-dominating operands.

Five additional capture probes record an explicit divergence, with exact pinned
TypeError name/message versus the accepted native outcome. The pinned generic
core wrapper reads a replacement global arity property even when invoked through
an old capture. The accepted design section7 requires an old captured function
to retain original behavior. This is the already documented arithmetic boundary
([arithmetic evidence](arithmetic-values.md)), not a new semantics decision.
The reference runner certifies the five exact errors separately; native tests
require the accepted old-capture outcome. Neither is labeled equivalent, skipped
or replaced by a fabricated success. Complete portable core acceptance must
retain this boundary explicitly.

Source provenance: pinned commitc4295f303100bbf5afac449242d30bca1126f1a1,
core.cljs2755–2801 (< through >=) and3064–3075 (numeric ==), plus core.cljc
1164–1187 (comparison macros). The source submodule retains upstream notices/EPL.
No upstream form is copied or imported here. Ten source declarations have partial
hash-bound reviews. There is no layout/global/prelude/version or dependency change;
the comparison functions append to the original runtime. Canonical bootstrap
cells now total29 (previously24). Relational string loops compare unsigned UTF-16
units; other supported primitives use existing numeric coercion. Strict identity
uses the private predicate factory, independent of mutable public predicate cells.
Public numeric== support is certified for numbers; non-number behavior remains
outside its documented upstream guarantee, and complete IEquiv/collection equality
is separate work; the public -equiv protocol hook and its redefinition are not
connected by this primitive bootstrap. Arbitrary object coercion and complete source/core/macro import
remain unfinished. Recursive macro expansion is limited to256 operands with a
located diagnostic; runtime first-class calls use constant callback code and a
loop. Full baseline, independent review/pushed fixes and exact-head CI are required
before PR readiness. No acceptance gate is complete from this prerequisite slice.

A separate ten-case pinned/native referral corpus certifies all five operators:
with a user provider explicitly referred into another namespace, unqualified
calls still expand the automatic core macro. Provider-qualified alias calls
invoke the user functions. This differs from an own runtime declaration hiding
an automatically referred comparison macro. Independent fresh source compilation
corrected the initial review's unsupported inference that both behaved alike.
