# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

Don't set `RUSTFLAGS=\"-A warnings\"` when invoking `Bash` commands, as we've set that environment variable in the local Claude `env`.

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
Compilable: `def`, `defn`, `fn` (closures with capture), `apply`, `let`, `if`, `do`, `loop/recur`, numbers (i32/i64/f64), strings, vectors, macros, `defprotocol`, `extend-type`, `deftype`.
Not yet compilable: BigInt (use i64).

### Macro System

Suss has ClojureScript-style compile-time macros:

**Architecture:**
```
Pipeline: Parse → [Expand] → Analyze → Lower → Codegen
                    ↑
            expand.rs + eval.rs
```

**Key files:**
- `crates/suss-compile/src/expand.rs` - MacroEnv, MacroDef, syntax-quote expansion, gensym
- `crates/suss-compile/src/eval.rs` - Tree-walking interpreter for macro bodies (~30 primitives)

**Built-in core macros:** `when`, `when-not`, `and`, `or`, `cond`, `case`, `->`, `->>`, `when-let`, `if-let`, `doto`, `..`

**Key structures:**
```rust
// expand.rs
pub struct MacroDef {
    pub name: String,
    pub params: Vec<String>,
    pub rest_param: Option<String>,  // Name after &
    pub body: Edn,
}

pub struct MacroEnv {
    macros: HashMap<String, MacroDef>,
    gensym_counter: u64,
    current_ns: Option<String>,
    evaluator: MacroEvaluator,
}
```

**Gensym:** `symbol#` in syntax-quote expands to `symbol__N__auto__` where N is unique.

### Protocol System

Suss supports ClojureScript-style protocols and type extensions:

**Defining Protocols:**
```clojure
(defprotocol IJsonable
  (-to-json [this]))

(defprotocol ICounted
  (-count [coll]))
```

**Extending Types:**
```clojure
(extend-type PersistentVector
  IJsonable
  (-to-json [coll] 42)  ; Protocol method implementations

  ICounted
  (-count [coll] (.-cnt coll)))
```

**Key files:**
- `crates/suss-compile/src/analyze.rs` - Protocol/extension parsing (`AnalyzedProtocol`, `AnalyzedExtension`)
- `crates/suss-compile/src/lower.rs` - Protocol lowering, dispatch entry generation
- `crates/suss-compile/src/codegen.rs` - Dispatch table population

**Built-in protocol methods** (method_ids 0-9):
- `-lookup` (0), `-assoc` (1), `-count` (2), `-nth` (3), `-conj` (4)
- `-first` (5), `-rest` (6), `-seq` (7), `-hash` (8), `-equiv` (9)

**User-defined methods** start at method_id 100+.

**Dispatch table:** `type_id * 10 + method_id` indexes into a funcref table of 160 entries (16 types × 10 methods).

### User-Defined Types (deftype)

Suss supports `deftype` for user-defined WASM GC struct types:

**Basic usage:**
```clojure
(deftype Point [x y])
(def p (->Point 10 20))
(.-x p)  ;; → 10
(instance? Point p)  ;; → true
```

**Reserved type IDs (for bootstrap types):**
```clojure
;; Use ^:type-id N to reserve a specific GC type index
(deftype ^:type-id 5 BitmapIndexedNode [^i32 bitmap arr])
```

**Key files:**
- `crates/suss-compile/src/analyze.rs` - `AnalyzedDeftype` parsing
- `crates/suss-compile/src/ir.rs` - `DeftypeDef` intermediate representation
- `crates/suss-compile/src/lower.rs` - Constructor generation, field access lowering, `LoweringMode`
- `crates/suss-compile/src/codegen.rs` - WASM GC struct type emission

**Type indices:**
- Built-in GC types: indices 0-38 (see `gc_types` module)
- Reserved deftypes: indices 5-7 (HAMT nodes defined in core.suss)
- User deftypes: indices 39+ (assigned at lowering time)
- Helper types follow user types
- Type IDs: built-in 0-10, user types start at 256 (`USER_TYPE_BASE`)

