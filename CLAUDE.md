# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

Don't set `RUSTFLAGS=\"-A warnings\"` when invoking `Bash` commands, as we've set that environment variable in the local Claude `env`.

```bash
cargo build                    # Build all crates
cargo test                     # Run all tests
cargo test -p suss-reader      # Test specific crate
```

### CLI Usage (Clojure-style)

The CLI follows Clojure conventions with init options and main modes:

```bash
# Execute mode (default) - run immediately
suss                                    # Start REPL
suss -r                                 # Explicit REPL
suss -e "(+ 1 2)"                       # Evaluate expression
suss script.sus                        # Run script file
suss -                                  # Run from stdin
suss -m myapp.core arg1 arg2            # Run -main with args (TODO)

# Init options (run before main action)
suss -i prelude.sus -r                 # Load file, then start REPL
suss -e "(def x 1)" -e "(+ x 2)"        # Chain evals (last is main)
suss -i lib.sus -e "(process)"         # Load file, then eval

# Compile subcommand - AOT output
suss compile src.sus -o out.wasm                   # REPL component (TODO)
suss compile -m ns src.sus -o app.wasm             # CLI command
suss compile -w api.wit src.sus -o lib.wasm        # Library/plugin
suss compile -n myapp.core -w world.wit             # Namespace mode (multi-file)
suss compile --world :app/v1                        # From deps.sus

# Run subcommand - execute compiled component
suss run app.wasm                       # Run CLI command (auto-finds run)
suss run lib.wasm --invoke add 3 5      # Run specific function
```

### Stateful REPL

The REPL maintains state across expressions via source accumulation:

```clojure
user=> (defn add [a b] (+ a b))
nil
user=> (add 2 3)
5
user=> (in-ns 'myapp.core)
nil
myapp.core=> (defn greet [] 42)
nil
myapp.core=> (greet)
42
user=> (require '[myapp.utils :as u])
nil
user=> (u/helper 10)
20
```

**Key files:**
- `crates/suss-cli/src/session.rs` - `SessionState` with WASM caching, symbol table, core.sus preloading
- `crates/suss-cli/src/completer.rs` - `SussCompleter` for tab completion (rustyline `Completer` trait)
- `crates/suss-cli/src/repl.rs` - `ReplState` (legacy), `is_definition()` helper
- `crates/suss-cli/src/main.rs` - `run_repl()` with `SessionState`, core preloading, tab completion

**SessionState tracks:**
- `ns_definitions: HashMap<String, String>` - Accumulated definitions per namespace
- `current_ns: String` - Current namespace (shown in prompt)
- `ns_aliases: HashMap<String, String>` - Alias → namespace mappings from requires
- `loaded_namespaces: HashMap<String, String>` - Namespace → source content
- `src_paths: Vec<PathBuf>` - Directories to search for namespace files
- `compiler: Compiler` - Reused compiler with `CoreCache` (core.sus parsed once)
- `wasm_cache: HashMap<u64, CacheEntry>` - Source hash → compiled WASM bytes (LRU, max 100)
- `symbols: HashMap<String, SymbolEntry>` - Symbol table for tab completion

**How it works:**
1. Definitions (`defn`, `def`, `deftype`, etc.) are accumulated as source strings
2. Each expression is compiled with all accumulated source prepended
3. `in-ns` switches `current_ns` and changes the prompt
4. `require` loads namespace files from `src/` and stores their source

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
ClojureScript implementation closely. The ClojureScript source is
included as a Git submodule in `clojurescript/`.

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

### Default WIT Worlds

The compiler provides sensible default worlds when no explicit WIT is provided:

**CLI Command World** (`compile -m ns`):
```wit
world command {
    import wasi:cli/environment@0.2.4;
    import wasi:cli/stdin@0.2.4;
    import wasi:cli/stdout@0.2.4;
    import wasi:cli/stderr@0.2.4;
    import wasi:filesystem/...;
    import wasi:random/random@0.2.4;
    import wasi:clocks/...;
    export wasi:cli/run@0.2.4;
}
```

