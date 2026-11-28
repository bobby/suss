# Exact Decimal values in native tooling EDN

The original reader rejects `edn::read_one("1.0M", &Options::default())` despite compiled Decimal support. `before/` retains an actual failing Rust control linked against the unchanged reader library, including its library/source hashes and actual exit 101. Its build succeeds; the failure is the unsupported-number diagnostic.

Native tooling now stores `Value::Decimal { unscaled: BigInt, scale: i32 }`. Native source conversion retains both fields and emits Clojure `M` literals. The printer preserves trailing zeros, uses the primary plain/scientific threshold, and handles both scale boundaries without enormous padding. Reader identities reject numerically equal Decimal keys across scales, including zero and the scale floor, while storage and structural equality preserve scale. Serde refuses lossy primitive Decimal conversion explicitly. The now-unused unsupported-number diagnostic is removed.

This Rust implementation is original code guided by behavioral controls, not a port of JVM implementation source. `primary/` preserves 126 fresh pinned Clojure 1.12.6 observations, raw output, source and dependency hashes. The cases cover coefficients, plain/scientific spelling, negative zero, signed 32-bit scale extremes, exponent admission and duplicate nested numeric keys. The native regression consumes those actual expected observations; it does not generate expectations from Suss.

The focused real-workspace command is:

```sh
cargo test -p suss-reader --locked --offline -- --test-threads=2
```

`reader-native-first/command.json.gz` records actual0 in 39.26 seconds with all hashed source/primary inputs and HEAD unchanged. It reuses the inactive isolated reader build target, independently of the live eval workspace target. This is reader validation within the real workspace dependency graph, not a full compiler/runtime or numeric suite acceptance claim. No dependency identities, primary oracle, suite bodies, accepted failures or skip rules change.

Integration must still validate analyzer/source emission and actual Runtime/Macro/AOT behavior, execute the complete original numeric source bodies, complete the exact workspace baseline, and pass independent promotion review and final-head CI. The eq/not-eq/num namespace regex dependency remains concrete work; the 229/200 tagged-operand subsets alone do not establish completion of the numeric issues.
