# Named and multiple fixed function signatures

The replacement pipeline executes named fn/fn* and multiple fixed signatures
through the existing shared Invoke type. A named self reference is the same GC
closure value as the created function, including through nested captures and
ordinary recursive calls. Its lexical scope includes its bodies and can be
shadowed by a parameter. Each signature has its own parameters and recur target;
recur cannot transfer to another signature or an outer function target.

A GeneralFunction HIR node retains methods, deterministic shared outer captures
and the named binding's identity/span/metadata. MakeGeneralClosure IR records
verified method bodies and exact fixed arities. Each body receives unified outer
captures, an optional dynamic self capture, then user parameters. Named construction
reserves one private environment slot, creates the closure, initializes that slot
with the closure reference and publishes only after initialization. Immutable outer
captures are never rewritten. GC owns the resulting cycle; old functions retain
self identity and behavior after a global replacement.

The shared runtime invoke first validates callable/argument-array type and
minimum/maximum arity. A generated Invoke dispatcher reads argument length once,
selects an exact method, and calls the shared typed arity-error helper for holes
in the range. No body reads arguments or performs effects for an unsupported
arity. Callee and argument expressions have already executed once in source order.
The helper uses the same rooted wrong-arity descriptor, language tag, message and
nil data/cause as central invocation. Call-site exception annotations remain future
integration, as for prior dynamic calls.

Source duplicate fixed arities preserve the pinned compiler's last-body behavior;
its warning is retained in the reference logs. Analyzer capture sets are recomputed
from selected bodies after normalization. Public IR rejects duplicate method
arities, empty methods, malformed environment/self shape, changed capture facts
and unbounded arity before emission. This normalization does not accept malformed
public IR. Every generated function uses the exact shared Invoke recursive type;
actual Wasm validation/linking and independently decoded execution gate evidence.

The recursive ABI layout/version remains 1. arity-error is an additive private
export; actual import linking rejects an older runtime missing it before eval.
No new dependency, copied core implementation, license change, private helper
artifact change or shipped Java/Node is introduced. Implementation is original;
semantic provenance is ClojureScript c4295f303100bbf5afac449242d30bca1126f1a1,
core.cljc fn expansion and analyzer.cljc fn* parsing (upstream EPL-1.0).

Evidence: the original source regression failed with a located unsupported
signature diagnostic before implementation; it now passes. Native tests inspect
exact self reference identity after GC, nested self captures, old function behavior
after rebinding, typed arity holes with retained ordered effects, duplicate arities
and shadowing. Public IR mutation tests reject malformed method/capture/self shapes.
Sixteen new scalar signature cases are in the 210-case common source corpus and
match fresh pinned observations plus actual generated Wasm decoding. Reference-only
identity probes supplement the independently inspected native identity assertions.

Variadic rest needs a proper portable persistent sequence, not the ABI argument
array. It remains unfinished with destructuring, pre/post conditions, complete
core macros/import, general nominal/callable dispatch and production frontend
migration. No upstream inventory declaration is marked implemented by this bounded
compiler increment. Issue #9 and M2–M9 remain incomplete; issue #10's audited runtime
foundation acceptance remains separate from full source signature/core coverage.