**Key structures:**
```rust
// analyze.rs
pub struct AnalyzedDeftype {
    pub name: String,
    pub fields: Vec<DeftypeField>,
    pub implementations: Vec<AnalyzedProtocolImpl>,
    pub reserved_type_id: Option<u32>,
}

// ir.rs
pub struct DeftypeDef {
    pub name: String,
    pub fields: Vec<DeftypeFieldDef>,
    pub gc_type_idx: u32,    // WASM GC type index
    pub type_id: i32,        // Runtime type ID (256+)
}

// lower.rs
pub enum LoweringMode {
    Full,      // REPL: includes runtime helper offset (39)
    Component, // compile_files: no runtime helpers
}
```

**Implementation notes:**
- Constructors (`->TypeName`) are regular functions emitted before user functions
- Field 0 of every struct is `type_id: i32` for protocol dispatch
- `.-field` access uses `struct.get` with dynamically resolved field index
- `instance?` uses `ref.test` against the GC type index
- User types shift helper type indices (use `helper_type()` method for dynamic offset calculation)
- Reserved types (gc_type_idx < NUM_GC_TYPES) skip constructor generation and use hardcoded field mappings

### core.suss (Auto-Loaded Library)

Following ClojureScript semantics, `core.suss` is automatically loaded before user code. It contains:
- **HAMT node types** with reserved type IDs (BitmapIndexedNode, ArrayNode, HashCollisionNode)
- Protocol definitions (ICounted, IIndexed, ISeq, ISeqable, ILookup, IAssociative, ICollection, IEquiv, IHash)
- Vector trie helper functions (`tail-off`, `array-for`, `new-path`, `push-tail`)
- HAMT helper functions (`hamt-mask`, `hamt-bitpos`, `hamt-index`, `bin-find`, `an-find`, `hcn-find`, `inode-find`)

**Key file:** `crates/suss-compile/src/core.suss`

**How it works:**
1. `lib.rs` includes core.suss via `include_str!`
2. Before compiling user code, core.suss is parsed and analyzed
3. Protocol definitions and helper functions become available to all user code
4. HAMT node deftypes use reserved type IDs to match hardcoded ir.rs constants

**Note:** core.suss has NO `(ns ...)` declaration - all definitions are at top level.

### Low-Level Primitives

WASM GC operations exposed to Suss for implementing protocols:

**Struct field access:**
```clojure
(.-cnt vec)    ; struct.get PersistentVector.cnt → i32
(.-shift vec)  ; struct.get PersistentVector.shift → i32
(.-root vec)   ; struct.get PersistentVector.root → eqref
(.-tail vec)   ; struct.get PersistentVector.tail → eqref
(.-first cons) ; struct.get Cons.first
(.-rest cons)  ; struct.get Cons.rest
(.-x point)    ; struct.get for user-defined types (deftype Point [x y])
```

**Array operations (WASM GC arrays):**
```clojure
(aget arr idx)       ; array.get - get element at index
(aset arr idx val)   ; array.set - set element (internal use for construction)
(alength arr)        ; array.len - get array length
(aclone arr)         ; array.copy to new array (for structural sharing)
(make-array n)       ; array.new with nil initialization
```

**Null/nil checking:**
```clojure
(nil? x)             ; true if x is nil, false otherwise
                     ; Uses ref.test i31 + i31.get_s check for NIL_SENTINEL
```

**Type checking:**
```clojure
(instance? PersistentVector obj)  ; ref.test
(instance? PersistentMap obj)
(instance? PersistentSet obj)
(instance? Cons obj)
(instance? String obj)
(instance? MyDeftype obj)         ; works with user-defined types
```

**Bit manipulation:**
```clojure
(bit-and 0xFF 0x0F)            ; i32.and → 15
(bit-or 0x01 0x02)             ; i32.or → 3
(bit-xor 0xFF 0x0F)            ; i32.xor → 240
(bit-shift-left x 5)           ; i32.shl
(bit-shift-right x 5)          ; i32.shr_s (arithmetic)
(unsigned-bit-shift-right x 5) ; i32.shr_u (logical)
(bit-count x)                  ; i32.popcnt - population count (for HAMT)
```

