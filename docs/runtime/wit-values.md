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
compose with nested lists, options and results. The accepted mapping also
specifies persistent maps for WIT maps and keyword sets for flags; those shapes
still require their own generated adapters and executing acceptance evidence. A resource handle has identity, ownership and borrow
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
are retained under `results/m5-wit-adapters/`. Full generic imports, the remaining composite
adapters, resources and async suspension have separate acceptance requirements;
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
