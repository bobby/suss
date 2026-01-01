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

user=> (+ 1 2 3)
6
user=> (defn add1 [x] (+ x 1))
nil
user=> (add1 5)
6
user=> (def x 42)
nil
user=> x
42
user=> (let [a 10 b 20] (+ a b))
30
user=> (apply + [1 2 3 4 5])
15
```

#### Stateful REPL Features

The REPL maintains state across expressions:

```clojure
;; Definitions persist across expressions
user=> (defn greet [name] (str "Hello, " name))
nil
user=> (greet "World")
"Hello, World"

;; Switch namespaces with in-ns
user=> (in-ns 'myapp.core)
nil
myapp.core=> (defn helper [] 42)
nil
myapp.core=> (helper)
42

;; Load namespace files with require
user=> (require '[myapp.utils :as u])
nil
user=> (u/process 10)
20
```

**Supported REPL forms:**
- `(in-ns 'namespace)` - Switch current namespace (changes prompt)
- `(require '[ns :as alias])` - Load namespace from `src/` directory
- `(defn ...)`, `(def ...)`, `(deftype ...)`, etc. - Definitions persist

### Running Tests

```bash
# Run all unit tests
cargo test

# Run conformance tests (verifies ClojureScript semantics)
cargo test -p suss-compile --test conformance -- --nocapture

# Run specific conformance category
cargo test -p suss-compile --test conformance test_conformance_collections -- --nocapture
cargo test -p suss-compile --test conformance test_conformance_core -- --nocapture
```

### Running Benchmarks

```bash
# Run all performance benchmarks
cargo bench -p suss-compile --bench performance

# Run specific benchmark groups
cargo bench -p suss-compile --bench performance -- compilation
cargo bench -p suss-compile --bench performance -- execution
cargo bench -p suss-compile --bench performance -- wasm_size
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

#### Macros

Suss supports ClojureScript-style compile-time macros with syntax-quote, unquote, and unquote-splicing:

```clojure
;; Define a macro
(defmacro unless [test & body]
  `(if (not ~test)
     (do ~@body)
     nil))

;; Use it
(unless (empty? items)
  (process items)
  (log "done"))
```

**Built-in core macros:**
- `when`, `when-not` - Conditional execution
- `and`, `or` - Short-circuit boolean operators
- `cond` - Multi-branch conditional
- `case` - Value dispatch

**Macro features:**
- Syntax-quote (`` ` ``) with automatic symbol qualification
- Unquote (`~`) to evaluate expressions inside syntax-quote
- Unquote-splicing (`~@`) to splice collections
- Gensym (`symbol#`) for hygienic symbols → `symbol__N__auto__`
- Rest parameters (`& body`) for variadic macros

Macros are expanded at compile time before code generation. The expansion uses a tree-walking interpreter to evaluate macro bodies.

#### Protocols and User-Defined Types

Suss supports ClojureScript-style protocols for polymorphic dispatch:

```clojure
;; Define a protocol
(defprotocol IJsonable
  "Protocol for JSON serialization"
  (-to-json [this]))

;; Extend built-in types
(extend-type PersistentVector
  IJsonable
  (-to-json [coll]
    ;; Implementation here
    42))

;; Use protocol methods
(-to-json [1 2 3])
```

#### User-Defined Types (deftype)

Create custom WASM GC struct types with `deftype`:

```clojure
;; Define a type with fields
(deftype Point [x y])

;; Create instances with auto-generated constructor
(def p (->Point 10 20))

;; Access fields with .-field syntax
(.-x p)  ;; → 10
(.-y p)  ;; → 20

;; Type checking with instance?
(instance? Point p)       ;; → true
(instance? Point [1 2])   ;; → false
```

User-defined types compile to WASM GC structs with automatic:
- Constructor function (`->TypeName`)
- Field accessors (`.-field`)
- Type identity for `instance?` checks

**Built-in protocols** (defined in `core.suss`, auto-loaded before user code):
- `ICounted` - `^i32 -count` for countable collections
- `IIndexed` - `-nth` for indexed access
- `ISeq` - `-first`, `-rest` for sequential access
- `ISeqable` - `-seq` for conversion to sequences (vectors, maps, sets)
- `ILookup` - `-lookup` for key-based lookup
- `IAssociative` - `-assoc`, `^i32 -contains-key` for associative structures
- `ICollection` - `-conj` for adding elements
- `IEquiv` - `^i32 -equiv` for equality testing
- `IHash` - `^i32 -hash` for hashing
- `IMapEntry` - `-key`, `-val` for map entry access

**Built-in sequence types:**
- `IndexedSeq` - Array-backed sequence for efficient indexed access
- `MapEntry` - Key-value pair returned when iterating maps
- `LazySeq` - Lazy sequence with thunk caching for deferred evaluation

**Return type hints:** Methods with `^i32` return unboxed i32 values (for dispatch table type matching). Methods without hints return boxed `eqref`.

**Low-level primitives** for protocol implementations:
- `(.-field struct)` - Access struct fields (e.g., `(.-cnt vec)`)
- `(nil? x)` - Check if value is nil
- `(aget arr idx)` - Array element access
- `(aset arr idx val)` - Array element mutation (internal use)
- `(aclone arr)` - Clone array (for structural sharing)
- `(make-array n)` - Create array of size n
- `(acopy dst dst-off src src-off len)` - Copy elements between arrays
- `(bit-count x)` - Population count (for HAMT)

#### Compilable Subset

The static compiler supports a subset of Suss suitable for ahead-of-time compilation:

| Feature | Supported | Notes |
|---------|-----------|-------|
| `def` | Yes | Top-level constants |
| `defn` | Yes | Named functions (use `^:export` for WIT exports) |
| `fn` | Yes | First-class closures with variable capture |
| `apply` | Yes | Dynamic dispatch for arities 0-8 |
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
| Sequences | Yes | `seq`, `first`, `rest`, `next` for all collections |
| Lazy Seqs | Yes | `lazy-seq`, `map`, `filter`, `take`, `drop`, `range` |
| HOFs | Yes | `reduce`, `iterate`, `repeat`, `repeatedly`, `mapcat` |
| Macros | Yes | `defmacro` with syntax-quote, core macros built-in |
| Protocols | Yes | `defprotocol`, `extend-type` for user-defined abstractions |
| `deftype` | Yes | User-defined WASM GC struct types with constructors |
| `eval` | No | Requires runtime interpreter |

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
* First-class functions (closures with variable capture)
* Lazy sequences with higher-order functions (`map`, `filter`, `reduce`, `take`, `range`, etc.)
* Compile-time macros (`defmacro` with syntax-quote, unquote, unquote-splicing)
* Standard library (follows ClojureScript's implementation)

Obviously, the primary compilation target will be static as a WASM/WASI application, so some of the more dynamic features of Clojure may need to be curtailed or made optional at compile-time.

## Sample Programs

The `samples/` directory contains classic Clojure programs that serve as implementation targets:

| Sample | Description | Status |
|--------|-------------|--------|
| `fibonacci.suss` | Fibonacci (naive, tail-recursive, sequence) | Partial |
| `factorial.suss` | Factorial implementations | Partial |
| `game_of_life.suss` | Conway's Game of Life (Christophe Grand's elegant version) | Needs `for`, `frequencies`, destructuring |
| `primes.suss` | Prime number algorithms (trial division, sieve) | Needs `some`, `Math/sqrt` |
| `quicksort.suss` | Functional quicksort | Ready (has `filter`, `concat2`) |
| `tree_traversal.suss` | Binary tree operations using maps | Ready (has `concat2`) |

These programs document the path toward full Clojure compatibility. See `samples/README.md` for details.

## Suss targets WASM/WASI

Suss targets native compilation (no runtime/GC needed) to WASI 0.2 and 0.3. It's main interfaces to other programs (initially just the REPL) are exposed as WIT interfaces.

## Thoughts/notes

* WASM (via WAT) is already sort of lisp-y, so maybe we can leverage this isomorphism?
