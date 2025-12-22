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

### Static Compilation (Suss → WASM)

```bash
cargo run -p suss-cli -- compile src.suss -w world.wit -o out.wasm
wasmtime run --invoke func_name out.wasm
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

Suss is a Clojure dialect targeting WASM/WASI with two execution paths:

### Interpreter Path (REPL/scripting)
```
suss-core (Sexp, Number, Interner, Env)
    ↓
suss-reader (chumsky parser → AST)
    ↓
suss-eval (tree-walking interpreter)
    ↓
suss-cli (REPL, file execution)
```

### Compiler Path (static WASM generation)
```
suss-core + suss-reader
    ↓
suss-compile (analyze → IR → wasm-encoder → .wasm)
    ↓
suss-cli (compile command)
```

### Crate Responsibilities

- **suss-core**: `Sexp` enum, `Number` (BigInt/Ratio/Float), `Interner` for symbols/keywords, `Env` for scopes
- **suss-reader**: Chumsky-based parser with EDN + reader conditionals (`#?(:suss ... :default ...)`)
- **suss-eval**: Tree-walking interpreter, special forms (quote, if, do, def, let, fn), primitives
- **suss-compile**: Static compiler with IR, semantic analysis, WASM codegen via wasm-encoder
- **suss-cli**: CLI parsing (lexopt), REPL (rustyline), command dispatch

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