**Embeddable REPL World** (`compile` without -w/-m):
```wit
interface repl {
    eval: func(expr: string) -> result<string, string>;
    rep: func(expr: string) -> result<string, string>;
}
world embeddable-repl {
    import wasi:cli/stdin@0.2.4;
    import wasi:cli/stdout@0.2.4;
    ...
    export repl;
}
```

**Key file:** `crates/suss-compile/src/worlds.rs`

## Key Patterns

### Symbol/Keyword Interning
All symbols and keywords go through `Interner` for O(1) equality. Use `SymbolId`/`KeywordId` instead of strings in hot paths.

### Numeric Tower
`Number` enum: `Integer(BigInt)` → `Ratio(BigRational)` → `Float(f64)`. Supports Clojure radix literals (`2r1010`, `16rFF`).

### Static Compiler Subset
Compilable: `def`, `defn`, `fn` (closures with capture), `apply`, `let`, `if`, `do`, `loop/recur`, numbers (i32/i64/f64), strings, vectors, macros, `defprotocol`, `extend-type`, `deftype`.
Not yet compilable: BigInt (use i64).

### Closure Implementation

Closures use WASM GC structs with typed function references:

**Closure struct types (by arity):**
- `CLOSURE_0` through `CLOSURE_4`: Fixed arity closures (0-4 params)
- `CLOSURE_N`: Higher arity closures (5+ params, uses args array)
- `VARIADIC_CLOSURE`: Builtin variadic functions (+, *, etc.) with fn0..fn8 fields

**Closure struct layout:**
```
struct Closure { type_id: i32, env: array<eqref>, fn: funcref }
```

**Variadic closures with captures:**
Closures like `(fn [& args] body)` that capture values use `CLOSURE_1` (arity=1 for args array) but with a special `type_id = VARIADIC_CAPTURE (-2)`. At call sites, this type_id is checked to pack arguments into an array before calling.

```clojure
;; Works correctly with any number of arguments:
((constantly 42))        ;; → 42
((constantly 42) 1 2 3)  ;; → 42
(let [f (fn [& xs] (count xs))] (f 1 2 3))  ;; → 3
```

**Key files:**
- `ir.rs`: `gc_types` module defines closure types, `type_ids::VARIADIC_CAPTURE`
- `lower.rs`: Sets `is_variadic` flag on `ClosureNew` for variadic closures
- `codegen.rs`: `generate_closure_call()` checks for VARIADIC_CAPTURE and packs args

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
  (^i32 -count [coll]))  ; ^i32 return type hint
```

**Return Type Hints:** Protocol methods can have explicit return type hints using ClojureScript-style metadata. This is critical for protocol dispatch where the WASM function signature must match.

```clojure
;; Methods returning raw i32 (not boxed eqref):
(defprotocol ICounted (^i32 -count [coll]))
(defprotocol IHash (^i32 -hash [o]))
(defprotocol IEquiv (^i32 -equiv [x y]))

;; Methods returning boxed values (default, no hint needed):
(defprotocol ISeq (-first [coll]) (-rest [coll]))
```

Without `^i32`, methods return boxed `eqref`. With `^i32`, codegen generates unboxed i32 return and adds unboxing code to the function body.

**Extending Types:**
```clojure
(extend-type PersistentVector
  IJsonable
  (-to-json [coll] 42)  ; Protocol method implementations

  ICounted
  (-count [coll] (.-cnt coll)))
