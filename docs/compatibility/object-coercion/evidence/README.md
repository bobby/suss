# Ordered ordinary object coercion reference

Eight original forms were freshly compiled with pinned ClojureScript
c4295f303100bbf5afac449242d30bca1126f1a1 and executed by Node. Every ordered
Boolean observation is true; source/output SHA256 hashes and compiler warnings
are retained. Native execution checks the same forms in Runtime/Macro phases,
independently decoding values after GC. This covers numeric/string hints,
left-before-right relational conversion, primitive string comparison and
addition, noncallable method skipping, object-result fallback and thrown payloads.

```sh
# From tests/oracle.
clojure -Sdeps '{:deps {org.clojure/clojurescript {:local/root "/private/tmp/suss-m4-queues/clojurescript"}}}' -M -m cljs.main -co '{:target :nodejs :output-to "out/object-coercion/main.js" :output-dir "out/object-coercion/compiled" :optimizations :none}' -c suss-oracle.object-coercion
node out/object-coercion/main.js
```

The original runtime uses immutable typed function roots to resolve the
conversion/storage dependency cycle. Shared recursive ABI v2 types are unchanged.
Second conversion-method lookup occurs after the first call. Complete Array join,
function/error prototypes, exotic conversion hooks, canonical TypeError,
physical-this behavior and broader getter/mutation coverage remain incomplete.
This evidence does not establish full object coercion or M4 acceptance.
