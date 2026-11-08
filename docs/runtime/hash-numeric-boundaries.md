# Numeric boundaries for retained public hashing (retired)

This page described the binary64 adapters (`f64-finite`, `f64-safe-integer`,
`f64-floor` and `safe-integer-remainder`) that `hash` used for numbers before
numbers followed Clojure. `hash` now uses Clojure's `hasheq`; see
[numeric equality and hashing](numeric-hashing.md) (#336).
