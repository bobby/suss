# Protocol reload reference

Pinned ClojureScript c4295f303100bbf5afac449242d30bca1126f1a1;
original development-only fixture tests/oracle/src/suss_oracle/protocol_reload.cljs.
Fresh compile session69719 exited0; Node exited0 with [false,true,false,17,17].
The preceding49648 compile failed because the generated fixture was placed outside
the classpath; its diagnostic is retained externally, not counted as a pass.

Observations in order: fresh public protocol is not identical to saved value;
current protocol symbol implements? holds for retained instance; saved ordinary
var symbol is not recognized by implements?; current method and saved method both
return17. The implements? macro resolves protocol symbols at compile time. This
fixture does not establish dynamic protocol-value argument semantics in Suss.

Reproduce from tests/oracle with the pinned local/root checkout:

```sh
JAVA_TOOL_OPTIONS=-Xmx512m CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/protocol-reload.js" :output-dir "out/protocol-reload-cljs" :optimizations :none :source-map false :cache-analysis false}' -c suss-oracle.protocol-reload
node out/protocol-reload.js
```

The native sorted-interface regression checks fresh public protocol identity,
current implementation, aliases and current/retained method dispatch separately.
It must execute in both phases after GC; reference results alone are not native proof.