```

**Key files:**
- `crates/suss-compile/src/analyze.rs` - Protocol/extension parsing (`AnalyzedProtocol`, `AnalyzedProtocolMethod`, `ProtocolParam`)
- `crates/suss-compile/src/lower.rs` - Protocol lowering, return type hints via `method_return_types` HashMap
- `crates/suss-compile/src/codegen.rs` - Dispatch table population, `type_to_valtype_for_signature()` for type hints

**Built-in protocol methods** (method_ids 0-9):
- `-lookup` (0), `-assoc` (1), `-count` (2), `-nth` (3), `-conj` (4)
- `-first` (5), `-rest` (6), `-seq` (7), `-hash` (8), `-equiv` (9)

**User-defined methods** start at method_id 100+.

**Dispatch table:** `type_id * 10 + method_id` indexes into a funcref table. Size is dynamically calculated based on number of deftypes. Dispatch entries are created from `extend-type` declarations in core.sus during lowering.

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

**Mutable fields (for lazy sequences):**
```clojure
;; Use ^:mutable for fields that need to be mutated (e.g., thunk caching)
(deftype LazySeq [^:mutable fn ^:mutable s])

;; Mutable fields can be set with set! expression
(set! (.-fn lazy-seq) nil)
```

**Inline protocol implementations:**
```clojure
;; Define a type with protocol implementations inline
(deftype Counter [val]
  ICounted
  (-count [this] (.-val this)))

(count (->Counter 42))  ;; → 42

;; Multiple protocols supported
(deftype Box [value]
  ICounted
  (-count [this] 1)
  IIndexed
  (-nth [this n] (.-value this)))
```

**Key files:**
- `crates/suss-compile/src/analyze.rs` - `AnalyzedDeftype` parsing
- `crates/suss-compile/src/ir.rs` - `DeftypeDef` intermediate representation
- `crates/suss-compile/src/lower.rs` - Constructor generation, field access lowering, `LoweringMode`
- `crates/suss-compile/src/codegen.rs` - WASM GC struct type emission

**Type indices:**
- Built-in GC types: indices 0-38 (see `gc_types` module)
- Reserved deftypes: indices 5-7 (HAMT nodes defined in core.sus)
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

pub struct DeftypeFieldDef {
    pub name: String,
    pub field_type: FieldType,
    pub is_mutable: bool,    // ^:mutable fields use FieldMutability::Mutable
}

// lower.rs
pub enum LoweringMode {
    Full,      // REPL mode
    Component, // compile_files mode (WIT exports)
}
// Note: Both modes use USER_FUNC_OFFSET (2) for function indices
// because runtime helpers (hash_string, get_type_id) are emitted in both modes.
```

**Implementation notes:**
- Constructors (`->TypeName`) are regular functions emitted before user functions
- Field 0 of every struct is `type_id: i32` for protocol dispatch
- `.-field` access uses `struct.get` with dynamically resolved field index
- `(set! (.-field obj) val)` uses `struct.set` for mutable fields (returns the value for chaining)
- `instance?` uses `ref.test` against the GC type index
- User types shift helper type indices (use `helper_type()` method for dynamic offset calculation)
- Reserved types (gc_type_idx < NUM_GC_TYPES) now generate constructors (`->PersistentVector`, etc.)
- Built-in types are declared in core.sus with `^:type-id N` metadata for bootstrap compatibility

### core.sus (Auto-Loaded Library)

Following ClojureScript semantics, `core.sus` is automatically loaded before user code. It contains:
- **Built-in collection types** with reserved type IDs (Cons, PersistentVector, PersistentMap, PersistentSet)
- **HAMT node types** with reserved type IDs (BitmapIndexedNode, ArrayNode, HashCollisionNode)
- **Sequence types** (IndexedSeq, MapEntry, LazySeq)
- Protocol definitions (ICounted, IIndexed, ISeq, ISeqable, ILookup, IAssociative, ICollection, IEquiv, IHash, IMapEntry)
- Protocol implementations via `extend-type`:
  - PersistentVector: `-nth`, `-first`, `-rest`, `-conj`, `-count`, `-seq`
  - PersistentMap: `-lookup`, `-assoc`, `-count`, `-seq`
  - PersistentSet: `-lookup`, `-conj`, `-count`, `-seq`
  - Cons: `-first`, `-rest`, `-count`
  - IndexedSeq: `-first`, `-rest`, `-count`, `-seq`
  - MapEntry: `-key`, `-val`, `-count`, `-nth`, `-seq`
  - LazySeq: `-first`, `-rest`, `-seq`
