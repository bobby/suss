# Retained cached string hashing

Five additional pinned runtime forms are retained with original source hashes,
EPL notices and explicit adaptations: js-obj, string-hash-cache,
string-hash-cache-count, add-to-string-hash-cache and hash-string. The recipe
selects50 forms and produces54 licensed artifacts. The strict manual overlay has
139 partial reviews and926 unassessed declarations. Provenance is separate from
execution evidence; M2–M9 and full public hashing remain incomplete.

The runtime js-obj form becomes a function-valued checked native factory, keeping
first-class/captured/live binding behavior and evaluated variadic argument handling.
The two cache globals retain upstream source. Fixed defn patches preserve the
add/hash algorithms, including live hash-string* lookup, write-before-counter order,
threshold greater than1024, object replacement/reset before nil handling, and the
number? hit test. The strict Closure gobject/set operation uses a separate
checked strict property adapter. Getter-only and readonly writes throw before incrementing
the cache counter; unchecked-set retains non-strict ignored-write behavior.
The adapted hash-string retains unchecked-get; bounded lowering
of unchecked-get/set evaluates each operand once in source order. This is not
compiled macro bootstrap acceptance.

The native cache uses real owned Object properties and callable prototype methods,
not a persistent map substitute. Inherited __proto__ numeric writes are ignored,
so repeated misses still increment the counter. Actual data/accessor descriptors,
receiver-aware calls, scalar UTF-16 keys and cycle/root guards are documented in
[native object storage](native-object-storage.md). Named member get/set delegates
owned objects to these adapters and preserves the existing fixed-schema class,
String and Array path for other owners.

## Executing evidence

All70 certified source observations match fresh pinned Node observations
and independently decoded native validated Wasm after forced GC. The original48
were preserved and16 object/key/prototype probes were added before implementation.
Six independent review probes additionally cover inherited getter effects,
inherited setters, throwing writes, getter-only cache failures and non-strict
bracket writes, plus strict first-class factory pair assignment after a prototype
pair. The original corpus includes UTF-16/lone-surrogate and
prototype-named keys, inherited function
values, __proto__ repeated misses, own/inherited data, null prototypes, cycles,
scalar property keys, effects, cache reset boundaries, false/zero/NaN caches,
throwing misses, cached-hit suppression and live hasher captures.

Three additional source guards establish aliases, first-class factories/arrays,
member invocation, own accessor reflection, mutable properties, live dependencies,
lexical macro shadowing, compile-atomic property arity errors, runtime core arities
and recovery. Original private adapter tests pass2; the ABI suite passes41 with
malformed storage/descriptor/buffer, copied callback, prototype and GC coverage.
Python76 and strict inventory/reviews/import checks pass. Full workspace results,
independent review and exact final-head CI must be recorded before PR readiness.

## Boundaries and next work

The separate upstream literal js-obj macro is not implemented or claimed; its
partition/deduplication/key handling differs from the retained runtime function.
Unchecked bracket macros currently support owned native objects with scalar keys.
General bracket access to foreign objects, strings and arrays, arbitrary ToPrimitive,
primitive boxing, Symbol keys/toStringTag and public define/delete APIs remain open.
Do not infer those from native member fallback or a function-valued runtime factory.
Source persistent collections, full hash/equality, compiled macro phase execution
and M2–M9 acceptance remain separate work. The pinned hash algorithm is unchanged;
future algorithm evaluation is deferred in issue98.
