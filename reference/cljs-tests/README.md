# ClojureScript Conformance Tests

Test cases adapted from [ClojureScript](https://github.com/clojure/clojurescript) for verifying Suss implementation correctness.

## License

The original ClojureScript source code is:

Copyright (c) Rich Hickey. All rights reserved.

The use and distribution terms for this software are covered by the
Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
which can be found in the file epl-v10.html at the root of this distribution.

## Source Files

Test cases were extracted and adapted from:
- `src/test/cljs/cljs/collections_test.cljs` - Collection operations
- `src/test/cljs/cljs/core_test.cljs` - Core functions
- `src/test/cljs/cljs/seqs_test.cljs` - Sequence operations
- `benchmark/cljs/benchmark_runner.cljs` - Performance benchmarks

## Test Format

Tests are stored as EDN vectors of maps:

```clojure
[{:name "vector-conj"
  :category :collections
  :expr "(conj [1 2] 3)"
  :expected [1 2 3]}
 ...]
```

Fields:
- `:name` - Unique test identifier
- `:category` - Test grouping (`:collections`, `:core`, `:seqs`, etc.)
- `:expr` - Suss expression to evaluate
- `:expected` - Expected result value
- `:skip` - Optional Boolean; true is rejected. Record reviewed failures in `docs/compatibility/known-failures.json` instead.

## Running Tests

```bash
# Run all conformance tests
cargo test -p suss-compile --test conformance -- --nocapture

# Run specific conformance category
cargo test -p suss-compile --test conformance test_conformance_collections -- --nocapture
cargo test -p suss-compile --test conformance test_conformance_core -- --nocapture

# Run performance benchmarks
cargo bench -p suss-compile --bench performance

# Run specific benchmark group
cargo bench -p suss-compile --bench performance -- compilation
cargo bench -p suss-compile --bench performance -- execution
cargo bench -p suss-compile --bench performance -- wasm_size

# Quick benchmark test (verify benchmarks compile/run)
cargo bench -p suss-compile --bench performance -- --test
```

## Adaptation Notes

Some tests were modified from the ClojureScript originals:
- JavaScript interop tests removed (not applicable to WASM target)
- `#js` literals converted to native Suss collections
- Tests requiring `deftype`/`defrecord` marked as skip until implemented
- Lazy sequence tests marked as skip until implemented

The adaptation notes above describe the legacy curation history, not the current
skip policy or complete upstream coverage. The reviewed 201-case corpus has no
skipped cases. Unknown/malformed fields, missing expectations/files and duplicate
IDs fail; `docs/compatibility/cases.json` protects every reviewed input and
expectation. Full pinned upstream differential coverage remains roadmap work.