- Vector trie helper functions (`tail-off`, `array-for`, `new-path`, `push-tail`, `-vec-conj-overflow`, `-vec-conj-push`)
- HAMT helper functions (`hamt-mask`, `hamt-bitpos`, `hamt-index`, `bin-find`, `an-find`, `hcn-find`, `inode-find`, `bin-assoc`, `inode-assoc`)
- HAMT traversal helpers (`-collect-map-entries`, `-collect-set-entries` for ISeqable)
- User-facing sequence functions (`seq`, `first`, `rest`, `next`, `key`, `val`, `cons`, `hash`)
- Higher-order functions (`map`, `filter`, `remove`, `take`, `drop`, `take-while`, `drop-while`, `reduce`, `iterate`, `repeat`, `repeatedly`, `range`, `concat2`, `mapcat`)
- Numeric helpers (`inc`, `dec`, `pos?`, `neg?`, `zero?`)
- `lazy-seq` macro for deferred evaluation

**Collection Literal Desugaring (Temporary):**

Non-empty collection literals are desugared to inline `conj`/`assoc` calls in `lower.rs`:
- `[1 2 3]` → `(conj (conj (conj [] 1) 2) 3)`
- `{1 2}` → `(assoc {} 1 2)`
- `#{1 2}` → `(conj (conj #{} 1) 2)`

This is temporary until variadic functions are implemented, at which point they should desugar to `(vector ...)`, `(hash-map ...)`, `(hash-set ...)`.

**Key file:** `crates/suss-compile/src/core.sus`

**How it works:**
1. `lib.rs` includes core.sus via `include_str!`
2. Before compiling user code, core.sus is parsed and analyzed
3. Protocol definitions and helper functions become available to all user code
4. HAMT node deftypes use reserved type IDs to match hardcoded ir.rs constants

**Note:** core.sus has `(ns suss.core)` declaration. Functions are registered with namespace-qualified names in file compilation mode. The REPL path strips the `ns` form for backward compatibility.

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
(acopy dst dst-off src src-off len) ; array.copy - copy elements between arrays
```

**Null/nil checking:**
```clojure
(nil? x)             ; true if x is nil or null reference, false otherwise
                     ; First checks RefIsNull for null struct fields (e.g., vector root)
                     ; Then checks i31ref for NIL_SENTINEL (value 0)
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

### Cross-Namespace Requires

Suss supports Clojure-style namespace requires for organizing code across multiple files.

**Namespace Declaration:**
```clojure
(ns myapp.core
  (require '[myapp.utils :as utils])
  (require '[myapp.helpers :refer [helper-fn]])
  (require '[wasi:random/random :as random]))  ;; WASI still works

;; Using required namespaces
(utils/process-data x)
(helper-fn y)
(random/get-random-u64)
```

**File Mapping (Clojure convention):**
- `myapp.core` → `src/myapp/core.sus`
- `myapp.utils-helpers` → `src/myapp/utils_helpers.sus`
- Dots become directory separators, hyphens become underscores

**Require Options:**
- `:as alias` - Import all functions with alias prefix
- `:refer [fn1 fn2]` - Import specific functions without prefix
- `:refer :all` - Import all public functions without prefix

**Compilation Modes:**
```bash
# Single-file compilation (existing)
suss compile src.sus -w world.wit -o out.wasm

# Multi-file namespace compilation (new)
suss compile -n myapp.core -w world.wit -o app.wasm
suss compile -n myapp.core --src lib --src vendor -w world.wit -o app.wasm
```

**Key files:**
- `crates/suss-compile/src/analyze.rs` - `AnalyzedRequire`, `RequireSource`, require parsing
- `crates/suss-compile/src/lib.rs` - `DependencyResolver`, `ns_to_path()`, `compile_with_namespaces()`
- `crates/suss-compile/src/lower.rs` - Namespace resolution: `ns_aliases`, `referred_symbols`, `resolve_func_name()`

