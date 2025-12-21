# The Suss Language

Suss is a Clojure dialect targeting WASM/WASI. It is named in honor of
Jay and Julie Sussman, who along with Hal Abelson wrote the classic
*Structure and Interpretation of Computer Programs*.

## Suss is (Currently) Experimental

Don't use it for anything real yet. Eventually our goal is for Suss to
become a first-class compile-to WASM language like Grain or Moonbit,
capable of hosting complete applications and running anywhere WASM can run:
browsers, servers, infrastructure.

## Getting Started

### Prerequisites

- Rust (edition 2024)
- wasmtime (for running WASM builds)
- wac (for component composition): `cargo install wac-cli`

### CLI Usage

```bash
# Start the REPL (default)
cargo run -p suss-cli

# Evaluate an expression
cargo run -p suss-cli -- -e "(+ 1 2 3)"

# Run a file
cargo run -p suss-cli -- script.suss

# Show help
cargo run -p suss-cli -- --help
```

Example REPL session:

```clojure
Suss v0.1.0 - A Clojure dialect for WASM
Type (help) for help, Ctrl-C to exit

suss> (+ 1 2 3)
6
suss> (def add1 (fn [x] (+ x 1)))
#<function>
suss> (add1 5)
6
suss> ((fn [x y] (+ x y)) 3 4)
7
suss> (let [a 10 b 20] (+ a b))
30
```

### Running Tests

```bash
cargo test
```

### Building WASM Components

Suss can be built as WASM components for WASI Preview 1 or WASI 0.2.

#### WASI Preview 1 (single module)

```bash
cargo build -p suss-cli --target wasm32-wasip1 --release
wasmtime run target/wasm32-wasip1/release/suss.wasm -e "(+ 1 2)"
```

#### WASI 0.2 (Component Model)

Build individual components:

```bash
cargo build -p suss-reader --target wasm32-wasip2 --features component --release
cargo build -p suss-eval --target wasm32-wasip2 --features component --release
cargo build -p suss-cli --target wasm32-wasip2 --features component --release
```

Compose into a single component:

```bash
wac compose \
  --dep suss:reader=target/wasm32-wasip2/release/suss_reader.wasm \
  --dep suss:eval=target/wasm32-wasip2/release/suss_eval.wasm \
  --dep suss:cli=target/wasm32-wasip2/release/suss.wasm \
  -o target/wasm32-wasip2/release/suss_composed.wasm \
  compose.wac
```

Run the composed component:

```bash
wasmtime run target/wasm32-wasip2/release/suss_composed.wasm -e "(+ 10 20)"
wasmtime run --dir=. target/wasm32-wasip2/release/suss_composed.wasm script.suss
```

### Static Compilation (Suss → WASM)

Suss can compile source code directly to standalone WASM components that implement user-specified WIT worlds:

```bash
# Compile a Suss file to WASM
cargo run -p suss-cli -- compile src.suss -w world.wit -o out.wasm

# Run the compiled component
wasmtime run --invoke add out.wasm 3 5
```

Example source (`add.suss`):

```clojure
(defn ^:export add [a b]
  (+ a b))
```

Example WIT world (`world.wit`):

```wit
package example:math;

world calculator {
    export add: func(a: s32, b: s32) -> s32;
}
```

The `^:export` metadata marks functions for export in the WIT world.

#### Compilable Subset

The static compiler supports a subset of Suss suitable for ahead-of-time compilation:

| Feature | Supported | Notes |
|---------|-----------|-------|
| `def` | Yes | Top-level constants |
| `defn` | Yes | Named functions (use `^:export` for WIT exports) |
| `fn` | Yes | Lambda expressions (no mutable capture) |
| `let`, `if`, `do` | Yes | Control flow |
| `loop/recur` | Yes | Maps to WASM loops |
| Numbers | Yes | i32, i64, f64 (no BigInt) |
| Strings | Yes | Linear memory |
| `eval`, macros | No | Requires interpreter |

## Architecture

Suss is built as composable WASM components:

```
┌─────────────────────────────────────────────┐
│              suss-cli                        │
│         imports: reader, evaluator           │
└─────────────────────────────────────────────┘
          │                    │
    ┌─────┘                    └─────┐
    ▼                                ▼
┌───────────────┐          ┌───────────────┐
│ suss-reader   │◄─────────│ suss-eval     │
│ exports:      │ imports  │ exports:      │
│   types       │          │   evaluator   │
│   reader      │          │               │
└───────────────┘          └───────────────┘
```

- **suss-core**: Core types (Sexp, Env, Interner)
- **suss-reader**: Clojure/EDN parser with WIT interface
- **suss-eval**: Tree-walking interpreter with WIT interface
- **suss-compile**: Static compiler (Suss → WASM)
- **suss-cli**: Command-line interface

## Suss is Clojure

* Can (optionally) run a REPL
* Supports EDN data literals
* Immutable data structures
* Standard library (follows ClojureScript's implementation)

Obviously, the primary compilation target will be static as a WASM/WASI application, so some of the more dynamic features of Clojure may need to be curtailed or made optional at compile-time.

## Suss targets WASM/WASI

Suss targets native compilation (no runtime/GC needed) to WASI 0.2 and 0.3. It's main interfaces to other programs (initially just the REPL) are exposed as WIT interfaces.

## Thoughts/notes

* WASM (via WAT) is already sort of lisp-y, so maybe we can leverage this isomorphism?
