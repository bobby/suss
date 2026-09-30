# Nominal types and protocols in progress

The replacement pipeline now lowers bounded `deftype`, `defprotocol`, `extend-type`,
`new`/dotted construction, `instance?` and `satisfies?` through verified HIR/IR and
shared ABI operations. This is original bootstrap/runtime code, not copied upstream
macro source or completed issue #11. Selected primitive/native fallback now has separate
[native protocol evidence](native-protocols.md). Arbitrary host classes/properties,
general field attributes, runtime metadata and compiled macro/core integration
remain unfinished. Scoped mutable fields have the bounded support described below.

User objects retain a Descriptor and field array in the existing ten-type prelude.
No per-source-type Wasm layout is generated. Descriptors retain schema names,
protocol tables and metadata, with diagnostic identities allocated per runtime
instance after seven error descriptors. Identity tests compare actual rooted
Descriptor references: equal numeric identities and equal layouts cannot forge a
matching type. Allocation rejects before the identity counter wraps.

A class value is a shared Invoke closure retaining its descriptor. Ordinary calls
return internal undefined, matching the pin's constructor-function behavior;
`new` and dotted calls construct objects separately. All operands evaluate once
in source order before checks. Source construction ignores extra arguments after
evaluating them and fills absent fields with internal undefined. The generated
`->Type` function reads the current type cell, so an old captured arrow sees a later
type replacement; an old captured class value retains its original descriptor.

Undefined uses internal i31 sentinel6, distinct from source nil0, false2 and true4.
It is falsey, numerically coerces to NaN and string-coerces to `undefined`; source
nil still coerces to zero/`null`. Ordinary protocol-object calls also return
undefined. A `defprotocol` expression also returns undefined; downstream numeric
and string coercions distinguish it from nil. The scalar development transport categorizes undefined as nil-like,
like the pinned encoder, but downstream arithmetic/string/truthiness regressions
retain its different behavior. Compiler manifests now identify `0.1.0+portable.2`;
pre-format2 artifacts reject before initialization. The recursive ABI/version1,
pinned numeric helper bytes/dependencies and numeric stack global6 are unchanged.
New runtime globals append after that stack: identity counter, nominal error and
a rooted opaque protocol sentinel (diagnostic identity0).

Protocols are callable values retaining internal bundles of marker/method keys.
Keys use canonical phase/namespace/name/method/arity identities and protected
compiler-owned defonce cells; redeclaration reuses them. Each method captures its
fixed-arity dispatcher keys. Calling an old captured method still consults the
live descriptor table, so an extension affects existing objects and dispatchers.
An already captured implementation closure retains its own environment.
`satisfies?` follows the resolved syntactic protocol name, rather than a runtime
value alias, and evaluates the receiver once. Its membership fast path avoids
reading a replaced protocol var; nil/undefined protocol values reject in the
fallback. User-object native property tables are an explicit unsupported boundary,
not fabricated false results. Selected native kinds and default fallback now execute with closure-owned tables;
arbitrary native object properties and full source/core integration remain unfinished.

Empty and partial protocol declarations use the shared opaque sentinel for
membership, matching the pin. An empty extension returns that same object, not a
boolean; a method extension returns the final implementation. Fixed overloads in
`deftype` require separate method forms. The implementation rejects grouped deftype
signatures instead of treating them as portable successful overloads. Grouped
`extend-type` signatures execute against the pinned reference. Method `this`
and implicit fields retain the original receiver across recur; the ignored first
recur operand still evaluates. Field reads occur at their original use, including
inside captured closures, rather than being hoisted before a method body.

Internal schema, argument, field, bundle and table arrays are not portable persistent
collections. Schemas/field storage are copied at construction. Table updates check
keys, storage and method shape before effects; growth publishes after copying.
Negative/out-of-range field indices, bad schemas/tables, unknown constructors,
unsupported receivers and missing methods throw typed language errors. Generic
invocation preserves central arity checks. Runtime failures return to the prompt;
compile failures publish no staged bindings.

The original private primitives include descriptor/object construction and identity,
checked fields, class/protocol values, separate checked-storage and source constructor
factories, checked protocol keys, live method/marker updates, dispatcher construction
and guarded membership. The compiler records nominal operations with ordered value
operands, verified arities/types/indices and source spans. Public malformed HIR/IR
rejects before emission. Namespace aliases, lexical shadowing and phase identities
remain in the existing resolution pipeline.

## Evidence and provenance

Pinned ClojureScript is `c4295f303100bbf5afac449242d30bca1126f1a1`; the primary semantic
sources are `src/main/clojure/cljs/core.cljc` at instance?1025–1032,
extend-type1668–1708, deftype1778–1851, defprotocol2041–2225 and satisfies?2253–2283,
plus native-satisfies? in `src/main/cljs/cljs/core.cljs`322 onward. The inventory and
review overlay retain exact source hashes. Upstream copyright/EPL notices remain in
that submodule and its LICENSE/epl-v10.html. These five original adaptations are
reviewed as in progress; no upstream form is copied/extracted or declared complete.
Reproducible extraction/patch records and full core license packaging remain M4 work.

Executing runtime tests use independently generated fragments and forced GC, including
forged identities, copied storage, live updates, retained captures and typed rejection.
Ten native nominal tests cover source forms, overloads, membership/redeclaration,
constructor aliases/arrows, effects, receiver anchoring, undefined coercion, marker
identity and compile-error atomicity. Public HIR/IR and phase/key guards have two
focused tests. The 245-case shared source corpus includes 35 nominal cases and executes
both pinned Node observations and independently decoded Suss fragments. These counts
are bounded evidence, not complete type/protocol/core or M2 acceptance. No PR readiness
is claimed before full baseline, independent review/fixes and exact-head CI.

## Scoped mutable fields

`deftype` recognizes pinned `:mutable`, `:unsynchronized-mutable` and
`:volatile-mutable` field metadata, including metadata maps. False/nil flags do
not make a field writable. Type tags are compile metadata, with no runtime value
restriction. Other field attributes remain explicitly unsupported.

Within a method or its nested closure, `set!` of a mutable field lowers to a
verified two-value nominal operation: the original physical receiver and the RHS.
The RHS evaluates once before the existing checked `object-field-set`, whose
return is the same assigned value. This uses the existing GC-owned field Args;
there is no new runtime helper, global registry, layout or ABI change. Aliases and
captured readers/setters retain the same owner. Receiver anchoring across `recur`
matches field reads. Lexical bindings/parameters shadow fields and remain
nonassignable; immutable fields reject before evaluation. Dynamic binding remains
restricted to global vars.

Pinned analyzer.cljc2727–2730 checks these three flags and rejects ordinary locals
and nonmutable fields; its deftype field metadata retention is at3624–3626.
The 28-case mutable-field corpus runs fresh pinned ClojureScript and independently
decoded Suss with forced GC. Additional native coverage retains a setter/reader
after clearing constructor and object globals. Compiler guards cover false/nil
flags, type-only hints, local shadowing, scope leakage and forged FieldSet HIR/IR.
This original lowering copies no upstream form. Sequence/list/hash-cache support
still requires source imports and acceptance; issue #11 remains incomplete.
