# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
cargo build                    # Build all crates
cargo test                     # Run all tests
cargo test -p suss-reader      # Test specific crate
cargo run -p suss-cli          # Start REPL
cargo run -p suss-cli -- -e "(+ 1 2)"  # Evaluate expression
cargo run -p suss-cli -- script.suss   # Run file
```

### Static Compilation (Suss → WASM Components)

```bash
# Project-based compilation (reads deps.suss)
cargo run -p suss-cli -- compile                    # Compile all worlds
cargo run -p suss-cli -- compile --world :app/v1    # Compile specific world
cargo run -p suss-cli -- compile -c path/deps.suss  # Custom config

# Single-file compilation
cargo run -p suss-cli -- compile src.suss -w world.wit -o out.wasm

# Run compiled components with WASI support
cargo run -p suss-cli -- run out.wasm --invoke add 3 5
```

### WASM Component Builds

```bash
# Build individual components
cargo build -p suss-reader --target wasm32-wasip2 --features component --release
cargo build -p suss-eval --target wasm32-wasip2 --features component --release
cargo build -p suss-cli --target wasm32-wasip2 --features component --release

# Compose into single component
wac compose \
  --dep suss:reader=target/wasm32-wasip2/release/suss_reader.wasm \
  --dep suss:eval=target/wasm32-wasip2/release/suss_eval.wasm \
  --dep suss:cli=target/wasm32-wasip2/release/suss.wasm \
  -o target/wasm32-wasip2/release/suss_composed.wasm \
  compose.wac

# Run composed WASI component
wasmtime run target/wasm32-wasip2/release/suss_composed.wasm
```

## Architecture

Suss is a Clojure dialect targeting WASM/WASI, which follows the
ClojureScript implementation closely. A local reference copy of the
ClojureScript core library is found in `reference/cljs.core.clj`.

All execution goes through WASM compilation:

```
suss-core (Edn, Number, Symbol, Keyword)
    ↓
suss-reader (chumsky parser → EDN AST)
    ↓
suss-compile (analyze → IR → wasm-encoder → WASM Component)
    ↓
suss-cli (compile command, REPL via wasmtime)
```

### Crate Responsibilities

- **suss-core**: `Edn` enum, `Number` (BigInt/Ratio/Float), `Symbol`, `Keyword`
- **suss-reader**: Chumsky-based parser with EDN + reader conditionals (`#?(:suss ... :default ...)`)
- **suss-compile**: Static compiler with IR, semantic analysis, WASM Component output via wit-component
- **suss-cli**: CLI parsing (lexopt), REPL (rustyline + wasmtime), compile command

### WIT Component Model

WIT interfaces in `wit/` define component boundaries:
- `types.wit` - Core types (sexp resource, number, errors)
- `reader.wit` - Parser interface, symbol interning
- `evaluator.wit` - Eval functions, environment management
- `world.wit` - Main REPL world combining all interfaces

Components use `wit-bindgen::generate!()` with the `component` feature flag.

## Key Patterns

### Symbol/Keyword Interning
All symbols and keywords go through `Interner` for O(1) equality. Use `SymbolId`/`KeywordId` instead of strings in hot paths.

### Numeric Tower
`Number` enum: `Integer(BigInt)` → `Ratio(BigRational)` → `Float(f64)`. Supports Clojure radix literals (`2r1010`, `16rFF`).

### Static Compiler Subset
Compilable: `def`, `defn`, `fn`, `let`, `if`, `do`, `loop/recur`, numbers (i32/i64/f64), strings, vectors.
Not compilable: `eval`, macros, BigInt (use i64).

### WIT Exports
Mark functions for export with `^:export` metadata:
```clojure
(defn ^:export add [a b] (+ a b))
```

### WIT Imports
Use `require` with `:as` alias or `:refer`:
```clojure
(require '[wasi:random/random :as random])
(random/get-random-u64)
```

### Bundled WASI
WASI 0.2.4 WIT files are bundled. When world.wit imports `wasi:*`, they're auto-loaded. No deps/ folder needed for WASI packages.

### Project Configuration (deps.suss)

Multi-world projects use `deps.suss` (EDN format like Clojure's deps.edn):

```clojure
{:worlds
 {:my-app/v1 {:wit "wit/v1.wit"
              :output "target/v1.wasm"}
  :my-app/v2 {:wit "wit/v2.wit"
              :output "target/v2.wasm"}}
 :src-paths ["src"]}
```

Source files declare their target world with `gen-world` in the namespace:

```clojure
(ns my-app.core
  (gen-world :my-app/v1))

(defn ^:export add [a b] (+ a b))
(defn helper [x] (* x 2))  ; Internal, not exported
```

### Key Types

- `SussConfig` - Loaded from deps.suss, contains worlds and src-paths
- `WorldConfig` - WIT path and output path for a world
- `AnalyzedModule` - Parsed source with namespace, world_target, functions
