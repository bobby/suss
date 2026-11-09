# Packed primitive arrays

Clojure's primitive arrays are packed WasmGC arrays
([ADR-0003](../adr/0003-clojure-numeric-tower.md) "Types", #336).

| Constructor | Storage | `aget` returns | `aset` converts with |
| --- | --- | --- | --- |
| `byte-array` | `i8` | long | `byte` |
| `short-array` | `i16` | long | `short` |
| `int-array` | `i32` | long | `int` |
| `float-array` | `f32` | double | `unchecked-float` |
| `long-array` | `i64` | long | `long` |
| `double-array` | `f64` | double | `double` |

Elements are signed, as in Clojure. A packed array is a `PACKED` struct
holding its storage and an identity UID ([ABI v3](abi-v3.md)), so `hash`,
`identical?` and printing treat it as an object with an identity.

## Constructors

Each takes `size-or-seq`, or `size` and `init-val-or-seq`, as in Clojure:

- A number makes a zeroed array of that many elements. The size is `(int n)`,
  so 2.5 makes two elements and 3e10 throws "Value out of range for int:
  3.0E10". A negative size throws its value, "-1", as Java's
  `NegativeArraySizeException` does. Sizes beyond 1,000,000 throw, the same
  resource bound as JS arrays ([arrays](arrays.md)).
- A sequence makes an array of its elements; nil makes an empty one.
- With a size, a number fills every element, and a sequence fills elements
  until it or the array ends; the rest stay zero.

Elements from a sequence or a fill value are stored with the unchecked cast, as
Java's `Number.byteValue` and the like convert them: `(byte-array [200])` holds
-56, `(int-array [3e10])` holds 2147483647 and `(float-array [1e300])` holds
`##Inf`. A non-number throws, for example "unchecked-long requires a number
operand".

`int-array` is the patched ClojureScript declaration
(`dependency-3840-int-array-patch.json`); the others are in core's appendix
(`exact-integers.sus`). They call the private `suss.bootstrap/packed-array`,
the runtime's `packed-array-new`, and `suss.bootstrap/packed-store`, its
`packed-array-store`.

## Access

- `aget`'s index is `(int i)`, as Clojure's inline `aget` casts it, so 1.5
  reads element 1. An index outside the array throws "Index 5 out of bounds
  for length 3", as Java does.
- `aset` converts its value with the cast in the table and returns the stored
  value. A narrow integer array checks the range: `(aset (byte-array 1) 0 200)`
  throws "Value out of range for byte: 200", and an `int-array` throws "integer
  overflow" for a long beyond 32 bits, as `int` does. A `float-array` rounds, so
  1e300 stores `##Inf`.
- `alength`, `aclone` and the `length` property work as for JS arrays. Packed
  arrays have no other properties; setting one throws.

`array?` is true of packed arrays, so core's `seq`, `count`, `nth`, `get`,
`contains?`, `reduce` and `into` read them as ClojureScript reads arrays. `vec`
copies one, as Clojure's `vec` of a primitive array does, where it aliases a JS
array.

## Types, strings and printing

`type` gives one value per element type. It prints as `#object[[J]` and its
name, for `class-name` and the printer, is the JVM's array class name: `[B`,
`[S`, `[I`, `[F`, `[J` or `[D`. Calling it throws. `str` is Java's
`Object.toString` of an array, and `pr` is Clojure's `#object` form for one,
whose class name prints as a string:

```clojure
(str (long-array 1))     ;=> "[J@1"
(pr-str (long-array 1))  ;=> "#object[\"[J\" 0x1 \"[J@1\"]"
```

As elsewhere, the hash is Suss's identity hash, not the JVM's
`identityHashCode` ([printing](printing.md)).

## Differences from Clojure

- `aget` returns a long or a double, never a boxed `Byte`, `Short`, `Integer`
  or `Float`, and a `float-array` element is a double rounded to binary32, so
  it prints as `0.10000000149011612` where Clojure prints `0.1`.
- `aset` converts with the element type's cast. In Clojure,
  `(aset (byte-array 1) 0 1)` and `(aset (long-array 1) 0 1.5)` fail
  reflection, as there is no `aset` for a long and a byte array; Clojure code
  uses `aset-byte` or a cast.
- The `aset-int`-style setters, `amap`, `areduce`, `char-array`,
  `boolean-array` and the `bytes`-style casts are not defined yet.
