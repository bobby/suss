# The Suss Language

Suss is a Clojure dialect targeting WASM/WASI. It is named in honor of
Jay and Julie Sussman, who along with Hal Abelson wrote the classic
*Structure and Interpretation of Computer Programs*.

## Suss is (Currently) Experimental

Don't use it for anything real yet. Eventually our goal is for Suss to
become a first-class compile-to WASM language like Grain or Moonbit,
capable of hosting complete applications and running anywhere WASM can run:
browsers, servers, infrastructure.

### Prior art

Others have looked into the relationship of Clojure and WASM:

* https://romanliutikov.com/blog/running-clojure-in-wasm
* https://github.com/kanaka/clj.wasm

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

Suss can be built as a WASM component for WASI 0.2.

#### WASI 0.2 (Component Model)

The Suss interpreter can run as a WASI 0.2 component, allowing it to be embedded in any WASM runtime that supports the component model.

**Step 1: Build individual components**

```bash
cargo build -p suss-reader --target wasm32-wasip2 --features component --release
cargo build -p suss-eval --target wasm32-wasip2 --features component --release
cargo build -p suss-cli --target wasm32-wasip2 --features component --release
```

**Step 2: Compose into a single component**

```bash
wac compose \
  --dep suss:reader=target/wasm32-wasip2/release/suss_reader.wasm \
  --dep suss:eval=target/wasm32-wasip2/release/suss_eval.wasm \
  --dep suss:cli=target/wasm32-wasip2/release/suss.wasm \
  -o target/wasm32-wasip2/release/suss_composed.wasm \
  compose.wac
```

**Step 3: Run the WASI 0.2 REPL**

```bash
# Start the REPL
wasmtime run target/wasm32-wasip2/release/suss_composed.wasm

# Evaluate an expression
wasmtime run target/wasm32-wasip2/release/suss_composed.wasm -e "(+ 10 20)"

# Run a script file (requires --dir for filesystem access)
wasmtime run --dir=. target/wasm32-wasip2/release/suss_composed.wasm -- script.suss
```

The composed component includes the reader, evaluator, and CLI - a fully self-contained Suss environment running as pure WASM.

### Static Compilation (Suss → WASM Components)

Suss compiles source code directly to standalone WASM components that implement user-specified WIT worlds.

#### Project-Based Compilation (Recommended)

For projects with multiple worlds or organized source trees, use `deps.suss`:

```bash
# Compile all worlds defined in deps.suss
cargo run -p suss-cli -- compile

# Compile a specific world
cargo run -p suss-cli -- compile --world :my-app/v1

# Use a custom config file
cargo run -p suss-cli -- compile -c path/to/deps.suss
```

**Project structure:**

```
myproject/
  deps.suss           # Project configuration
  wit/
    v1.wit            # WIT world definitions
    v2.wit
  src/
    core.suss         # Source files with (gen-world ...)
    utils.suss
  target/             # Compiled output
```

