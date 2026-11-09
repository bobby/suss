# WIT numbers

WIT's numeric types cross the component boundary as Suss longs and doubles
([ADR-0003](../adr/0003-clojure-numeric-tower.md) "WIT boundary", #336). The
generated AOT adapter (`portable/aot.rs`, `aot/lists.rs`) converts them itself,
so redefining core functions such as `number?` or `identical?` changes neither
what crosses nor what is rejected.

| WIT type | Entering Suss | Leaving Suss |
| --- | --- | --- |
| `s8`–`s64`, `u8`–`u32` | A long | A long within the type's range |
| `u64` | A long; above 2^63 − 1 throws, until BigInt (#338) | A long from 0 to 2^63 − 1 |
| `f32`, `f64` | A double | Any number, as a double; `f32` rounds |

- An integer result must be a long: a double, even `1.0`, is rejected rather
  than truncated, as is a long outside the type's range. The error is "WIT
  export *name* returned an incompatible scalar value".
- A `u64` argument above 2^63 − 1 throws "WIT export *name* received a u64
  above 9223372036854775807, which needs unsupported BigInt", in a list or an
  option too.
- A float result may be a long, converted as `double` converts it, so
  `(defn f [] 7)` returns 7.0 as an `f64`.
- Options and lists of these types follow the same rules for each element.

## Lists

`list<T>` of a numeric `T` enters Suss as a persistent vector. Leaving Suss,
it accepts a vector or the matching packed array
([packed arrays](packed-arrays.md)), whose storage is copied without boxing:

| WIT list | Packed array | Elements |
| --- | --- | --- |
| `list<s8>`, `list<s16>`, `list<s32>`, `list<s64>` | `byte-array`, `short-array`, `int-array`, `long-array` | As stored |
| `list<u8>` | `byte-array` | Each byte's bits, so -1 sends 255, as Java's byte I/O does |
| `list<f32>`, `list<f64>` | `float-array`, `double-array` | As stored |

A packed array of another element type throws "WIT export *name* returned a
primitive array of another element type"; `list<u16>`, `list<u32>` and
`list<u64>` take vectors only.

## History

Before #336, numbers were binary64 doubles and `s64`/`u64` crossed as the
`WitSigned64` and `WitUnsigned64` wrappers, built with `wit-s64` and `wit-u64`
from decimal strings and read back with `wit-to-number`. They are removed;
an `s64` is now an ordinary long.
