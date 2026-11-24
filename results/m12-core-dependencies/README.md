`results/m12-core-dependencies/oracle.py` records396 fresh JVM Clojure1.12.6
controls for min-key and some-fn. Full upstream forms, EPL notice, source/JAR
hashes and def/fn adaptation patches are retained. Key functions execute once;
min-key uses < for the first pair and <= thereafter, preserving ties/NaN. The
some-fn controls cover1..5 predicates,0..6 arguments, every hit position,
short-circuiting, returned truthy values and exact predicate/argument traces.
`run-prototype-repl.py` executes396/396 matching rows through the actual CLI,
with each request at an explicit ordinary10M compilation allowance, executable
hash unchanged. These are local definitions, not a production-core, Macro or
AOT pass. Six pure map-indexed fixture constructors are materialized because
that helper is unported; separate fresh JVM execution verifies all396 values
identical to the untouched primary controls. No function under test changes.
Earlier prototype failures (missing println, batch analysis fuel, unavailable
map-indexed and an unsupported -e compilation option) remain terminal receipts
with zero observed controls, never passes. No production budget is changed.