**Resolution Order (for unqualified symbols):**
1. Current namespace
2. Referred symbols from requires
3. `suss.core` (implicit require)
4. Unqualified name (backward compatibility)

**Key data structures:**
```rust
// analyze.rs
pub enum RequireSource {
    WitInterface { interface: String },
    SussNamespace { namespace: String },
}

pub struct AnalyzedRequire {
    pub source: RequireSource,
    pub alias: Option<String>,
    pub refers: Vec<String>,
    pub refer_all: bool,
}

// lib.rs
pub struct DependencyResolver {
    deps: HashMap<String, Vec<String>>,  // ns -> dependencies
    parsed: HashMap<String, Vec<Edn>>,   // ns -> parsed expressions
    files: HashMap<String, PathBuf>,     // ns -> source file
    src_paths: Vec<PathBuf>,
}
```

**Dependency Resolution:**
- Topological sort ensures dependencies compile before dependents
- Circular dependency detection with helpful error messages
- Lazy discovery: only scans files as needed from entry namespace

### Bundled WASI
WASI 0.2.4 WIT files are bundled. When world.wit imports `wasi:*`, they're auto-loaded. No deps/ folder needed for WASI packages.

### Project Configuration (deps.sus)

Multi-world projects use `deps.sus` (EDN format like Clojure's deps.edn):

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

- `SussConfig` - Loaded from deps.sus, contains worlds and src-paths
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
    std::fs::write("./tmp/debug.wasm", &wasm).unwrap();
}
```

Then run: `cargo test -p suss-compile --test compile_expr dump_wasm`

### Inspecting WASM

```bash
# Validate WASM
wasm-tools validate --features gc ./tmp/debug.wasm

# Disassemble to WAT format
wasm-tools print ./tmp/debug.wasm > ./tmp/debug.wat

# Print with instruction offsets (for backtrace debugging)
wasm-tools print --print-offsets ./tmp/debug.wasm

# Show section overview
wasm-tools objdump ./tmp/debug.wasm

# Run directly with wasmtime
~/.wasmtime/bin/wasmtime run -W gc --invoke eval ./tmp/debug.wasm
```

### Debugging Dispatch Table Issues

When debugging `call_indirect` issues with the protocol dispatch table:

1. **Verify element sections**: `grep "(elem" ./tmp/debug.wat`
2. **Check table size**: `grep "(table" ./tmp/debug.wat`
3. **Verify function types**: `grep "type (;N;)" ./tmp/debug.wat`
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
wasm-tools print ./tmp/debug.wasm > ./tmp/debug.wat
wasm-tools parse ./tmp/debug.wat -o ./tmp/debug_round.wasm
~/.wasmtime/bin/wasmtime run -W gc --invoke eval ./tmp/debug_round.wasm
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

- `clojurescript/` - ClojureScript source (Git submodule)
  - `src/main/clojure/cljs/core.cljc` - Core library implementation
  - `src/test/cljs/` - ClojureScript test suite
- `reference/cljs-tests/` - Conformance test cases adapted from ClojureScript

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

Suss conformance test files in `reference/cljs-tests/`:
- `collections.sus` - Vectors, maps, sets (76 tests)
- `core.sus` - Arithmetic, logic, control flow, functions (125 tests)
- `benchmarks.sus` - Performance benchmark definitions

For the original ClojureScript tests, see `clojurescript/src/test/cljs/`.

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
| `fibonacci.sus` | loop/recur | ✓ Working |
| `factorial.sus` | loop/recur, reduce, range | ✓ Working |
| `game_of_life.sus` | for, mapcat, frequencies, destructuring, sets | ✓ Working |
| `primes.sus` | filter, some, range, sets | ✓ Working |
| `quicksort.sus` | filter, concat | ✓ Working |
| `tree_traversal.sus` | map keyword access, concat | ✓ Working |

These programs are valid Clojure code and serve as progress markers. When a sample runs correctly, it demonstrates that feature set is complete.
