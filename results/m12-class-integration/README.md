# Numeric class integration controls

The complete workspace partitions at `8a6e350c` found four integration failures:

- `printer_options` expected the previous opaque Number type instead of `java.lang.Long`.
- `core_state` expected that same opaque name in missing multimethod dispatch errors.
- `core_comparator_foundations` treated Long and Double as one callable JS Number constructor.
- `portable_sorted_tree_collections` reached an invalid nominal property access while formatting an unsupported numeric `nth` operand. Numeric Class descriptors have no JS constructor fields.

`before-ci.log.gz` is the byte-preserved failed CI log from run45066275293. The new `type->str` branch obtains the canonical numeric class name before constructor-property access; both `nth` arities use the fresh JVM colon/simple-name diagnostic. Non-numeric constructor adaptation is retained. The 132-case pinned CLJS sorted-tree corpus is unchanged; the scalar set callback error alone uses the JVM `nth` expectation, with complete error shape/message and callback trace still decoded independently. The extension itself has no JVM `.forEach` equivalent.

The fresh primary call control additionally showed that a Class callee is rejected before evaluating arguments. The reviewed old executable instead evaluated both argument effects. An internal, effectful call-lowering check now runs after evaluating the callee and before its arguments, retaining the original callee SSA identity. A fast physical descriptor test avoids canonical-class lookup for ordinary closures and callable collections. Explicit `apply` arguments retain their own evaluation order.

Run `python3 results/m12-class-integration/oracle.py` to derive controls from Clojure1.12.6. `oracle.json` retains the exact command, pinned jar hashes and stdout/stderr. `cases.json` feeds the existing Runtime/Macro/actual-AOT class matrix. `run-focused.py LABEL` executes the four failing targets plus the complete class matrix with two test threads and a bounded memory scope. Each receipt records source/fixture hashes and terminal results; unsuccessful intermediate attempts remain evidence.


Final local evidence:

- `after-known-closures`: terminal0,77 tests/10 targets, unchanged source and fixture. This includes strict record fuel and no-box/no-call closure codegen.
- `gates`: all eight commands terminal0,227 tests (4 printer,80 reader,51 numeric/analyzer CLI,88 compiler/ABI,4 bootstrap), with Java/Node-free double reproduction, compiler identity, both manifests and workspace test-target compilation. Source inputs unchanged.
- `suite-commands`: build, actual full capture and suite lock all terminal0, source unchanged. `suite-audit` retains4,247 raw rows and exactly3,739 host matches/61 guest judgments/459 failures/2,680 unexecuted/90 failed namespaces, with zero changed assertions or namespace diagnostics. No baseline/reference/skip/count-guard rewrite.
- `independent-final.json.gz`: fresh independent subprocess captures for all23 JVM controls and11 combined current-CLI probes; exact commands, output, exits, executable hash and production/test/fixture hash checks. The one nonmatrix multimethod control is explicitly mapped to its existing macro-aware test. The earlier temporary harness failure is retained in after-call-check.
- `final-input-diff.json`: only documentation differs after the gates/capture; production/test/fixture inputs match. Historical performance measurements elsewhere remain qualified to their recorded source, rather than becoming measurements of this call-guard change.