### Variadic Arithmetic
Arithmetic operators match ClojureScript semantics:
- `(+)` → 0, `(*)` → 1 (identity elements)
- `(- x)` → negation, `(/ x)` → reciprocal
- Division always returns float: `(/ 10 4)` → 2.5
- `(apply + [1 2 3])` → 6 (works for arities 0-8)

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

## Debugging WASM Issues

### Dumping WASM for Inspection

Add a test in `compile_expr.rs` to dump WASM to a file:
```rust
#[test]
fn dump_wasm_for_debug() {
    let expr = "(your expression here)";
    let mut compiler = Compiler::new();
    let wasm = compiler.compile_expr(expr).unwrap();
    std::fs::write("/tmp/debug.wasm", &wasm).unwrap();
}
```

Then run: `cargo test -p suss-compile --test compile_expr dump_wasm`

### Inspecting WASM

```bash
# Validate WASM
wasm-tools validate --features gc /tmp/debug.wasm

# Disassemble to WAT format
wasm-tools print /tmp/debug.wasm > /tmp/debug.wat

# Print with instruction offsets (for backtrace debugging)
wasm-tools print --print-offsets /tmp/debug.wasm

# Show section overview
wasm-tools objdump /tmp/debug.wasm

# Run directly with wasmtime
~/.wasmtime/bin/wasmtime run -W gc --invoke eval /tmp/debug.wasm
```

### Debugging Dispatch Table Issues

When debugging `call_indirect` issues with the protocol dispatch table:

1. **Verify element sections**: `grep "(elem" /tmp/debug.wat`
2. **Check table size**: `grep "(table" /tmp/debug.wat`
3. **Verify function types**: `grep "type (;N;)" /tmp/debug.wat`
4. **Compare with working case**: Dump both working and failing WASM and diff them

Key dispatch table facts:
- Table size: 160 (16 type slots × 10 method slots)
- Index formula: `type_id * 10 + method_id`
- Common indices: VEC_CONJ=84, SET_COUNT=102, SET_CONJ=104

### Isolating Runtime Issues

To test if an issue is with the computed index vs the table itself:
1. Modify the WAT to use hardcoded index: replace dynamic calculation with `i32.const <index>`
2. If hardcoded works but dynamic fails, the issue is in index calculation
3. If hardcoded also fails, the issue is in table initialization

### Roundtripping Through WAT

```bash
# Convert WASM → WAT → WASM to verify encoding
wasm-tools print /tmp/debug.wasm > /tmp/debug.wat
wasm-tools parse /tmp/debug.wat -o /tmp/debug_round.wasm
~/.wasmtime/bin/wasmtime run -W gc --invoke eval /tmp/debug_round.wasm
```

### WASM GC Structural Typing Gotcha

**Important:** WASM GC uses structural typing for `ref.test`. Two struct types with identical field layouts are indistinguishable at runtime, even if they have different type indices.

For example, if PersistentMap and PersistentSet both have:
```
struct { type_id: i32, cnt: i32, root: eqref }
```

Then `ref.test (ref $PersistentMap)` will return true for a PersistentSet! The solution is to add a marker field to make structs structurally distinct:
```
PersistentSet: struct { type_id: i32, cnt: i32, root: eqref, _marker: i32 }
```

This affects `get_type_id` which uses `ref.test` chains to determine type.

### Debugging Macro Expansion

When macros produce unexpected results:

1. **Check expansion output:** Add debug prints in `expand.rs`:
   ```rust
   // In expand_call after macro expansion:
   eprintln!("Macro {} expanded to: {:?}", name, result);
   ```

