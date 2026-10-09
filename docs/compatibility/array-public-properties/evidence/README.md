# Public Array property reference

Eleven original cases execute against the pinned ClojureScript compiler and
Node, with complete ordered Boolean observations and source/output SHA256
receipts. The same corpus drives two native tests in both Runtime/Macro phases;
values are independently decoded after GC. They cover named length assignment,
string/boolean/nil length RHS, returned RHS identity, own method shadowing,
dynamic/first-class holes, literal nil presence, independent multidimensional
leaves, and maximum valid sparse length. Both native tests pass.

From `tests/oracle`, use the existing pinned deps override:

```sh
clojure -Sdeps '{:deps {org.clojure/clojurescript {:local/root "/private/tmp/suss-m4-queues/clojurescript"}}}' -M -m cljs.main -co '{:target :nodejs :output-to "out/array-public-properties/main.js" :output-dir "out/array-public-properties/compiled" :optimizations :none}' -c suss-oracle.array-public-properties
node out/array-public-properties/main.js
```

These development-only oracles do not add Java/Node dependencies to shipped code.
Complete object coercions, inherited properties and accessors remain unproven.
