# Array push and property reference evidence

This original eight-case fixture was compiled and executed against ClojureScript
c4295f303100bbf5afac449242d30bca1126f1a1. All eight ordered Boolean observations
match the corpus. `files.json` retains corpus, original fixture, output and
compile-log SHA256 hashes. The Java/Node runner is a development oracle only.

From `tests/oracle`, with the clean pinned checkout at the specified path:

```sh
clojure -Sdeps '{:deps {org.clojure/clojurescript {:local/root "/private/tmp/suss-m4-queues/clojurescript"}}}' -M -m cljs.main -co '{:target :nodejs :output-to "out/array-push-properties/main.js" :output-dir "out/array-push-properties/compiled" :optimizations :none}' -c suss-oracle.array-push-properties
node out/array-push-properties/main.js
```

Native `portable_array_constructor` executes the same eight source forms in both
Runtime/Macro phases and independently decodes Boolean ABI values after GC. Its
four tests pass, including the existing 22 constructor cases and separate
fragment constructor/method identity checks. Direct runtime tests independently
check retained payloads for three-value overflow pushes starting at uint32 max
and max-minus-one. These are bounded Array prerequisites for collection work;
prototype/accessor behavior and complete object-to-property-key coercion remain
unimplemented. They do not establish full M4 acceptance.