**deps.suss** (EDN format, like Clojure's deps.edn):

```clojure
{:worlds
 {:my-app/v1 {:wit "wit/v1.wit"
              :output "target/v1.wasm"}
  :my-app/v2 {:wit "wit/v2.wit"
              :output "target/v2.wasm"}}
 :src-paths ["src"]}
```

**Source file with namespace and world target:**

```clojure
(ns my-app.core
  (gen-world :my-app/v1))

(defn ^:export add [a b]
  (+ a b))

(defn helper [x]  ; Not exported (no ^:export)
  (* x 2))
```

#### Single-File Compilation

For quick one-off compilation without project setup:

```bash
# Compile a single file
cargo run -p suss-cli -- compile src.suss -w world.wit -o out.wasm

# Run the compiled component using suss run
cargo run -p suss-cli -- run out.wasm --invoke add 3 5
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

The `^:export` metadata marks functions for export in the WIT world. Functions without `^:export` are compiled but remain internal.

#### WIT Imports

Suss supports importing functions from WIT interfaces using Clojure-style `require`:

```clojure
;; Import with alias - access via alias/function-name
(require '[wasi:random/random :as random])

(defn ^:export get-random []
  (random/get-random-u64))

;; Import specific functions directly (no prefix needed)
(require '[wasi:cli/stdout :refer [print]])

(defn ^:export greet []
  (print "Hello from Suss!"))
```

The corresponding WIT world must declare the imports:

```wit
package myapp:example;

world app {
    import wasi:random/random@0.2.0;
    export get-random: func() -> u64;
}
```

#### Bundled WASI Support

WASI 0.2.4 WIT definitions are bundled with the compiler. When your world.wit imports `wasi:*` packages, they are automatically detected and loaded - no `deps/` folder needed:

```
myproject/
  world.wit       # Just import wasi:random/random - it works!
  app.suss
```

Supported bundled packages:
- `wasi:random` - Random number generation
- `wasi:cli` - stdin, stdout, stderr, environment
- `wasi:clocks` - Wall clock and monotonic clock
- `wasi:io` - Streams and polling
- `wasi:filesystem` - File and directory operations
- `wasi:sockets` - TCP/UDP networking

For custom (non-WASI) WIT packages, place them in a `deps/` folder next to your world file:

```
myproject/
  world.wit       # Your world definition
  app.suss        # Your Suss source
  deps/
    mypackage/    # Custom WIT packages
      types.wit
```

#### String Concatenation

The `str` function concatenates strings. When all arguments are string literals, concatenation happens at compile time:

```clojure
;; Compile-time concatenation (optimized)
(defn ^:export greeting []
  (str "Hello" ", " "World" "!"))
;; Produces: "Hello, World!" in the WASM data section
```

#### Compilable Subset

The static compiler supports a subset of Suss suitable for ahead-of-time compilation:

| Feature | Supported | Notes |
|---------|-----------|-------|
| `def` | Yes | Top-level constants |
| `defn` | Yes | Named functions (use `^:export` for WIT exports) |
| `fn` | Yes | Lambda expressions (no mutable capture) |
| `let`, `if`, `do` | Yes | Control flow |
| `loop/recur` | Yes | Maps to WASM loops |
| `require` | Yes | Import WIT interfaces (`:as` alias or `:refer` direct) |
| `str` | Partial | Compile-time literal concatenation only |
| Numbers | Yes | i32, i64, f64 (no BigInt) |
| Strings | Yes | Linear memory (ptr, len pairs) |
| Vectors | Yes | Persistent 32-way trie, `nth`, `conj`, `count` |
| Maps | Yes | HAMT-based, `get`, `assoc`, `count`, `contains?` |
| Sets | Yes | HAMT-based, `conj`, `count`, `contains?` |
| Lists | Yes | Cons cells, `first`, `rest`, `cons` |
| `eval`, macros | No | Requires interpreter |

#### Running Compiled Components

Use `suss run` to execute compiled WASM components with WASI support:

```bash
# Run a component and invoke an exported function
cargo run -p suss-cli -- run component.wasm --invoke add 3 5

# Output: 8
```

The `suss run` command:
- Loads WASM components (not core modules)
- Provides WASI 0.2 runtime support
- Parses arguments as i32 values (more types coming)
- Prints function return values

This is equivalent to `wasmtime run --invoke` but with WASI pre-configured.

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
* Immutable persistent data structures:
  - Vectors (32-way branching trie, O(log32 n) access)
  - Maps (HAMT - Hash Array Mapped Trie)
  - Sets (HAMT-based)
  - Lists (cons cells)
* Standard library (follows ClojureScript's implementation)

Obviously, the primary compilation target will be static as a WASM/WASI application, so some of the more dynamic features of Clojure may need to be curtailed or made optional at compile-time.

## Suss targets WASM/WASI

Suss targets native compilation (no runtime/GC needed) to WASI 0.2 and 0.3. It's main interfaces to other programs (initially just the REPL) are exposed as WIT interfaces.

## Thoughts/notes

* WASM (via WAT) is already sort of lisp-y, so maybe we can leverage this isomorphism?
