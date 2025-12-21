# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Suss is an experimental Clojure dialect targeting WASM/WASI. Named after Jay and Julie Sussman (co-authors of SICP).

## Build Commands

```bash
cargo build                    # Build all crates (native)
cargo test                     # Run all tests
cargo run -p suss-cli          # Run the REPL
cargo component build          # Build for WASM (requires cargo-component)
```

## Project Structure

```
suss/
├── wit/                       # WIT interfaces (Component Model)
│   ├── types.wit              # Core Suss types (sexp, numbers)
│   ├── reader.wit             # Reader/parser interface
│   ├── evaluator.wit          # Evaluator interface
│   └── world.wit              # Main CLI world
└── crates/
    ├── suss-reader/           # S-expression parser (chumsky-based)
    │   └── src/
    │       ├── lib.rs         # Public API
    │       ├── parser.rs      # Chumsky parser
    │       ├── sexp.rs        # Sexp type and printer
    │       ├── number.rs      # BigInt/Ratio/Float numeric tower
    │       └── intern.rs      # Symbol/keyword interning
    ├── suss-eval/             # Tree-walking interpreter
    │   └── src/
    │       ├── lib.rs         # Runtime with interner + env
    │       ├── eval.rs        # Core evaluator + special forms
    │       ├── env.rs         # Scoped environment
    │       └── primitives.rs  # Built-in functions
    └── suss-cli/              # REPL executable
        └── src/main.rs        # Native + WASM REPL entry points
```

## Architecture

- **Parser**: chumsky-based with error recovery for REPL friendliness
- **Numeric tower**: Full BigInt + Ratio + Float (num-bigint, num-rational)
- **Interning**: Symbols and keywords are interned for O(1) equality
- **Evaluator**: Direct tree-walking interpreter (compilation to WASM planned)
- **Component Model**: WIT interfaces defined for eventual WASM componentization

## Design Decisions

| Decision | Choice |
|----------|--------|
| Parser | chumsky (pure Rust, great error recovery) |
| Numerics | Full tower: BigInt + Ratio + f64 |
| TCO | Will use native WASM tail calls (baseline since Dec 2024) |
| Memory | Custom GC initially, WASM 3.0 GC backend later |
| Reader conditionals | `#?(:suss ... :default ...)` |

## Current Status

Phase 1 MVP in progress:
- [x] S-expression reader with EDN support
- [x] Basic REPL with rustyline
- [x] Minimal evaluator (quote, if, do, def, let, fn placeholders)
- [ ] Full function evaluation
- [ ] Numeric primitive operations wired up
- [ ] WASM component build integration
