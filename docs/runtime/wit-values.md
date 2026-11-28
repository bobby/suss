# WIT values and core collection operations

WIT lists enter source functions as owned persistent vectors. Ordinary core
operations apply to those values: `count`, `nth`, `conj`, `map`, `reduce` and
the usual equality and hashing contracts. For example:

```clojure
(defn ^:export append [xs x] (conj xs x))
```

```wit
export append: func(xs: list<char>, x: char) -> list<char>;
```

The canonical input buffers are copied into GC storage and released before
the source function executes. A value retained in an atom survives later calls
and GC independently of canonical memory reuse. Returned vectors are validated
against the selected WIT element schema and copied into owned canonical result
buffers; post-return releases those buffers. A numeric list result can also
come from its documented packed-array element type.

Tuples use persistent vectors and records use keyword-keyed maps. These shapes
compose with nested lists, options and results. WIT maps are
persistent maps and flags are keyword sets. A resource handle has identity, ownership and borrow
lifetimes; collection behavior on a host-backed resource requires an explicit
protocol implementation and a documented lifetime/error contract.

## Unicode scalar values

WIT `char` carries one Unicode scalar. Suss represents it as an ordinary string
with one UTF-16 unit for a BMP scalar or two units for an astral scalar. Thus
`count` of an incoming astral `char` is two. NUL and Unicode noncharacters are
valid scalars. Empty strings, multiple scalars and unpaired surrogates cannot
leave through a `char` result. They raise a language boundary exception naming
the export. The conversion uses GC storage directly and needs no canonical
linear-memory buffer for a scalar `char`.

`option<char>` uses the ordinary tagged-vector option representation and
`list<char>` uses a persistent vector of these strings. The canonical option
payload and list element are four-byte scalar values, independently of UTF-16
source indexing. The public source `char` coercion still has its separate
[Clojure overload contract](character-coercion.md).

## Indirect argument records

Above the canonical ABI's sixteen flattened argument fields, an exported
function receives a pointer to its argument record. Record offsets and size
come from the pinned `wit-parser::SizeAlign`, including option alignment and
padding. The adapter reads the fields into typed locals and uses the same
scalar, string, list and option conversions as a direct signature. After
copying all values, it releases nested transfer buffers and the argument record
before calling source code. Non-suspending canonical async exports forward the
same record to that adapter before reporting completion.

The focused regressions are `portable_aot_wit_char` and
`portable_aot_wit_indirect`. Their commands, raw observations and limitations
are retained under `results/m5-wit-adapters/`. Full generic imports, resources and async suspension have separate acceptance requirements;
these scalar and argument conversions do not establish those capabilities.

## Public interface type identities

An exported interface includes its named aliases of supported value types.
Primitive aliases receive a named type index; list and option aliases retain
one shared resolved definition. Function signatures reference those same types.
Versioned interface names and selected aliases keep their normal mapping paths
and implements annotations. A public type alias does not enable an unsupported
underlying boundary shape.

## Recursive data values

Tuple fields are native vector elements; record fields are map entries with
keyword keys. General options and results use `[:none]`, `[:some value]`,
`[:ok]`/`[:ok value]` and `[:err]`/`[:err value]`, preserving payloadless cases
and false values. These shapes compose recursively, including lists of records
and nested options. Source functions can retain incoming values across calls
and GC because their storage is owned by the guest.

The generated result projector checks complete product shapes, record keys,
case tags, payload arity and scalar leaves before allocating canonical output
buffers. Tuple/record fields and mixed variant payloads use the pinned upstream
layout and bit-preserving flat joins. Argument records and non-suspending async
results above the flat-field limits use indirect pointers. Empty tuples and
records are rejected before source macros because the pinned component
validator does not admit zero-field canonical products.

The executing controls are in `portable_aot_wit_composites`. They do not certify
arbitrary imported callbacks, resource lifetimes or cancellation cleanup.

## Nominal values, maps and fixed-length lists

Enums are ordinary keywords. Flags are persistent keyword sets, so `conj`,
`disj`, `contains?`, equality and the other set functions apply. General variants
use `[:case]` or `[:case payload]` with the declared case keyword. Unknown enum
values, flag names, variant tags or payload arities are rejected.

WIT maps are persistent maps. The adapter uses sequential `assoc` for incoming
entries, so duplicate keys keep the last value. Returned map iteration order is
unspecified. Fixed-length lists are persistent vectors with the declared length
checked at the boundary. The pinned canonical ABI admits one through32 flags
and nonempty fixed-length lists; unsupported declarations fail before source
macros. Hosts must enable their pinned engine's map and fixed-length-list
features; the shipped CLI does so.

Numeric lists also accept their documented packed-array element kinds inside
records, tuples, lists and variants. Copies preserve bits: for example, a byte
array containing `-1` leaves a `list<u8>` boundary as255. Arrays of another
primitive kind and user object arrays are rejected. Output projection and leaf
validation precede canonical output allocation.

Root function signatures publish nominal value types before defining their
parents and functions, using the newly exported type identities. Declared root
alias names are retained; a name collision appends `-value` until unique.
Nominal dependencies reached through root aliases also receive public names.
Interface exports retain their declared named types and versioned instance
names. These types add no implicit host capabilities.

The executing controls are `portable_aot_wit_value_shapes`, including
malformed-result checks, core redefinitions and10,000 nested calls constrained
to one64KiB transfer-memory page. Arbitrary imported functions, resource scopes,
future/stream scheduling and error-context lifetimes require their separate
adapters and ownership rules.
