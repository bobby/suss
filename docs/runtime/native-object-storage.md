# Owned native objects and descriptors

This original runtime code is a prerequisite for adapted string hash caching.
The source cache corpus still has 64 unresolved native failures. Executing ABI
tests below do not establish source `js-obj`, public properties or cache acceptance.

## Owned storage

`native-object-new` creates an existing shared GC UserObject with a private
descriptor identity. Its owned header contains an alternating UTF-16 key/property
descriptor table and a prototype. No process registry retains objects. Shared
GC types, ABI version and core cell count are unchanged. These objects are not
persistent maps or persistent collection implementations.

Private property descriptors reuse owned GC arrays: `[flags, payload]`. Flags
are writable1, enumerable2, configurable4, accessor8. Data payloads are non-null
language values. Accessor payloads contain two callable values or Undefined:
getter and setter. Checks reject malformed lengths, invalid flags, writable
accessor descriptors, physical nulls and noncallable accessors with language
exceptions before mutation. Keys compare raw UTF-16 content, preserving empty
strings, astral units and lone surrogates without protocol-name normalization.

`native-object-own-descriptor` returns an own descriptor or Undefined;
`native-object-own-get` reads its raw payload without invoking accessors.
`native-object-own-store` and `native-object-own-define` are internal storage
operations; they do not implement public Object.defineProperty restrictions.
`native-object-own-set` defines an ordinary data property with flags7. Internal
operations can redefine properties; future public definitions/deletion must enforce
configurability and descriptor compatibility. This boundary is not public success.

## Prototypes and scalar properties

Raw prototype operations accept owned objects or nil, validate candidate chains
before mutation, and reject owner-containing cycles atomically. Iterative Floyd
checking rejects forged cycles without recursion, registries or arbitrary depth
limits. Raw chain lookup returns the first present own payload, including Undefined.

`native-object-property-descriptor` resolves a stored descriptor through the chain.
`native-object-property-get` and `native-object-property-set` convert supported
scalar keys through checked UTF-16 coercion and invoke getter/setter values through
the existing receiver-aware call path. Getter-less accessors return Undefined;
setter-less accessors and non-writable data ignore writes in this bounded non-strict
adapter. Writable own data retains its flags; writable inherited data becomes an
ordinary own property. Assigned values are evaluated before calling this layer.
Physical null and unsupported foreign object domains remain explicit errors.

The shared default root owns an actual non-enumerable, configurable `__proto__`
accessor descriptor. Its callable getter returns the original receiver's prototype.
Its setter ignores supported primitives, permits nil removal, rejects cycles and
protects the default root's immutable prototype (setting its existing nil is allowed).
Nearer own/inherited data descriptors shadow the accessor. After nil removal,
subsequent `__proto__` writes can create ordinary own data. Root-identity lookup
special-casing has been removed; hasOwnProperty correctly observes the descriptor.

## Default builtin preparation

`native-object-default-new` attaches one lazy GC-rooted shared prototype. Eleven
real functions currently live there: constructor, toString, valueOf, hasOwnProperty,
isPrototypeOf, propertyIsEnumerable, toLocaleString, __defineGetter__,
__defineSetter__, __lookupGetter__ and __lookupSetter__. Builtin data descriptors
are writable/configurable and non-enumerable (flags5). Methods are unbound values;
member calls supply a receiver, and detached calls retain no owner.

For owned objects, toString returns the Object tag, valueOf preserves receiver
identity, hasOwnProperty checks presence, isPrototypeOf checks the actual chain,
and propertyIsEnumerable reads descriptor flags. toLocaleString reads and invokes
the current toString property, including overrides and accessor lookup. Undefined/
nil toString uses its corresponding tag. Detached isPrototypeOf returns false for
missing/supported primitive arguments before receiver validation; object arguments
throw for undefined this. Constructor creates a default object for missing/nil/
Undefined arguments and preserves an existing owned object's identity.

Legacy definitions check callback callability before scalar key conversion, reject
non-configurable own properties, preserve the other half of an own accessor, and
create enumerable/configurable accessors. Lookup follows the actual prototype
chain and stops at a nearer data descriptor without invoking getters.

Primitive boxing, other
object kinds, Symbol.toStringTag, public descriptor definition/deletion and source
factory/property adapters remain unfinished. No placeholder functions stand in
for those operations. Complete these boundaries before claiming public Object or
cached hashing compatibility.

## Executing evidence

The complete runtime ABI suite passes 40 tests. Owned-object tests validate and
instantiate actual Wasm, covering storage growth/replacement, separate owners,
UTF-16, malformed tables/descriptors, typed recovery, inherited values, Undefined
shadowing, atomic cycle rejection, a 130-object chain and forced GC. Method tests
exercise member/detached calls, constructor identity/allocation, own nil-valued
presence, descriptor attributes, readonly writes, copied getter/setter receiver
behavior, root reflection and live toString lookup. The former failing own-accessor
reflection regression now passes unchanged. Independent development Node assertions
confirm bounded descriptor/accessor behavior; they are not a fresh pinned
ClojureScript corpus comparison. No full workspace/review/final CI claim on this
unpublished preparation branch.

Next wire source factory/property
adapters and retain cache forms with EPL/source provenance. Preserve all 64
certified source observations, including the original 48; require fresh native
agreement, independent PR review, full workspace tests and exact final-head CI.

Specification references: [Object prototype operations](https://tc39.es/ecma262/multipage/fundamental-objects.html#sec-properties-of-the-object-prototype-object),
[legacy prototype accessor](https://tc39.es/ecma262/multipage/additional-ecmascript-features-for-web-browsers.html#sec-object.prototype.__proto__).


## Private compiled source adapters

Three private `suss.bootstrap` operations are checked by HIR and IR and emitted
with typed shared-runtime imports: `object-factory` (zero operands, returns a
function), `object-get` (two operands) and `object-set` (three operands). Ordinary
operand lowering preserves source order. These are not public core bindings or
a zero-argument-only public factory.

The original factory callback accepts evaluated variadic arguments, then rejects
odd pairs with a language error. It creates a default owned object and assigns
pairs through the descriptor-aware scalar property adapter. A single source-native
array is flattened recursively; iterative cycle detection rejects self and mutual
array cycles without stack recursion or depth limits. Checked array storage remains
an interoperability argument source, not a persistent collection substitution.
The returned function can be captured, invoked across fragments and retained after
rebinding a live var. No new bootstrap core cells or singleton factory registry.

Two executing compiled-source tests cover empty/pair/nested-array factory calls,
property effects in order12345, scalar keys, prototypes, argument effects before
odd-arity errors, compile-atomic private arity errors, malformed runtime inputs,
self/mutual cycles, capture/rebinding and forced GC recovery. A separate ABI guard
tests physically foreign/null flatten buffers and recovery after a forged key.
No pinned source oracle match is claimed for these original private adapters.

Next retain the licensed public js-obj/cache forms and compile their bracket
property macro dependencies. Preserve the64 certified source observations and
require fresh primary/native agreement before opening the coherent PR slice.
