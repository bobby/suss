# Comparison foundation preparation

No comparison implementation is added by this preparation. The accepted design
and pinned source require separate runtime and macro behavior. Runtime <, <=, >,
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

The separate119-case primary corpus verifies primitive scalar/UTF-16 order,
signed zero, NaN, infinity, nil/undefined, unary effects, repeated middle effects,
short-circuit chains, runtime lookup, qualified macros and lexical shadowing.
Native execution currently fails unresolved comparison names. New regressions
also require located macro arity errors, universal runtime arity rejection,
erased unary throws, retained values through GC and lexical call behavior.
These are targets, not implemented or passing native claims.

Four additional capture probes record an explicit divergence, with exact pinned
TypeError name/message versus the accepted native outcome. The pinned generic
core wrapper reads a replacement global arity property even when invoked through
an old capture. The accepted design section7 requires an old captured function
to retain original behavior. This is the already documented arithmetic boundary
([arithmetic evidence](arithmetic-values.md)), not a new semantics decision.
The reference runner certifies the four exact errors separately; native tests
require the accepted old-capture outcome. Neither is labeled equivalent, skipped
or replaced by a fabricated success. Complete portable core acceptance must
retain this boundary explicitly.

Source provenance: pinned commitc4295f303100bbf5afac449242d30bca1126f1a1,
core.cljs2755–2801 (< through >=) and3064–3075 (numeric ==), plus core.cljc
1164–1187 (comparison macros). The source submodule retains upstream notices/EPL.
No upstream form is copied or imported here; no declaration status, ABI, runtime
helper, dependency or acceptance gate changed. Next implement verified binary
HIR/IR/runtime primitives, canonical first-class functions and bounded macro
expansion, execute this corpus, update partial reviews and require full baseline,
independent review and exact-head CI before PR readiness.
