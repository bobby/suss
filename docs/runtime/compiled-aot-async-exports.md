# Compiled asynchronous component exports

Portable AOT accepts asynchronous freestanding WIT functions, including functions
inside exported interfaces, when their parameters and results use the currently
supported scalar types. Their ordinary source bodies run through the same compiled
source, macro and shared GC pipeline as synchronous exports.

For example, `app.sus`:

```clojure
(ns app)
(def ^:export add (fn [x] (+ x 1)))
```

With `api.wit`:

```wit
package example:library;
world api {
  export add: async func(x: f64) -> f64;
}
```

```sh
suss compile app.sus -w api.wit -o app.wasm
suss run app.wasm --invoke add -- 41
```

The invocation prints `42`. Interface functions require explicit mappings such as
`--export 'example:library/api#add=app/add'`; freestanding metadata shorthand follows
[the existing export policy](compiled-export-metadata.md).

The assembler retains each resolved function's asynchronous kind. A private core
bridge calls the checked scalar adapter, completes the canonical task through
`task.return`, and reports EXIT. It uses the canonical callback ABI without
stackful switching. Ordinary bodies complete without suspension; their callback
cannot receive a pending event. Indexed private entry names prevent a public
export named `callback` from colliding with the bridge callback. Live cells,
captured functions, once-only effects and language exceptions keep their ordinary
shared-runtime behavior. Canonical builtins introduce no hidden host imports.

The native runner checks the selected path and typed arguments before guest
initialization. It invokes asynchronous exports on a current-thread Tokio runtime,
and registers the pinned Wasmtime WASI p3 bindings alongside p2. Tokio1.53.1 was
already locked transitively; the CLI now declares its native dependency explicitly.
Registration is not evidence for the complete WASI capability graph: these tests
execute pure scalar component exports.

Evidence:

- Controlled parent assembler: both new execution tests fail on its explicit async
  rejection. Controlled parent runner, with fixed compiler/images: both new CLI
  tests fail on its explicit async invocation rejection.
- Existing compiler AOT13 and new async2 pass. Actual async/sync type flags, zero
  component imports, old captures, live effects, interfaces, bool/void/f64/f32,
  signed zero/infinity bits, public callback naming, GC and two Stores are checked.
  Boundary messages and source throw17 are independently decoded from exceptions.
- New native async2 and existing source/file/namespace/project/export metadata24
  pass. Macro source executes, typed calls print exact results, malformed inputs
  reject, and validation precedes a throwing guest initializer.
- Both phase Wasm/JSON artifacts were regenerated at the new compiler/lock identity
  and reproduced twice without Java; bootstrap4 passes. Python118 passes.

This is a prerequisite for native main and the official asynchronous command.
Imported WIT adapters, canonical argument strings/lists, rooted suspended
continuations, pending-I/O cancellation, component-host migration, evaluator
retirement and original M3 acceptance remain unfinished. Independent review,
unfiltered final-head baseline and final-head CI are required before readiness.
No core inventory declaration or milestone is reclassified.
