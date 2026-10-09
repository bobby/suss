# Object-valued Array length conversion reference

Four original forms freshly execute on pinned ClojureScript and Node; complete
ordered Boolean observations match native execution in both phases after GC.
Source/output SHA256 receipts identify mismatch, second-conversion throw,
mutation before valid shrink, and first-conversion modulo wrapping followed by
a matching second value. These tests exercise effects and retained state.

[ArraySetLength](https://tc39.es/ecma262/multipage/ordinary-and-exotic-objects-behaviours.html#sec-arraysetlength)
requires separate ToUint32 and ToNumber conversions. The runtime reuses the
checked int32 bit pattern as unsigned uint32, then independently converts the
original RHS and compares numeric values before changing storage. Conversion
mutations and thrown payloads are preserved.

```sh
# From tests/oracle.
clojure -Sdeps '{:deps {org.clojure/clojurescript {:local/root "/private/tmp/suss-m4-queues/clojurescript"}}}' -M -m cljs.main -co '{:target :nodejs :output-to "out/object-length-conversion/main.js" :output-dir "out/object-length-conversion/compiled" :optimizations :none}' -c suss-oracle.object-length-conversion
node out/object-length-conversion/main.js
```

These development oracles do not introduce Java/Node into shipped code. Full
property descriptors, readonly/deletion effects, inherited indices and exotic
conversion behavior remain outside this proved subset, within the active goal.
