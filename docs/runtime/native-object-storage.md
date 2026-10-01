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
The complete runtime ABI suite passes 31 tests. This does not establish source factory,
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