2. **Verify syntax-quote:** Test quote expansion in isolation:
   ```rust
   // expand_syntax_quote should handle:
   // - `sym` → (quote sym)
   // - `~expr` → evaluate expr
   // - `~@coll` → splice coll elements
   // - `sym#` → gensym
   ```

3. **Test macro evaluator:** The `MacroEvaluator` in `eval.rs` supports:
   - Special forms: `quote`, `if`, `let`, `do`, `fn`, `loop/recur`
   - Core primitives: `list`, `cons`, `first`, `rest`, `seq`, `concat`, `=`, `+`, `-`, `*`, `/`, etc.
   - If a primitive is missing, add it to `MacroEvaluator::new()`

4. **Common issues:**
   - `Symbol.name` is a field, not a method (use `sym.name` not `sym.name()`)
   - `Keyword.name` is a field, not a method (use `k.name` not `k.name()`)
   - `Env.define()` creates new bindings, `Env.set()` only updates existing ones
   - `Number` needs conversion methods (`to_i64()`, `from_i64()`, etc.)

## References

### WASM Specifications

- [WASM 3.0 Core Specification](https://webassembly.github.io/spec/core/bikeshed/) - Full spec (GC + function references)
- [Typed Function References](https://github.com/WebAssembly/spec/blob/wasm-3.0/proposals/function-references/Overview.md) - `ref.func`, `call_ref` for closures
- [GC Proposal](https://github.com/WebAssembly/gc/blob/main/proposals/gc/Overview.md) - Structs, arrays, i31ref

### Clojure References

- `reference/cljs.core.clj` - Local copy of ClojureScript core
- `reference/cljs-tests/` - Conformance test cases adapted from ClojureScript
- [ClojureScript Source](https://github.com/clojure/clojurescript) - Persistent data structure implementations

## Conformance & Performance Testing

### Conformance Tests

Test cases adapted from ClojureScript's test suite to verify Suss semantics:

```bash
# Run all conformance tests
cargo test -p suss-compile --test conformance -- --nocapture

# Run specific category
cargo test -p suss-compile --test conformance test_conformance_collections -- --nocapture
cargo test -p suss-compile --test conformance test_conformance_core -- --nocapture
```

Test files in `reference/cljs-tests/`:
- `collections.suss` - Vectors, maps, sets (76 tests)
- `core.suss` - Arithmetic, logic, control flow, functions (125 tests)
- `benchmarks.suss` - Performance benchmark definitions

Test format (EDN):
```clojure
{:name "vector-conj"
 :category :vectors
 :expr "(conj [1 2] 3)"
 :expected [1 2 3]
 :skip false}  ; Optional, skip unimplemented features
```

### Performance Benchmarks

Criterion-based benchmarks measuring compilation and execution performance:

```bash
# Run all benchmarks
cargo bench -p suss-compile --bench performance

# Run specific groups
cargo bench -p suss-compile --bench performance -- compilation
cargo bench -p suss-compile --bench performance -- execution
cargo bench -p suss-compile --bench performance -- wasm_size
cargo bench -p suss-compile --bench performance -- vector_scaling

# Quick validation (verify benchmarks run)
cargo bench -p suss-compile --bench performance -- --test
```

Benchmark categories:
- **compilation** - Time to compile expressions to WASM
- **execution** - Time to instantiate and run WASM modules
- **wasm_size** - Binary size of compiled output
- **vector_scaling** - Performance scaling with collection size

## Sample Programs

The `samples/` directory contains classic Clojure programs as implementation targets. These document idiomatic patterns we're working toward supporting.

| Sample | Features Needed | Status |
|--------|-----------------|--------|
| `fibonacci.suss` | loop/recur | Partial |
| `factorial.suss` | loop/recur, reduce, range | Partial |
| `game_of_life.suss` | for, mapcat, frequencies, destructuring, sets | Needs HOFs |
| `primes.suss` | filter, some, range, sets, Math/sqrt | Needs HOFs |
| `quicksort.suss` | filter, concat | Needs filter, concat |
| `tree_traversal.suss` | map keyword access, concat | Needs concat |

These programs are valid Clojure code and serve as progress markers. When a sample runs correctly, it demonstrates that feature set is complete.
