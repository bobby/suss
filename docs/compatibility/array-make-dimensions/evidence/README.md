# Array dimension reference

The nine original forms cover dynamic invalid numeric outer dimensions, invalid
leaves, zero outer dimensions suppressing unreachable invalid dimensions,
nonnumeric singleton dimensions, and literal macro negative/fractional outer
allocation. Fresh pinned compilation and Node execution produce nine true
Boolean observations, in order. Native execution checks the same nine forms in
both phases after GC. SHA256 receipts identify fixtures, corpus and outputs.

Historical seven-case output retains two false results: the original test
incorrectly expected literal negative/fractional outer sizes to throw. The pin's
macro makes an empty/ceil-sized outer array instead. Dynamic inputs throw
RangeError; the corrected nine-case corpus separates those behaviors. Historical
false results are not rewritten or presented as successes.

```sh
# From tests/oracle; pinned checkout path may be replaced by an equivalent clean checkout.
clojure -Sdeps '{:deps {org.clojure/clojurescript {:local/root "/private/tmp/suss-m4-queues/clojurescript"}}}' -M -m cljs.main -co '{:target :nodejs :output-to "out/array-make-dimensions-nine/main.js" :output-dir "out/array-make-dimensions-nine/compiled" :optimizations :none}' -c suss-oracle.array-make-dimensions
node out/array-make-dimensions-nine/main.js
```

Runtime sparse leaves do not allocate logical-length-sized buffers. The resource
check budgets dimension containers, rather than logical leaf cells; two 2000
sizes are valid, while three exceed the existing container budget. Nonleaf
insertion still has quadratic cost; full object coercion/prototype contracts and
M4 acceptance remain incomplete. These cases verify messages; direct runtime
constructor tests separately verify canonical RangeError descriptor identity.
