# Named properties for retained source

The compiler lowers literal property reads and assignments needed by retained
source, including List.EMPTY and UTF-16 string length. Named class/function
properties use the existing GC-owned closure wrapper's key/value table. Public
native names null/boolean/number/string/function/object/array/_ alias its existing
i31 dispatch slots0–7; other UTF-16 names use content equality. Known instance
fields resolve through their UTF-16 schema and retain existing field storage.
Callback environments remain intact; no owner registry retains replaced values.

Sixty-four fresh pinned ClojureScript observations at
c4295f303100bbf5afac449242d30bca1126f1a1 match independently decoded actual Wasm.
The original22 observations remain unchanged. Additions cover native/name storage,
extensions, public native aliases, captured environments, nested rooted owners,
length fields, dynamic undefined errors, zero/false values and evaluation order.
Four native tests force GC and cover located malformed forms, compile isolation,
unsupported writes and recovery. A direct ABI regression rejects forged opaque
keys, bad strides and wrong owner/name values as language exceptions, never traps.

Membership follows the pin's unchecked-if property truthiness: nil, undefined,
false, positive/negative zero, NaN and empty strings are false. Ordinary language
conditionals retain their separate ClojureScript truthiness. Native method dispatch
checks nil/undefined before trying the default slot; other noncallable entries
raise an exception. Probes preserve this distinction instead of treating false
methods as missing. Public marker/method writes affect the same native tables.

Properties preserve owner identity, assigned values and once-only source order.
Class/function replacement leaves retained owners and their properties valid.
Strings/source arrays expose actual UTF-16/indexed length. Missing owned
class/function or declared-instance names return undefined. Dynamic nil/undefined
access raises a language exception. Literal nil dot forms are compile errors,
matching a separately observed analyzer boundary.

The shared ten-type prelude, existing globals and ABI version are unchanged.
Six checked named-storage helpers and a native-membership truthiness helper are
appended; fragments import get/set by name. No shipped Java/Node dependency is
added. Property tables remain even-length; schema length must match field storage.
Malformed names/keys are rejected. Table lookup validates every key before returning a remembered first match, so malformed tail entries cannot hide behind a valid prefix. Expansion follows existing bounded array limits.

Supported source names start with a literal ASCII letter/_/$ and continue with
ASCII letters/digits/_/$. Munged/computed names, extra instance fields,
array/string length writes, other array names and unsupported owner shapes
produce explicit diagnostics. Instance schemas with reserved words or characters
requiring source-name munging reject named access; lexical field access retains
its existing slot semantics. The guard checks the pinned analyzer reserved
spellings and compiler munging boundary. It does not adapt names. For example,
the pin stores source field null as null$: raw .-null is undefined and .-null$
reads the field. Both named forms raise an explicit unsupported-language error
in this partial implementation.

Callable prototype/length/name/caller/arguments/call/apply/bind and inherited
Object attribute names raise explicit errors. Missing instance inherited names
such as constructor/__proto__/toString also raise errors; a declared unmunged
own field takes precedence. This bounded guard prevents these known unfinished
attributes from becoming false missing-property successes.

Prototype/Object method blocks, automatic host/function attributes, coercion,
reflection/metadata and general JS interop remain unfinished. Own-property probes
do not certify them. Actual List/EmptyList/Cons/IndexedSeq, canonical empty-list
literals and full hashing/reduction/rest/apply remain unfinished.

```sh
CARGO_BUILD_JOBS=2 sh scripts/test-named-property-oracle.sh
```

Development history remains explicit: the literal-nil candidate failed in the
pinned analyzer; a dynamic-parameter case then certified22 primary observations
and failed natively on unresolved .-EMPTY. The first implementation passed those22;
32 extended observations passed before native aliases were tested. Public native
names initially failed to resolve their existing slots, false marker writes then
exposed incorrect membership, and undefined method writes exposed a missing
fallback. These failures were fixed against fresh source evidence, with49 exact
primary observations and both native tests passing. The intentional generated
->PropertyProbe replacement warning remains visible. See the handoff for commands
and logs. No new upstream form is copied and review statuses remain partial.

Independent PR review added15 source probes without changing the original49: all
remaining public native slots, null/undefined aliasing, number/_ instance
fields, assignment error effects, stored nil and object-marker truthiness.
Noncallable method probes use different default and catch values, distinguishing
fallback from rejection. Fresh64 primary observations and all four native tests
pass after malformed-tail, munged-schema and known host-attribute guards. Full
review evidence is recorded in the handoff.

PR #93 review extends the explicit unsupported-schema guard to raw __proto__.
Pinned constructors assign this.__proto__, invoking its inherited setter rather
than creating an own field. Source type declarations reject this spelling before
physical protocol slots can fabricate a field; host-created schemas reject named
get/set through checked language exceptions. The existing64-case corpus is unchanged.

Source closure display names now live in the same GC-owned wrapper, in a third
private payload slot beside the original environment and property table. They do
not share the mutable property table or a nominal descriptor's metadata/UID slots.
All wrapper readers validate the new three-slot payload; raw foreign closure
environments still follow the existing separate path. A source function has its
real scoped/munged UTF16 name, or an empty name when genuinely anonymous. Unset
kernel names remain unavailable and raise a language exception when queried.
This does not certify names for synthetic protocol/delegate/foreign callbacks,
constructor display names or general JavaScript function-property interop.
