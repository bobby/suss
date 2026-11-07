# Shared GC runtime ABI v3

ABI v3 (#335, [ADR-0003](../adr/0003-clojure-numeric-tower.md)) frees i31 for
Clojure longs. `runtime_abi::VERSION` is 3, and artifacts built for ABI v2 are
rejected before initialization, as [ABI v1](abi-v1.md) artifacts were.

## Sentinels

| Value | Representation |
| --- | --- |
| nil | A null `eqref` (`ref.null eq`) |
| false, true, undefined | Singleton `SENTINEL` structs, `struct { i32 }`, holding the tag 2, 4 or 6 |

- `Value` was already a nullable `eqref`, so nil needs no new type. Default-initialized
  arrays and structs, and unassigned locals, therefore hold nil.
- The runtime defines false, true and undefined as immutable globals right after
  the numeric stack global, before any global initializer that uses them, and
  exports them as `sentinel-false`, `sentinel-true` and `sentinel-undefined`.
- Compiled fragments import the three globals after their binding cells. AOT
  modules import them after their binding cells and before their own schema
  roots. `runtime_abi::sentinel_in` and `boolean_in` build them for a module
  whose sentinel globals start at a given index.
- Truthiness tests `ref.is_null` and `ref.eq` against the false and undefined
  globals. A boolean result is an `if` that selects the true or false global.
- On the host, `portable_session::sentinel_tag` decodes a value: `Some(0)` for nil (a
  null reference), the tag of a `SENTINEL` struct, or `None`. `SessionValue`
  holds nil as `None`.

## Types

The shared recursive group has twelve types: the ten of ABI v2, then `SENTINEL`
(index 10) and `LONG` (index 11, `struct { i64 }`), which holds longs outside
i31's range once longs land. Fragments append their function types after
`TYPE_COUNT` (12).

## Private i31 state

Runtime state that is never a language value still uses i31 integers: identity
UIDs before assignment (i31 0), async future status, continuation program
counters, generations and event kinds, stream phases and counts, property
descriptor flag words, error presence masks, and closure-table slot keys
(including the `property-native-name` kinds). None of these reach user code.
