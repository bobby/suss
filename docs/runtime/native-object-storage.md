# Owned dynamic property storage

This original runtime kernel is a prerequisite for the adapted string hash cache.
It implements own data storage and raw prototype chains, not full JavaScript
prototype behavior or public
`js-obj`. The source cache corpus still fails natively and is not acceptance
evidence for this kernel.

`native-object-new` creates an existing shared GC UserObject with a private
descriptor identity. Its owned header contains an alternating UTF-16 key/value
table and a prototype slot. No process registry retains objects.
The shared type layout and ABI version are unchanged. This private object is
not a persistent map and does not implement persistent collection contracts.

`native-object-own-slot`, `native-object-own-get` and `native-object-own-set`
compare raw UTF-16 content, preserving empty strings, astral units and lone
surrogates. They do not normalize protocol names such as `object` or `array`.
Missing keys return language Undefined. Replacing a property preserves the owner;
adding a property allocates a new owned table with bounded length. The setter
returns the stored value. Physical null references, foreign owners, malformed
pair tables and non-string keys raise language exceptions. Language nil is a
valid value. These own-storage operations treat `__proto__` as an ordinary key;
the future prototype adapter must intercept inherited accessor writes.

The two `runtime_abi_owned_dynamic_properties_*` tests validate and instantiate
actual Wasm and exercise table growth, replacement, separate owners, literal
reserved keys, UTF-16, forced GC, corrupt storage and typed recovery. Three additional prototype tests exercise inherited lookup, Undefined shadowing,
atomic cycle rejection, forged cycles and a 130-object chain after forced GC.
The existing 34 runtime ABI tests pass, including the bounded default
prototype method checks below. A new own-accessor reflection regression fails
until actual descriptor storage is implemented. This does not establish source factory,
property-key conversion, prototype accessors, inherited methods or cache behavior.

`native-object-prototype` reads and validates the immediate raw prototype;
`native-object-prototype-set` accepts an owned object or nil and validates the
complete candidate chain before mutation. It rejects cycles atomically.
`native-object-chain-get` returns the first present own value, including Undefined,
or language Undefined for an absent key. An iterative Floyd check rejects forged
cycles before traversal, without recursion, registries or a fixed chain depth cap.
These exports do not coerce keys or invoke a JavaScript accessor. Raw prototype
set rejects primitive values; the future inherited `__proto__` setter must instead
ignore primitive assignments.

Next implement the default Object prototype with genuine inherited function values,
primitive `__proto__` writes ignored, null prototype removal, own data shadows and
cycle rejection. Then wire source adapters and retain the pinned cache forms with
EPL/source provenance. Preserve the 64 certified oracle observations, including
the original 48, rather than changing expectations to fit implementation.


## Default prototype preparation

`native-object-default-new` creates an object with one lazy GC-rooted shared
prototype. `native-object-default-prototype` exposes that root internally.
Five real function values currently live there: `constructor`, `toString`,
`valueOf`, `hasOwnProperty` and `isPrototypeOf`. The four member methods use the
existing unbound Object wrapper convention; member invocation supplies the
receiver and detached invocation does not retain an object owner.

For owned objects, toString returns `[object Object]`, valueOf returns the same
receiver, hasOwnProperty distinguishes own presence from inherited properties,
and isPrototypeOf walks the actual validated chain. Undefined/null toString
receivers use their corresponding object tags. Detached valueOf/hasOwnProperty
raise language exceptions. isPrototypeOf returns false for missing or supported
primitive arguments before validating the receiver; a detached object argument
raises an exception. Constructor calls create a default object for missing/nil/
Undefined arguments and preserve identity for an existing owned object.

Primitive boxing, other object kinds, Symbol.toStringTag, the remaining Object
methods, property attributes and legacy accessors remain unsupported. This root
is preparation, not a complete public Object prototype or source cache success.
No placeholder function is provided for an unfinished method. To finish cache
integration, add real remaining methods and attributes, inherited __proto__ getter/
setter semantics, primitive-key conversion and immutable default prototype rules.
Do not replace these with own-map behavior or false successful observations.

The executing default-method regression covers shared prototype identity, actual
member/detached calls, constructor identity/allocation, own nil-valued presence,
inherited versus own keys, argument-sensitive detached isPrototypeOf behavior,
and forced GC/error recovery. A development Node check confirmed the detached
call distinctions; it is not a fresh pinned ClojureScript corpus comparison.
Specification reference: [ECMAScript Object prototype operations](https://tc39.es/ecma262/multipage/fundamental-objects.html#sec-properties-of-the-object-prototype-object).


## Scalar properties and legacy prototype accessor preparation

`native-object-property-get` and `native-object-property-set` convert supported
scalar keys through the existing checked UTF-16 string coercion. They layer the
legacy `__proto__` operation above raw storage: a nearer own/inherited data slot
shadows the default accessor, primitive setter values are ignored, nil removes
the chain, and subsequent writes on a null-prototype object become own data.
Accessor reads return the original receiver's prototype. Cycle errors preserve
the former chain, and changing the shared default root's prototype fails; setting
its existing nil prototype remains allowed. Foreign object domains and physical
null references remain language errors instead of successful unknown coercions.

Two executing tests cover these operations and scalar keys (negative zero, NaN,
nil, booleans, Undefined, fractional numbers, empty/astral/lone-surrogate strings),
forced GC and typed recovery. Development Node assertions independently confirm
the write/shadow/root rules. This is not a pinned source cache comparison.

The default accessor is currently recognized by root identity, not a stored
property descriptor. An executing regression confirms this unfinished behavior: `hasOwnProperty` on the root with key
`__proto__` currently returns false, whereas Node returns true. Property attributes,
reflection, custom accessors and deletion require actual owned descriptors;
do not claim these operations based on the adapter. Replace the implicit default
accessor with that descriptor representation, implement remaining methods, and
then wire source cache forms. The source corpus still has 64 unresolved native
failures, and no public factory/property compatibility gate is complete.
Specification reference: [ECMAScript legacy prototype accessor](https://tc39.es/ecma262/multipage/additional-ecmascript-features-for-web-browsers.html#sec-object.prototype.__proto__).
