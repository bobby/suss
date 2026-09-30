# Native protocol dispatch in progress

The portable bootstrap extends declared protocols to nil, Boolean, Number,
UTF-16 String, function and ordinary object receiver kinds, with default fallback.
This is a prerequisite for sequence and collection core forms; it does not port
persistent collections or complete M2-04/M4 acceptance. Source arrays, JS Symbol,
bigint and arbitrary host class/property interoperability remain unfinished.
The private array kind can classify existing internal argument arrays; those
arrays are not public persistent collections or a claim of JS array source support.

Dispatch evaluates receiver/arguments once before inspecting values. A direct
UserObject descriptor method has priority, followed by the current method var's
specific native implementation and then its default implementation. Missing methods
and wrong arities produce typed language errors, not Wasm traps. Nil and internal
undefined select the same native kind, while false selects Boolean.

Native implementations replace an entire function for one receiver kind, including
its fixed arities. A valid protocol call selecting a plain fixed implementation
fills missing parameters with internal undefined and ignores already evaluated
surplus operands, matching the pinned native call convention. Multiple-signature
implementations retain their exact signature dispatch. Their first argument is an ordinary function
parameter: recur may replace it. Nominal methods continue to anchor their implicit
receiver and store independent prototype method slots.

Captured protocol method functions consult the current method cell for native
fallback tables. Replacing that var with an ordinary function removes its former
native table. Re-extending the replacement makes an old captured dispatcher call
the new implementation. Redeclaring defprotocol replaces its protocol/method
values and native tables; direct methods on existing nominal objects retain their
stable keys. Native membership reads the current protocol value after the existing
syntactic marker fast path. Native empty extension returns true, whereas a nominal
empty extension returns the opaque upstream marker.

## GC ownership and shared ABI

Shipped closure factories wrap the original environment in an existing UserObject
whose rooted descriptor identifies private property storage. Its field array owns
the original environment and a mutable array of native-kind/value pairs. Mutation
checks kinds/storage and copies a growing array before publication. No global
registry retains all replaced closures. The wrapper/table/implementations remain
reachable from their owning function and can be collected with it.

Universal invocation and original environment inspectors unwrap the environment
before invoking the unchanged shared callback. The ten-type recursive prelude,
ABI version, numeric helper and its stack/global indices remain unchanged. Only
one private descriptor global is appended after the earlier runtime globals.
Independent foreign closures with raw environments remain callable. Assigning
native properties to a foreign raw closure is an explicit unsupported boundary,
with a typed diagnostic; no lifetime-leaking side table is fabricated for it.

The compiler uses a private GlobalCell HIR/IR operand to retain the live method
cell without evaluating its current value during protocol construction. It records
ordered NativeMarker/NativeSet operations and grouped native implementation
functions. Source names and phase identities remain in the existing resolver.

## Primary sources and evidence

Pinned ClojureScript commit c4295f303100bbf5afac449242d30bca1126f1a1:
core.cljc1332 base-type,1477 base-assign-impls,2124–2147 expand-dyn,
and core.cljs322 native-satisfies?. Original generated runtime/macro bootstrap
code is Rust, with no copied upstream form or new shipped JVM/JS dependency.
The pinned submodule retains upstream copyright and EPL notices/licenses.

Development-only tests/oracle/native-protocol-cases.json and its runner encode
primary results independently. Initial22 fresh observations prove fallback order,
redefinition/redeclaration and native Boolean extension results; the expanded
29-case corpus adds whole-function replacement, receiver-changing recur,
missing/surplus native parameters, a public first-class native-satisfies? function
and its live macro fallback.
Fresh29 observations match exactly; the full locked workspace baseline passes. Seven missing-feature native regressions initially failed; native10
now passes against actual Wasm execution, including the corpus decoder. The
surrounding session/core/dynamic/exception/predicate/compiler suites pass.
Low-level ABI16 passes, including ownership/foreign-environment checks.

Complete compiled macros, source warnings/options, protocol implementations via
metadata, arbitrary UserObject native property tables, full Error surfaces and
collection/production frontend migration remain unfinished. Review/provenance
statuses stay in progress. No issue closure or merge follows from these foundations.
