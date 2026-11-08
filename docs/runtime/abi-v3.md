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
(index 10) and `LONG` (index 11, `struct { i64 }`). Fragments append their
function types after `TYPE_COUNT` (12).

## Longs and doubles

| Value | Representation |
| --- | --- |
| A long from −2³⁰ to 2³⁰ − 1 | An `i31ref` |
| Any other long | A `LONG` struct holding the `i64` |
| A double | A `NUMBER` struct (index 0) holding the `f64` |

- Each long has exactly one representation: `long-box` makes an i31 whenever the
  value fits, so `identical?` and `ref.eq` see one small long as one value.
  `long-test`, `double-test` and `number-test` test the kinds, `long-unbox`
  reads a long's `i64`, and `number-unbox` reads any number as an `f64`.
  `runtime_abi::number_test`, `number_to_f64` and `long_box_in` are the inline
  forms for modules that can't call the runtime, such as AOT adapters.
- Integer literals in the long range compile to longs: `i32.const` and
  `ref.i31` when they fit, else `i64.const` and a `LONG`. Other decimal
  literals are doubles. Counts, indexes, hashes, character codes and the other
  integral results of the runtime are longs.
- `number-add`, `-subtract`, `-multiply` and `-negate` on two longs give a long
  or throw "long overflow"; with a double operand they give a double. Two i31
  operands take an inline path in the checked and coercing wrappers.
  `number-divide` of longs that divide evenly is a long, other quotients of
  longs are doubles until ratios (#338), and a long divided by zero throws
  "Divide by zero". A `Long/MIN_VALUE` operand throws instead, since Clojure's
  quotient is then a BigInt or ratio (#338). `number-quot`, `-rem`, `-abs`,
  `-unchecked-*`, `-long?`, `-double?`, `-to-long` and `-range-count` back the
  private `suss.bootstrap` primitives of core's `quot`, `rem`, `mod`, `abs`,
  `unchecked-*`, `integer?`, `double?`, count narrowing (which throws "Value
  out of range for long", as Clojure's `longCast`) and integer range counts
  (exact on the unsigned distance). Double `quot` and `rem` truncate the
  quotient through a long, as Clojure does, and throw "Infinite or NaN" when
  it isn't finite.
- Comparisons of two longs are exact, beyond binary64's 2⁵³, and so is core's
  strict equality (`===`), which also equates a long with an equal double.
  `identical?` keeps them distinct, and so does `case` (`comparison-case-equal`),
  which matches a number only within its kind and a double only by its bits.
- `long-string` prints a long; `coerce-string` prints a double in Java's
  `Double.toString` format, through the numeric helper's ryu-js digits.
  `number-parse-long` reads core's integer literals exactly.
- A JS property key that is an integral double in the long range names the same
  property as the equal long (`property-key`), as JS's ToString would.
- On the host, `portable_session::runtime_number` decodes a number as
  `RuntimeNumber::Long` or `RuntimeNumber::Double`. Macro data and EDN carry
  `Kind::Long` and `Value::Long` beside doubles.
- AOT exports lift WIT integers up to `u32` as longs. `s64` and `u64` still
  travel as the `WitSigned64` and `WitUnsigned64` wrappers, whose words and
  schema operations are now longs; direct WIT numbers are #336's.

## Collector

Every Suss engine, the REPL's and `suss run`'s, configures
`Collector::Copying` explicitly (`portable_session::configure_collector`), with
a 64 MiB initial GC heap per store. `SUSS_GC=null` selects the null collector
for short-lived runs, `SUSS_GC_INITIAL_MIB` sets the initial heap, and any
other `SUSS_GC` value is an error. [ADR-0003](../adr/0003-clojure-numeric-tower.md#collector)
records the choice and its measurements.

## Private i31 state

Runtime state that is never a language value still uses i31 integers: identity
UIDs before assignment (i31 0), async future status, continuation program
counters, generations and event kinds, stream phases and counts, property
descriptor flag words, error presence masks, and closure-table slot keys
(including the `property-native-name` kinds). None of these reach user code.
