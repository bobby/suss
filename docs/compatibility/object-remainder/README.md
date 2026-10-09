# Object remainder regression reference

Fresh ClojureScript `c4295f303100bbf5afac449242d30bca1126f1a1` compilation and Node execution confirm three ordered original cases: default ordinary-object remainder is NaN; both operand expressions execute before left conversion throws the original 73 payload and prevents right conversion (trace123); subsequent scalar remainder is1. Retained observations are independently prewritten boolean checks, alltrue. This is three-case reference evidence, not complete numeric/object compatibility.

From `tests/oracle`:
```sh
clojure -Sdeps '{:deps {org.clojure/clojurescript {:local/root "/private/tmp/suss-m4-queues/clojurescript"}}}' -M -m cljs.main -co '{:target :nodejs :output-to "out/object-remainder/main.js" :output-dir "out/object-remainder/compiled" :optimizations :none}' -c suss-oracle.object-remainder
node out/object-remainder/main.js
```

The native macro regression replaces an obsolete unsupported-js-obj expectation with independent boxed-Number decoding after GC and NaN classification. It retains original operand/arity assertions, adds exact thrown-payload/ordered-effect assertions and checks recovery result bits. Native execution is pending until the existing full baseline releases the sole local Cargo lane. No runtime source or test allowance changes.
