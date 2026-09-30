# Portable source exception regions

The replacement pipeline now lowers source `throw` and bounded `try` with ordered
nominal catches, a final `:default` catch, and optional `finally`. It uses the
existing shared `language-exception` tag carrying a portable Value. Nil, false,
binary64 values, UTF-16 strings, objects and closures are thrown without wrapping
or replacing their identity. Typed catch tests compare actual nominal descriptors;
equal Wasm layouts do not establish a match. Runtime diagnostic exceptions also
use this tag and can be handled with `:default`.

HIR retains source spans, throw operands and compiled body/handler/cleanup regions.
These private regions capture lexical bindings through the existing shared Invoke
closure ABI; they do not replay source or run an interpreter. Catch type operands
are evaluated only on an exception, in source order. Catch names shadow outer
locals only within their handler, and captured payloads remain rooted across GC.
Regions reject recurrence to an enclosing loop/function, while loops and functions
created inside a region retain their own legal recurrence targets.

IR represents throw as a terminal edge using an already evaluated value. Divergent
operands stop subsequent callee/argument/binding/initializer effects and publication.
A defonce bound path still reaches its join when the unbound path throws. IR Try
operands identify three compiled regions; verification checks body/cleanup arity0,
handler arity1, optional nil regions, operand dominance and dynamic result shape.
Malformed public HIR/IR rejects before emission.

Fragments import the existing runtime exception tag only if they actually throw.
The original `try-invoke` helper invokes the body and selects a handler on a typed
language exception. A second typed region retains normal results or pending body/
handler payloads until cleanup finishes. Cleanup runs exactly once; its result is
discarded, and its own exception supersedes a pending exception. An unmatched catch
rethrows the exact original value. Ordinary failures return to the persistent prompt;
a failed definition initializer retains the previous binding, while already performed
effects remain. No runtime ABI layout, compiler format, dependency or numeric-helper
artifact change is required for these additive operations.

Wasm fuel traps and foreign host exception tags remain distinct from portable language
exceptions. They do not become catchable nil values. Host cancellation/async cleanup,
ExceptionInfo/public Error classes, dynamic binding frames, compiled macro/core import,
collections and production CLI/AOT migration remain explicit roadmap work. This
increment does not fulfill all issue #11 criteria or complete M2.

## Evidence and provenance

The semantic reference is pinned ClojureScript
`c4295f303100bbf5afac449242d30bca1126f1a1`, especially
`src/main/clojure/cljs/analyzer.cljc`1891–1994. The source/runtime implementation is
original Rust/Wasm code, not an extracted upstream form. Upstream source and its EPL
notice remain in the pinned development-only submodule. No JVM or Node is introduced
in shipped code, and no inventory item is classified implemented by this increment.

Ten executing native regressions cover exact thrown scalars and GC, typed nominal
selection, handler capture/shadowing, cleanup/superseding exceptions, ordered divergent
operands, failed publication and located compile atomicity. Independent review adds unhandled
nil/closure cleanup with GC, lazy ordered computed catch tests (including a throwing
test), and field/loop capture through nested regions. Two public HIR/IR guard
tests cover malformed handler/throw shapes and cross-region recurrence. The shared
259-case source corpus includes14 exception observations matched against fresh pinned
Node execution and independently decoded Suss fragments. These are bounded executing
results, not full exception/core/dynamic-scope compatibility. The initial six source
regressions failed before implementation; failures and commands remain in the handoff.
