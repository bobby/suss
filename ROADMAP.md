# Suss Roadmap

> **Suss requires WASM GC.** All execution uses WASM GC types (structs, arrays, i31ref). There is no non-GC fallback mode.

This roadmap is organized around **self-hosting**: implementing Suss's persistent data structures and core library in Suss itself, with the compiler providing only irreducible primitives.

---

## Architecture: Self-Hosting via deftype

### The Goal

The compiler should be **minimal**. All collection algorithms and protocol implementations should live in `core.sus`, not in Rust codegen. This makes Suss self-extending: adding new collection operations requires only Suss code.

### The Bootstrap Problem

Collection literals `[1 2 3]`, `{:a 1}`, `#{1 2}` need type indices at compile time. If PersistentVector/Map/Set were pure `deftype` definitions in core.sus, we'd have a circular dependency.

**Solution: Struct shapes in compiler, behaviors in Suss**
- Collection struct layouts (field order, type indices 8-10) stay in compiler
- ALL algorithms, ALL protocol implementations move to core.sus
- HAMT nodes (BitmapIndexedNode, etc.) can be pure deftype (no literal syntax)
- User-defined types start at index 256+

### Irreducible Primitives (Must Stay in Compiler)

| Category | What | Why Irreducible |
|----------|------|-----------------|
| **Atomic Values** | i31ref (nil/bool/small int), boxed i64, boxed f64, strings | WASM GC fundamentals |
| **Generic Struct Ops** | deftype → struct.new/get | Need WASM type generation |
| **Generic Array Ops** | make-array, aget, aset, alength, aclone | WASM GC array primitives |
| **Control Flow** | if, let, loop/recur, do | Core language semantics |
| **Closures** | fn, apply, call_ref | Function references |
| **Type Checking** | instance?, ref.test | Runtime dispatch |
| **Primitives** | i32/i64/f64 arithmetic, bit-ops, popcnt | CPU operations |
| **Protocol Dispatch** | get-type-id, dispatch table, call_indirect | Polymorphism mechanism |
| **Reserved Types** | Collection struct shapes (PersistentVector/Map/Set) | Bootstrap literals |

### What Moves to core.sus

| What | Currently | After Self-Hosting |
|------|-----------|-------------------|
| **HAMT Nodes** | Hardcoded structs in ir.rs | `(deftype BitmapIndexedNode ...)` |
| **Vector Trie Ops** | 5 Rust functions in codegen.rs | Pure Suss |
| **HAMT Ops** | 13 Rust functions in codegen.rs | Pure Suss |
| **Protocol Impls** | 14 wrapper functions in codegen.rs | `(extend-type ...)` |
| **Collection Algorithms** | Mixed Rust/Suss | All Suss |

### How core.sus Functions Are Called

core.sus is compiled **before** user code. Functions defined there get known indices:

```
Function Index Layout:
  0..N-1              : WASI/WIT imports (if any)
  N..N+H-1            : Runtime helper functions (hash, get-type-id, etc.)
  N+H..N+H+P-1        : Protocol implementation wrappers
  N+H+P..N+H+P+C-1    : core.sus functions (tail-off, inode-find, etc.)
  N+H+P+C..           : User functions
```

**Calling convention:** When lowering `(inode-find ...)`, the compiler:
1. Looks up `inode-find` in the function index map (populated when core.sus is lowered)
2. Emits `call $inode-find` with the resolved index
3. No special handling needed - same as any user function call

**Key invariant:** core.sus functions are lowered first, so their indices are known when lowering user code or when codegen needs to call them.

---

## Current Status

### Completed
- [x] First-class functions (closures with `call_ref`)
- [x] `apply` for variadic dispatch (arities 0-8)
- [x] Compile-time macros (`defmacro`, syntax-quote, gensym)
- [x] User protocols (`defprotocol`, `extend-type`)
- [x] xxHash32 for consistent hashing
- [x] Protocol dispatch table with `call_indirect`
- [x] core.sus auto-loading infrastructure

### In Progress
- [x] Array primitives: `aget`, `aset`, `alength`, `aclone`, `make-array`
- [x] `bit-count` (popcnt for HAMT bitmap indexing)
- [x] `nil?` check (handles both nil sentinel and null references)
- [x] Parser fix for `nil?`, `true?`, `false?` symbols
- [x] `deftype` basic implementation (Phase 3.1 - fields, constructor, field access, instance?)
- [x] Reserved type indices for bootstrap (Phase 3.3)
- [x] Bootstrap HAMT nodes in core.sus (Phase 4)
- [x] Protocol method return type hints (`^i32` on `-count`, `-hash`, etc.)
- [x] WIT param_offset propagation fix (unblocks VEC_CONJ migration)
- [x] Protocol-from-protocol call fix (param_offset in dispatch table)
- [x] VEC_CONJ migrated to core.sus (Phase 5 - first major protocol migration)
- [x] SET_CONJ migrated to core.sus via protocol dispatch
- [x] MAP_ASSOC migrated to core.sus via protocol dispatch
- [x] Collection literal desugaring (temporary inline approach - see note below)
- [x] `get` and `contains?` now use protocol dispatch
- [x] TCO with WIT-exported functions (function index offset fix in Component mode)
- [x] End-to-end verification of complex trie operations
- [x] `deftype` with inline protocols (Phase 3.2)
- [x] Cross-namespace require system (compile-time, Phases 8.1-8.6 complete)
- [x] REPL runtime loading for `require` and `in-ns` (Phase 8.7)
- [x] Convert core.sus to suss.core namespace (Phase 8.8)
- [x] Nested closures bug fix (closure_counter instead of pending_closures.len())
- [x] Phase 5 complete: All protocol impls in core.sus (removed protocol_impl_funcs module)
- [x] Phase 6 complete: All collection algorithm helpers in core.sus (only HASH_STRING, GET_TYPE_ID remain)
- [x] Phase 7 complete: codegen.rs reduced ~25% (~5,300 lines), dead code and deprecated aliases removed
- [x] Phase 12 complete: Full sequence abstraction with lazy sequences and higher-order functions
  - IndexedSeq, MapEntry, LazySeq types
  - ISeqable for all collection types (vector, map, set)
  - `^:mutable` field support in deftype
  - Higher-order functions: map, filter, reduce, take, drop, range, iterate, etc.
- [x] Phase 13 complete: Multi-arity function support
  - `(defn foo ([x] x) ([x y] (+ x y)))` syntax
  - `(fn ([x] x) ([x y] (+ x y)))` anonymous multi-arity
  - Implemented via variadic closure with arity dispatch
- [x] Phase 11 complete: WASI CLI command support
  - `-main` functions compile to `wasi:cli/run@0.2.4` command components
  - Run with `wasmtime run -W gc,function-references app.wasm`
  - Result type handling for WIT interface exports
- [x] Phase 8.9 partial: REPL enhancements
  - Core.sus caching via `CoreCache` in `Compiler` (parse/analyze once per session)
  - WASM compilation caching with LRU eviction (hash-based, 100 entries)
  - Tab completion for builtins, core.sus symbols, and user definitions
  - `SessionState` with integrated caching, symbol table, and namespace management

### Blocking Issues
- None currently blocking

### Known Bugs
- None currently identified

---

## Priority Zero: Compositional Primitives ✓ COMPLETE

> **Goal:** Stop modifying the compiler for each new feature. Build the primitives that let Suss extend itself.

### The Four Fundamental Primitives

| Primitive | Enables | Status |
|-----------|---------|--------|
| **First-class functions** | `map`, `filter`, `reduce`, higher-order programming | ✓ COMPLETE |
| **apply** | `(apply + [1 2 3])`, variadic dispatch | ✓ COMPLETE |
| **Macros** | `cond`, `when`, `->`, `for`, `core.async` | ✓ COMPLETE |
| **User protocols** | `defprotocol`, `extend-type`, abstraction | ✓ COMPLETE |

### P0.1: First-Class Functions (Closures) ✓ COMPLETE

Functions are **values** that can be passed, returned, and stored.

**WASM 3.0 implementation using Typed Function References:**
```wasm
;; Closure struct: captures environment + typed function reference
(type $closure_fn (func (param (ref eq)) (param eqref) (result eqref)))
(type $Closure (struct
  (field $env (ref eq))              ;; captured variables
  (field $fn (ref $closure_fn))))    ;; typed funcref - no runtime check

;; Call closure with call_ref (no table lookup, no runtime type check)
(call_ref $closure_fn ...)
```

**Completed:**
- [x] `$Closure` GC types for arities 0-8
- [x] `fn` forms compile to closure structs
- [x] Lambda lifting: identify free variables, capture in env
- [x] Closure invocation via `call_ref`

### P0.2: apply ✓ COMPLETE

Dynamic function invocation with argument vector.

```clojure
(apply + [1 2 3])        ;; → 6
(apply f a b [c d e])    ;; Mixed fixed + rest args
```

**Completed:**
- [x] Parse `apply` special form
- [x] Generate arity-dispatching code (0-8 args via VARIADIC_CLOSURE)

### P0.3: Macros (defmacro) ✓ COMPLETE

Code that writes code, expanded at compile time.

**Implementation:**
- `expand.rs` - MacroEnv, syntax-quote expansion, gensym (`symbol#` → `symbol__N__auto__`)
- `eval.rs` - Tree-walking interpreter with ~30 primitives for compile-time evaluation
- Pipeline: Parse → **Expand** → Analyze → Lower → Codegen

**Built-in core macros:** `when`, `when-not`, `and`, `or`, `cond`, `case`, `->`, `->>`, `when-let`, `if-let`, `doto`, `..`

### P0.4: User-Defined Protocols ✓ COMPLETE

```clojure
(defprotocol IJsonable
  (-to-json [this]))

(extend-type PersistentVector
  IJsonable
  (-to-json [coll] 42))
```

**Built-in method IDs (0-9):** `-lookup`, `-assoc`, `-count`, `-nth`, `-conj`, `-first`, `-rest`, `-seq`, `-hash`, `-equiv`

**User method IDs:** Start at 100+, assigned dynamically per protocol

---

## Phase 1: Core Hash Function ✓ COMPLETE

All HAMT operations require consistent hashing. xxHash32 chosen for WASM efficiency.

**Completed:**
- [x] xxHash32 core in WASM (uses native `i32.rotl`)
- [x] Type-specific hashing for all value types
- [x] `hash-combine` for ordered collections
- [x] `hash-unordered` for maps/sets
- [x] IHash protocol method registration

---

## Phase 2: Protocol System & Polymorphic Dispatch ✓ COMPLETE

**Completed:**
- [x] Type ID system: All GC structs have type_id in field 0
- [x] `get-type-id` function via `ref.test` chain
- [x] Dispatch table infrastructure with `call_indirect`
- [x] Table-based dispatch for all protocol methods
- [x] Polymorphic `nth`, `count`, `first`, `rest`, `get`, `conj`
- [x] `ISeqable/-seq` for vectors, maps, sets (returns IndexedSeq)
- [x] IndexedSeq type for array-backed sequences

### Dispatch Table

| Call Site | Type Known | Dispatch |
|-----------|------------|----------|
| `(nth vec 0)` | Yes (vector) | **Fast**: inline struct access |
| `(nth coll 0)` | No | **Slow**: table lookup via type_id |
| `(-to-json obj)` | Any | **Slow**: user protocol dispatch |

---

## Phase 3: deftype - User-Defined Types

> **Central enabler for self-hosting.** Once deftype works, HAMT nodes and collection algorithms can move to core.sus.
>
> **Dependency:** Requires working array primitives (aget, aset, aclone) for HAMT field access.

### 3.1 Basic deftype (Fields Only) ✓ COMPLETE

```clojure
(deftype Point [x y])

;; Usage:
(def p (->Point 10 20))
(.-x p)  ;; → 10
(.-y p)  ;; → 20
(instance? Point p)  ;; → true
```

**Implementation (completed):**
- [x] Parse `deftype` form in analyzer → `AnalyzedDeftype` struct
- [x] Allocate type ID from user pool (256+) in lowerer
- [x] Generate WASM GC struct type: `(type $Point (struct (field $type_id i32) (field $x eqref) (field $y eqref)))`
- [x] Generate constructor function `->Point` that calls `struct.new`
- [x] Register field names in deftype registry for `.-field` access
- [x] Register type name in deftype registry for `instance?` checks
- [x] Handle type index offsets when user deftypes shift subsequent types
- [x] Fix tail-call optimization in field access and instance? checks

**Key files modified:**
- `analyze.rs` - Added `AnalyzedDeftype`, `DeftypeField`, `analyze_deftype()`
- `ir.rs` - Added `DeftypeDef`, `FieldType`, `DeftypeFieldDef` to Module
- `lower.rs` - Added `UserTypeInfo`, `lower_deftypes()`, `lower_deftype_constructor()`, `lookup_user_field()`
- `codegen.rs` - Emit user struct types after built-in GC types, dynamic type offset calculation
- `lib.rs` - Extract deftype forms from source, handle multi-expression parsing

**Passing tests:**
```clojure
;; Test 1: Constructor and field access
(.-x (->Point 10 20))  ;; → 10

;; Test 2: Multiple fields
(+ (.-x (->Point 10 20)) (.-y (->Point 10 20)))  ;; → 30

;; Test 3: instance? check
(instance? Point (->Point 1 2))  ;; → true
(instance? Point [1 2])  ;; → false
```

### 3.2 deftype with Inline Protocols ✓ COMPLETE

```clojure
(deftype Point [x y]
  IEquiv
  (-equiv [this other]
    (and (instance? Point other)
         (= (.-x this) (.-x other))
         (= (.-y this) (.-y other))))

  IHash
  (-hash [this]
    (hash-combine (hash (.-x this)) (hash (.-y this)))))
```

**Implementation completed:**
- [x] Parse protocol implementations after field vector (reuse extend-type parsing)
- [x] `this` parameter refers to the newly constructed instance
- [x] Generate wrapper functions for each method (same as extend-type)
- [x] Create dispatch table entries mapping type_id → method implementations
- [x] Ensure deftype's type_id is available during protocol method lowering

**Verified working:**
```clojure
;; Protocol method dispatch
(deftype Counter [val] ICounted (-count [this] (.-val this)))
(count (->Counter 42))  ;; → 42

;; IIndexed protocol
(deftype Box [value] IIndexed (-nth [this n] (.-value this)))
(nth (->Box 123) 0)  ;; → 123
```

### 3.3 Reserved Type Indices (Bootstrap Support) ✓ COMPLETE

```clojure
;; Metadata to specify fixed type index for core types
(deftype ^:type-id 5 BitmapIndexedNode [^i32 bitmap arr])
```

This enables core.sus to define HAMT node types at the **same indices** currently hardcoded in `ir.rs`, ensuring backward compatibility during the transition.

**Completed:**
- [x] Parse `^:type-id N` metadata on deftype name
- [x] Use specified index instead of allocating from user pool (gc_type_idx = reserved_type_id)
- [x] Skip emitting GC types for reserved deftypes (they reuse built-in type slots)
- [x] Skip constructor generation for reserved types (use hardcoded struct layouts)
- [x] Added `LoweringMode` to distinguish REPL vs component compilation contexts

**Key implementation notes:**
- Reserved type IDs use `^:type-id N` syntax (simpler than `^{:type-id N}`)
- Types with reserved IDs < NUM_GC_TYPES (39) reuse built-in GC type slots
- Field access uses hardcoded mappings for reserved types (not user field lookup)
- Both REPL and component modes use the same function index offset (runtime helpers are emitted in both)

### 3.4 Implementation Checklist

| Sub-phase | Blocking? | Status |
|-----------|-----------|--------|
| 3.1 Basic deftype | Yes - enables Phase 4 | ✓ COMPLETE |
| 3.2 Inline protocols | No - extend-type works | ✓ COMPLETE |
| 3.3 Reserved indices | Yes - enables Phase 4 | ✓ COMPLETE |

---

## Phase 4: Bootstrap HAMT Nodes in core.sus ✓ COMPLETE

> **Dependency:** Requires Phase 3.1 (basic deftype) and Phase 3.3 (reserved type indices).

Move HAMT node types from hardcoded Rust to deftype in core.sus.

### Implementation (Completed)

HAMT node types are now defined in `core.sus` using reserved type IDs:

```clojure
;; core.sus - HAMT node types with reserved type IDs matching ir.rs
(deftype ^:type-id 5 BitmapIndexedNode [^i32 bitmap arr])
(deftype ^:type-id 6 ArrayNode [^i32 cnt arr])
(deftype ^:type-id 7 HashCollisionNode [^i32 hash ^i32 cnt arr])
```

**Key changes:**
- [x] Added deftype declarations to core.sus with `^:type-id N` metadata
- [x] Types reuse GC type indices 5-7 (same as hardcoded ir.rs constants)
- [x] Constructors are NOT generated for reserved types (use existing struct layouts)
- [x] Field access uses hardcoded mappings (not user field lookup)
- [x] All existing HAMT algorithms continue to work unchanged

**Note:** The type definitions in ir.rs (BITMAP_INDEXED_NODE, ARRAY_NODE, HASH_COLLISION_NODE) remain for now - they define the struct layouts. The core.sus deftypes allow `instance?` checks and future protocol implementations.

### Backward Compatibility

The reserved type ID approach ensures:
1. Existing `instance?` checks work (same GC type indices)
2. Field access via `.-bitmap`, `.-arr`, etc. uses hardcoded mappings
3. All 207+ existing tests pass without modification

---

## Phase 5: Protocol Impls in core.sus ✓ COMPLETE

> **Dependency:** Requires Phase 4 (HAMT nodes as deftype).
> **CRITICAL:** This phase must come BEFORE algorithm migration (old Phase 5, now Phase 6).
>
> **Why this order matters:** The Rust protocol wrappers (`VEC_NTH`, `VEC_CONJ`, etc.) pass raw i32 values
> to helper functions. Core.sus functions expect boxed eqref values. If we try to remove helpers first,
> we get type mismatches. By moving protocol impls to core.sus first, they naturally use boxed values,
> and then the raw helper versions become unnecessary.

Move the protocol implementation wrapper functions from Rust to `extend-type` in core.sus.

### Current State (codegen.rs protocol_impl_funcs)
```rust
VEC_NTH, VEC_COUNT, VEC_CONJ, VEC_FIRST, VEC_REST,
CONS_FIRST, CONS_REST, CONS_COUNT, CONS_NTH,
MAP_COUNT, MAP_LOOKUP,
SET_COUNT, SET_CONTAINS, SET_CONJ
```

### Target State (core.sus)
```clojure
(extend-type PersistentVector
  ICounted
  (-count [v] (.-cnt v))

  IIndexed
  (-nth [v n]
    (aget (array-for v n) (bit-and n 31)))

  ICollection
  (-conj [v val]
    ;; Full vector conj implementation
    ...))

(extend-type PersistentMap
  ICounted
  (-count [m] (.-cnt m))

  ILookup
  (-lookup [m k]
    (let [root (.-root m)]
      (if (nil? root)
        nil
        (inode-find root 0 (hash k) k nil))))

  IAssociative
  (-assoc [m k v]
    ...))

(extend-type PersistentSet
  ICounted
  (-count [s] (.-cnt s))

  ILookup
  (-lookup [s k]
    ...)

  ICollection
  (-conj [s v]
    ...))

(extend-type Cons
  ISeq
  (-first [c] (.-first c))
  (-rest [c] (.-rest c))

  ICounted
  (-count [c]
    (loop [n 0, c c]
      (if (nil? c) n (recur (+ n 1) (.-rest c))))))
```

**Progress:**
- [x] Add `extend-type` declarations to core.sus for PersistentVector (IIndexed/-nth, ISeq/-first/-rest)
- [x] Add built-in type deftypes to core.sus (Cons, PersistentVector, PersistentMap, PersistentSet)
- [x] Enable constructor generation for reserved deftypes (`->PersistentVector`, etc.)
- [x] Add `acopy` primitive for efficient array copying (WASM array.copy)
- [x] Migrate VEC_CONJ to core.sus with helper functions (`-vec-conj-overflow`, `-vec-conj-push`)
- [x] Remove `Expr::VecConj` from ir.rs and `generate_vec_conj` from codegen.rs
- [x] Update `generate_vec_new_large` to use protocol dispatch for large vector literals
- [x] Migrate SET_CONJ to core.sus via `-conj` protocol dispatch
- [x] Migrate MAP_ASSOC to core.sus via `-assoc` protocol dispatch
- [x] Remove `Expr::MapAssoc`, `Expr::SetConj`, `Expr::MapGet`, `Expr::SetContains` from ir.rs
- [x] Remove corresponding codegen functions (~400 lines of hardcoded WASM)
- [x] Collection literal desugaring via inline `conj`/`assoc` calls (temporary - see note)
- [x] Remove remaining protocol_impl_funcs from codegen.rs (NUM_PROTOCOL_IMPLS=0, module removed)
- [x] Dispatch table fully populated from core.sus extend-type declarations

**Note: Temporary Literal Desugaring**

Collection literals currently desugar to inline nested calls:
- `[1 2 3]` → `(conj (conj (conj [] 1) 2) 3)`
- `{1 2}` → `(assoc {} 1 2)`
- `#{1 2}` → `(conj (conj #{} 1) 2)`

This is temporary because multi-arity `defn` isn't yet supported. Once variadic functions (`& args`) are implemented, the desugaring should change to:
- `[1 2 3]` → `(vector 1 2 3)` where `(defn vector [& args] (reduce conj [] args))`
- `{1 2}` → `(hash-map 1 2)` where `(defn hash-map [& kvs] (apply assoc {} kvs))`
- `#{1 2}` → `(hash-set 1 2)` where `(defn hash-set [& args] (reduce conj #{} args))`

**Acceptance tests:**
```clojure
;; Test 1: Vector protocols from core.sus
(count [1 2 3])  ;; → 3
(nth [10 20 30] 1)  ;; → 20
(conj [1 2] 3)  ;; → [1 2 3]
(first [1 2 3])  ;; → 1
(rest [1 2 3])  ;; → (2 3)

;; Test 2: Map protocols from core.sus
(count {:a 1 :b 2})  ;; → 2
(get {:a 1} :a)  ;; → 1
(assoc {:a 1} :b 2)  ;; → {:a 1 :b 2}

;; Test 3: Set protocols from core.sus
(count #{1 2 3})  ;; → 3
(contains? #{1 2} 1)  ;; → true
(conj #{1 2} 3)  ;; → #{1 2 3}

;; Test 4: Cons protocols from core.sus
(first (cons 1 nil))  ;; → 1
(rest (cons 1 (cons 2 nil)))  ;; → (2)
(count (cons 1 (cons 2 nil)))  ;; → 2

;; Test 5: Polymorphic dispatch works
(let [colls [[1 2 3] {:a 1} #{1 2}]]
  (map count colls))  ;; → (3 2 2)
```

---

## Phase 6: Pure Suss Collection Algorithms ✓ COMPLETE

> **Dependency:** Requires Phase 5 (protocol impls in core.sus).
> **Note:** All collection algorithm helpers have been migrated to core.sus.

Move collection algorithm helpers from hardcoded Rust to pure Suss in core.sus.

### Current State (codegen.rs helper_funcs)

**Vector Trie Operations (4 functions):**
```rust
VEC_TAIL_OFF    // Calculate tail offset from count
VEC_NEW_PATH    // Create new trie path during growth
VEC_ARRAY_FOR   // Navigate trie to find leaf array
VEC_PUSH_TAIL   // Add element to tail, growing tree if needed
```

**HAMT Operations (16 functions):**
```rust
// Core HAMT algorithms
HAMT_BITPOS     // Calculate bit position from hash
HAMT_INDEX      // Calculate array index using popcount

// Find operations
INODE_FIND      // Type dispatch for find
BIN_FIND        // BitmapIndexedNode find
AN_FIND         // ArrayNode find
HCN_FIND        // HashCollisionNode find

// Assoc operations
INODE_ASSOC     // Type dispatch for assoc
BIN_ASSOC       // BitmapIndexedNode insert
AN_ASSOC        // ArrayNode insert
HCN_ASSOC       // HashCollisionNode insert
CREATE_NODE     // Create appropriate node type

// Dissoc operations
INODE_DISSOC    // Type dispatch for dissoc
BIN_DISSOC      // BitmapIndexedNode remove
AN_DISSOC       // ArrayNode remove
HCN_DISSOC      // HashCollisionNode remove
```

### Target State (core.sus)

Many of these are already in core.sus but have duplicate hardcoded versions:

```clojure
;; Already in core.sus (need to remove hardcoded duplicates):
(defn tail-off [cnt] ...)
(defn new-path [level node] ...)
(defn array-for [v i] ...)
(defn push-tail [v level parent tailnode] ...)

(defn hamt-mask [hash shift] ...)
(defn hamt-bitpos [hash shift] ...)
(defn hamt-index [bitmap bit] ...)

(defn bin-find [node shift hash key not-found] ...)
(defn an-find [node shift hash key not-found] ...)
(defn hcn-find [node hash key not-found] ...)
(defn inode-find [node shift hash key not-found] ...)

;; Need to add (currently hardcoded only):
(defn bin-assoc [node shift hash key val added-leaf] ...)
(defn an-assoc [node shift hash key val added-leaf] ...)
(defn hcn-assoc [node hash key val added-leaf] ...)
(defn inode-assoc [node shift hash key val added-leaf] ...)
(defn create-node [shift key1 val1 key2 val2] ...)

(defn bin-dissoc [node shift hash key] ...)
(defn an-dissoc [node shift hash key] ...)
(defn hcn-dissoc [node hash key] ...)
(defn inode-dissoc [node shift hash key] ...)
```

### Migration Challenge

The challenge is that protocol impls (Phase 5) call these helpers:
- `VEC_CONJ` calls `tail-off`, `new-path`, `push-tail`
- `VEC_NTH` calls `array-for`
- `MAP_LOOKUP` calls `inode-find`
- `MAP_ASSOC` calls `inode-assoc`
- `SET_CONJ` calls `inode-assoc`

**Solution:** After Phase 5, protocol impls are in core.sus and naturally use the core.sus helper functions. Then we can delete the hardcoded duplicates.

### Tasks

**Phase 6a - Remove duplicate helpers (already in core.sus):**
- [x] VEC_CONJ migrated to core.sus, hardcoded `generate_vec_conj` removed (~350 lines)
- [x] SET_CONJ migrated, hardcoded `generate_set_conj` removed (~150 lines)
- [x] MAP_ASSOC migrated, hardcoded `generate_map_assoc` removed (~150 lines)
- [x] MAP_GET removed, now uses protocol dispatch (~80 lines)
- [x] SET_CONTAINS removed, now uses protocol dispatch (~100 lines)
- [x] All deprecated helper_funcs stubs removed (now just HASH_STRING and GET_TYPE_ID)
- [x] Remove `VEC_TAIL_OFF`, `VEC_NEW_PATH`, `VEC_ARRAY_FOR`, `VEC_PUSH_TAIL` from protocol callers (already done - only in comments)
- [x] Remove `HAMT_BITPOS`, `HAMT_INDEX` from protocol callers (already done - only in comments)
- [x] Remove `INODE_FIND`, `BIN_FIND`, `AN_FIND`, `HCN_FIND` from protocol callers (already done - only in comments)

**Phase 6b - Add assoc helpers to core.sus:** ✓ COMPLETE
- [x] Implement `bin-assoc`, `inode-assoc` in core.sus
- [x] Implement `an-assoc`, `hcn-assoc`, `create-node` (for ArrayNode and HashCollisionNode)
- [x] Remove hardcoded versions from codegen.rs

**Phase 6c - Add dissoc helpers to core.sus:** ✓ COMPLETE
- [x] Implement `bin-dissoc`, `an-dissoc`, `hcn-dissoc`, `inode-dissoc`
- [x] Implement `dissoc` and `disj` user-facing functions
- [x] Update lowerer to call core.sus functions instead of stub codegen
- [x] Remove hardcoded versions from codegen.rs (MapDissoc, SetDisj IR types removed)

### Acceptance Tests

```clojure
;; All existing collection tests must pass
(conj [1 2 3] 4)  ;; → [1 2 3 4]
(nth [10 20 30] 1)  ;; → 20
(get {:a 1} :a)  ;; → 1
(assoc {:a 1} :b 2)  ;; → {:a 1 :b 2}
(dissoc {:a 1 :b 2} :a)  ;; → {:b 2}
(conj #{1 2} 3)  ;; → #{1 2 3}
(disj #{1 2 3} 2)  ;; → #{1 3}

;; Large collections (stress tests)
(count (loop [v [] i 0] (if (< i 1000) (recur (conj v i) (+ i 1)) v)))  ;; → 1000
```

---

## Phase 7: Minimize Compiler ✓ COMPLETE

> **Dependency:** Requires Phases 4-6 complete (all behaviors in core.sus).
> This phase removes now-dead Rust code after self-hosting migration.

### Current State

**codegen.rs:** 5,412 lines (~23% reduction from ~7,000 already achieved)

The major cleanup work was completed during Phases 5-6:
- `helper_funcs` module reduced from 25 functions to 2 irreducible helpers (HASH_STRING, GET_TYPE_ID)
- `protocol_impl_funcs` module removed entirely (NUM_PROTOCOL_IMPLS=0)
- Collection generation functions removed (~800+ lines of hardcoded WASM)

### Completed

**Runtime helper cleanup:**
- [x] `helper_funcs` reduced to 2 irreducible helpers (HASH_STRING, GET_TYPE_ID)
- [x] Vector trie helpers (tail-off, new-path, array-for, push-tail) removed - now in core.sus
- [x] HAMT helpers (inode-find, bin-find, inode-assoc, bin-assoc, etc.) removed - now in core.sus
- [x] Collection algorithm helpers (generate_vec_conj, etc.) removed - now in core.sus

**Protocol implementations:**
- [x] `protocol_impl_funcs` module removed (NUM_PROTOCOL_IMPLS=0)
- [x] All protocol dispatch via core.sus `extend-type` declarations

**ir.rs cleanup:**
- [x] Deprecated field constants removed (~60 lines): PV_*, CONS_*, PM_*, PS_*, BIN_*, AN_*, HCN_*, VC_FN0-8

### Quick Wins ✓ COMPLETE

**Dead code removed:**
- [x] `emit_protocol_impl_function_decls()` and `emit_protocol_impl_functions()` - empty functions
- [x] Call sites in `generate_standalone_module` and `generate_with_wit`
- [x] `variadic_fn_type_for_arity()` - deprecated function

**Deprecated aliases updated to canonical names:**
- [x] `LARGE_INT` → `INT64`, `FLOAT` → `FLOAT64`, `TRIE_NODE` → `ARRAY`
- [x] `LI_VALUE` → `I64_VALUE`, `FL_VALUE` → `F64_VALUE`
- [x] All comments updated to use new names

### Constants That Cannot Be Removed Yet

These gc_types constants are still actively used in lower.rs and codegen.rs:
- `CONS` - field indexing for cons cells
- `BITMAP_INDEXED_NODE`, `ARRAY_NODE`, `HASH_COLLISION_NODE` - field access mappings
- `PERSISTENT_VECTOR`, `PERSISTENT_MAP`, `PERSISTENT_SET` - dispatch table indices

Removing these requires refactoring to resolve field indices dynamically from DeftypeDef.

### What Remains in Compiler

The compiler now provides only:
1. **Primitive value boxing** - i31ref, LARGE_INT, FLOAT, STRING
2. **Generic struct.new/get** - via deftype
3. **Array primitives** - make-array, aget, aset, alength, aclone
4. **Closure machinery** - fn, apply, call_ref
5. **Protocol dispatch** - get-type-id, dispatch table, call_indirect
6. **Collection literal → struct.new** - shape only, behavior from core.sus
7. **Hash functions** - HASH_STRING (irreducible - needs native string access)
8. **Type dispatch** - GET_TYPE_ID (irreducible - uses ref.test chain)

### Validation

**Metric:** Lines of code in `codegen.rs`
- Original: ~7,000 lines
- Current: ~5,380 lines (~23% reduction)

**All existing tests pass** - behavior unchanged, just moved to core.sus.

---

## Phase 7b: WIT Code Consolidation ✓ COMPLETE

> **Goal:** Reduce code duplication between WIT and non-WIT code generation paths.
> **Achieved:** ~400 lines removed (450 deleted, 50 added for marshaling fixes)

### Background

The compiler had nearly identical code paths for WIT-exported functions vs internal functions. The key insight was that `generate_expr_inner` already takes a `param_offset` parameter that handles the difference.

### What Was Done

**Deleted legacy WIT-specific functions (~450 lines):**
- `generate_expr_wit()` - Wrapper for WIT expression generation
- `generate_expr_wit_inner()` - Duplicate of generate_expr_inner with param_offset
- `generate_binop_wit()` - Duplicate binary op handling
- `generate_unop_wit()` - Duplicate unary op handling
- `generate_if_wit()` - Duplicate conditional handling
- `generate_condition_wit()` - Duplicate condition evaluation

**Updated unified path:**
- `generate_function_wit()` now calls `generate_expr_with_offset()` instead of deleted `generate_expr_wit()`
- Added `param_offset` to `LocalSet` in `Let`, `Loop`, and `Recur` expression handling
- Added WIT marshaling for `Call` and `TailCall` when calling exported functions from WIT context

### Architecture After Consolidation

```
generate_function()      → generate_expr() → generate_expr_inner(param_offset=0)
generate_function_wit()  → generate_expr_with_offset() → generate_expr_inner(param_offset=N)
```

The only remaining WIT-specific code is in `generate_function_wit()`:
- Entry marshaling: Convert WIT params (i32/i64/f64) to boxed eqref
- Exit marshaling: Convert boxed eqref result back to WIT type
- Local layout with dual param slots

### Call Site Marshaling

When calling an exported function from within WIT context (e.g., recursive calls to `^:export` functions), the unified `generate_expr_inner` now:
1. Detects if target is an exported function (`func.exported`)
2. Unwraps eqref args to i32 before the call
3. Wraps i32 result back to eqref after the call

This was previously handled by the deleted `generate_expr_wit_inner()`.

### Metrics

| Metric | Before | After |
|--------|--------|-------|
| codegen.rs lines | ~5,400 | ~5,000 |
| `_wit` functions | 6 | 0 (deleted) |
| Lines changed | - | -450, +50 |

### Success Criteria ✓

- [x] codegen.rs reduced by ~400 lines
- [x] All 246 tests pass (237 compile_expr + 6 component + 3 conformance)
- [x] WIT export functionality unchanged
- [x] TCO with WIT exports now works (was a known bug)

---

## Phase 8: Cross-Namespace Require System ✓ MOSTLY COMPLETE

> **Goal:** Enable Clojure-style code organization across multiple files with proper dependency resolution.

### Completed (Compile-Time Support)

**Phase 8.1: Data Structures** ✓
- [x] `NamespaceInfo`, `NamespaceRegistry` for tracking namespaces
- [x] `AnalyzedRequire`, `RequireSource` for require parsing
- [x] `PublicDef`, `DefKind` for export tracking

**Phase 8.2: File Resolution** ✓
- [x] `ns_to_path()` - Convert namespace to file path (Clojure convention)
- [x] `path_to_ns()` - Convert file path back to namespace
- [x] Support for multiple source paths (`--src` flag)

**Phase 8.3: Require Parsing** ✓
- [x] Parse both WASI and Suss namespace requires
- [x] Discrimination: contains `:` → WASI, otherwise → Suss namespace
- [x] Support `:as`, `:refer [...]`, `:refer :all`

**Phase 8.4: Dependency Resolution** ✓
- [x] `DependencyResolver` with topological sort
- [x] Circular dependency detection
- [x] Lazy discovery from entry namespace

**Phase 8.5: Symbol Resolution** ✓
- [x] Namespace aliases (`ns_aliases` HashMap)
- [x] Referred symbols (`referred_symbols` HashMap)
- [x] Resolution chain: current ns → referred → suss.core → unqualified

**Phase 8.6: Multi-File Compilation** ✓
- [x] `compile_with_namespaces()` entry point
- [x] CLI support: `suss compile -n myapp.core -w world.wit`

**Phase 8.7: REPL Runtime Loading** ✓
- [x] `(require '[ns :as alias])` at REPL runtime - loads namespace files
- [x] `(in-ns 'ns)` to switch current namespace - changes REPL prompt
- [x] Definition persistence across expressions via source accumulation
- [x] Dynamic prompt showing current namespace

### Remaining

**Phase 8.8: core.sus as suss.core** ✅
- [x] Add `(ns suss.core)` declaration to core.sus
- [x] Functions registered with namespace-qualified names in file compilation
- [x] Skip `ns` forms in REPL path (extract_core_definitions)
- [x] Backward compatibility maintained via resolution chain

**Phase 8.9: REPL Enhancements** ✓ PARTIALLY COMPLETE
- [x] Core.sus caching - Parse/analyze core.sus once per session via `CoreCache` in `Compiler`
- [x] WASM compilation caching - Hash accumulated source, cache compiled WASM bytes (LRU, 100 entries)
- [x] Tab completion - Symbol completion for builtins, core.sus functions, and user definitions
- [x] Stateful sessions - `SessionState` accumulates defs, manages namespaces, caches compilations
- [ ] Hot reload - Watch source files, auto-reload namespaces on change
- [ ] `*ns*` dynamic var - Clojure-style current namespace binding
- [ ] Definition redefinition - Handle `(defn foo ...)` replacing previous `foo`
- [ ] Incremental parsing - Cache parsed EDN per file to speed up recompilation

### Usage

```clojure
;; src/myapp/core.sus
(ns myapp.core
  (require '[myapp.utils :as utils])
  (require '[wasi:random/random :as random]))

(defn ^:export main []
  (utils/process (random/get-random-u64)))
```

```bash
# Multi-file namespace compilation
suss compile -n myapp.core -w world.wit -o app.wasm

# With custom source paths
suss compile -n myapp.core --src lib --src vendor -w world.wit
```

### Key Files Modified

| File | Changes |
|------|---------|
| `analyze.rs` | `AnalyzedRequire`, `RequireSource`, `NamespaceInfo`, require parsing |
| `lib.rs` | `DependencyResolver`, `ns_to_path()`, `compile_with_namespaces()` |
| `lower.rs` | `ns_aliases`, `referred_symbols`, `resolve_func_name()` |
| `error.rs` | `CyclicDependency`, `IoError` variants |
| `args.rs` | `-n`/`--namespace` and `--src` CLI flags |
| `main.rs` | `compile_namespace()` command handler, stateful REPL |
| `repl.rs` | **NEW** - `ReplState`, `handle_in_ns()`, `handle_require()`, definition persistence |

---

## Phase 9: WIT Boundary Marshaling

Convert between internal GC refs and WIT primitives at export boundaries.

| WIT Type | To GC Ref | From GC Ref |
|----------|-----------|-------------|
| i32 | `(n << 1) \| 1` → `ref.i31` | `i31.get_s >> 1` |
| i64 | `struct.new $LARGE_INT` | `struct.get` |
| f64 | `struct.new $FLOAT` | `struct.get` |
| string | `array.new_data $STRING` | extract bytes |

**Tasks:**
- [ ] Implement marshaling for all primitive types
- [ ] Implement list<T> ↔ vector marshaling
- [ ] Implement record ↔ map marshaling (if needed)

---

## Phase 9: Transient Collections (Performance Optimization)

Mutable "transient" variants for batch construction:

```clojure
(persistent! (conj! (conj! (transient []) 1) 2))
```

**Tasks:**
- [ ] TransientVector with mutable tail
- [ ] TransientHashMap with edit tracking
- [ ] `transient`, `conj!`, `assoc!`, `persistent!` special forms

Low priority - optimization only.

---

## Phase 10: wasm-opt Integration

Run Binaryen's optimizer on compiled output.

**Tasks:**
- [ ] Add `--optimize` / `-O` flag to compile command
- [ ] Shell out to `wasm-opt -O3` on generated `.wasm` files
- [ ] Optionally bundle wasm-opt or require it in PATH

Expected benefit: ~1.9x speedup on WasmGC code (per V8 benchmarks).

---

## Phase 11: WASI CLI Commands ✓ COMPLETE

Functions named `-main` compile to `wasi:cli/run` command components.

```clojure
;; app.sus
(defn -main [] 42)
```

```bash
# Compile to CLI command component
suss compile -m myapp app.sus -o app.wasm

# Run with wasmtime
wasmtime run -W gc,function-references app.wasm
```

**Completed:**
- [x] Detect `-main` in analyzed module via `extract_definitions_for_main`
- [x] Generate CLI_COMMAND_WORLD with proper `wasi:cli/run@0.2.4` export
- [x] Add Result type to IR for WIT result types
- [x] Handle Result in `wit_type_to_ir` and codegen exit marshaling
- [x] Fix `def + fn` handling (expanded `defn` forms) in main extraction
- [x] Interface-only CLI package loading (avoids cross-package `include` issues)
- [x] Disable tail call optimization for Result-returning functions

**Not yet implemented:**
- [ ] Handle args via `wasi:cli/environment.get-arguments` (future enhancement)
- [ ] Support exit codes from `-main` return value (future enhancement)

---

## Phase 12: List/Seq Operations ✓ COMPLETE

Full sequence abstraction for all collections, including lazy sequences and higher-order functions.

### Completed

**Core Sequence Types:**
- [x] `IndexedSeq` - Array-backed sequence for efficient indexed access
- [x] `MapEntry` - Key-value pair with IMapEntry protocol
- [x] `LazySeq` - Lazy sequence with thunk caching (mutable `fn` and `s` fields)

**ISeqable Implementations:**
- [x] `PersistentVector` - Returns IndexedSeq over array copy
- [x] `PersistentMap` - Returns IndexedSeq of MapEntry pairs (HAMT traversal)
- [x] `PersistentSet` - Returns IndexedSeq of elements (HAMT traversal)
- [x] `MapEntry` - Returns IndexedSeq of [key val]
- [x] `Cons` - Already implements ISeq

**User-Facing Functions:**
- [x] `seq`, `first`, `rest`, `next` - Core sequence operations
- [x] `key`, `val` - MapEntry accessors

**Lazy Sequence Support:**
- [x] `^:mutable` field support in `deftype`
- [x] `lazy-seq` macro for deferred evaluation
- [x] Thunk caching in LazySeq type

**Higher-Order Functions:**
- [x] `map`, `filter`, `remove` - Lazy transformations
- [x] `take`, `drop`, `take-while`, `drop-while` - Subsequences
- [x] `reduce` - Eager fold with accumulator
- [x] `iterate`, `repeat`, `repeatedly` - Infinite sequences
- [x] `range` - Lazy numeric range
- [x] `concat2`, `mapcat` - Sequence concatenation

**Numeric Helpers:**
- [x] `inc`, `dec`, `pos?`, `neg?`, `zero?`

---

## Phase 13: Multi-Arity Functions ✓ COMPLETE

Support ClojureScript-style multi-arity functions.

### Syntax

```clojure
;; Multi-arity defn
(defn greet
  ([] "Hello!")
  ([name] (str "Hello, " name "!"))
  ([greeting name] (str greeting ", " name "!")))

;; Multi-arity anonymous fn
(fn ([x] x) ([x y] (+ x y)))
```

### Implementation

Multi-arity functions are compiled to variadic closures with arity dispatch:

1. **Parsing (expand.rs):** Detect multi-arity form when first element after name is a List starting with Vector
2. **Lowering (lower.rs):** Convert to variadic closure with nested if dispatch:
   ```clojure
   (fn [& args]
     (if (= (alength args) 0) body0
       (if (= (alength args) 1) (let [x (aget args 0)] body1)
         (let [x (aget args 0) y (aget args 1)] body2))))
   ```
3. **Codegen:** Reuses existing variadic closure infrastructure

### Completed
- [x] expand.rs: Multi-arity detection in `expand_defn` and `expand_fn`
- [x] expand.rs: `expand_fn_clause` for individual arity clauses with destructuring
- [x] lower.rs: `lower_multi_arity_fn` generates dispatch closure
- [x] All arities 0-N work correctly
- [x] Existing single-arity functions unchanged (237 tests pass)

### Limitations
- Variadic arities within multi-arity (e.g., `([x & more] ...)`) not yet fully supported
- No compile-time arity checking (wrong arity returns nil)

---

## Implementation Order

### Completed Foundation
1. ✓ **Compositional primitives** - closures, apply, macros, protocols
2. ✓ **Hash function** - xxHash32
3. ✓ **Protocol dispatch** - dispatch table, polymorphic operations
4. ✓ **core.sus infrastructure** - auto-loaded, parser fixes
5. ✓ **Array primitives** - aget, aset, alength, aclone, make-array, acopy

### Self-Hosting Phases (Current Focus)

```
Phase 3.1 ──→ Phase 3.3 ──→ Phase 4 ──→ Phase 5 ──→ Phase 6 ──→ Phase 7
(deftype)    (reserved)   (HAMT nodes) (protocols) (algorithms) (cleanup)
              ↓
           Phase 3.2
           (inline protocols)
```

**Why Phase 5 before Phase 6:**
The Rust protocol wrappers pass raw i32 values to helper functions. Core.sus functions expect boxed eqref.
By moving protocol impls to core.sus first (Phase 5), they naturally use boxed values.
Then the raw i32 helper functions become dead code that can be removed (Phase 6).

| Phase | What | Blocking? | Status |
|-------|------|-----------|--------|
| **3.1** | Basic deftype (fields only) | Yes | ✓ COMPLETE |
| **3.2** | deftype with inline protocols | No | ✓ COMPLETE |
| **3.3** | Reserved type indices | Yes | ✓ COMPLETE |
| **4** | HAMT nodes as deftype | Yes | ✓ COMPLETE |
| **5** | Protocol impls in core.sus | Yes | ✓ COMPLETE |
| **6** | Pure Suss algorithms | Incremental | ✓ COMPLETE |
| **7** | Minimize compiler | No | ✓ COMPLETE |
| **7b** | WIT code consolidation | No | ✓ COMPLETE |
| **11** | WASI CLI commands | No | ✓ COMPLETE |
| **12** | List/Seq operations | No | ✓ COMPLETE |
| **13** | Multi-arity functions | No | ✓ COMPLETE |
| **8.9** | REPL enhancements | No | ✓ PARTIAL (caching, completion, sessions done) |

### After Self-Hosting
- ✓ **Phase 7b: WIT consolidation** - Reduced codegen duplication (~400 lines)
- ✓ **Phase 13: Multi-arity functions** - `(defn foo ([x] x) ([x y] (+ x y)))` syntax
- ✓ **Phase 11: WASI CLI** - Command components with `-main` functions
- ✓ **Phase 8.9: REPL enhancements** - Core caching, WASM caching, tab completion, stateful sessions
- **Phase 9: WIT marshaling** - Component boundary type conversion
- **Phase 9b: Transients** - Performance optimization
- **Phase 10: wasm-opt** - Binary optimization

---

## Success Criteria

### Phase 3 Complete When:
- [x] `(->Point 10 20)` creates a user-defined struct
- [x] `(.-x p)` accesses fields on user types
- [x] `(instance? Point p)` works for user types
- [x] `(deftype Foo [...] IBar (-method ...))` compiles and dispatches correctly (Phase 3.2)
- [x] `^:type-id N` reserves specific type indices

### Phase 4 Complete When: ✓ COMPLETE
- [x] `BitmapIndexedNode`, `ArrayNode`, `HashCollisionNode` defined in core.sus
- [x] Types use reserved IDs matching ir.rs constants (5, 6, 7)
- [x] All existing map/set tests still pass (207+ tests passing)

### Phase 5 Complete When: ✓ COMPLETE
- [x] `codegen.rs` `protocol_impl_funcs` module is empty or removed
- [x] All collection protocol methods dispatch via core.sus `extend-type` declarations
- [x] PersistentVector, PersistentMap, PersistentSet, Cons protocols in core.sus

### Phase 6 Complete When: ✓ COMPLETE
- [x] `codegen.rs` `helper_funcs` module has only truly irreducible helpers (HASH_STRING, GET_TYPE_ID)
- [x] All deprecated helper stubs removed
- [x] HAMT assoc helpers in core.sus (bin-assoc, an-assoc, hcn-assoc, inode-assoc, create-node)
- [x] HAMT dissoc helpers in core.sus (bin-dissoc, an-dissoc, hcn-dissoc, inode-dissoc)
- [x] IR types MapDissoc, SetDisj removed (now call core.sus functions)
- [x] Vector trie helpers called from core.sus only (helper_func_idx calls removed)
- [x] HAMT find helpers called from core.sus only (helper_func_idx calls removed)

### Phase 7 Complete When: ✓ COMPLETE
- [x] `helper_funcs` module reduced to irreducible helpers (HASH_STRING, GET_TYPE_ID)
- [x] `protocol_impl_funcs` module removed
- [x] `codegen.rs` reduced by ~25% (~5,300 lines from ~7,000)
- [x] All existing tests pass without modification
- [x] New collection operations can be added purely in core.sus
- [x] Dead code and deprecated aliases removed

### Phase 12 Complete When: ✓ COMPLETE
- [x] `(seq [1 2 3])` returns IndexedSeq
- [x] `(first [1 2 3])` => 1
- [x] `(rest [1 2 3])` => (2 3)
- [x] `(seq {:a 1})` returns seq of MapEntry
- [x] `(key (first {:a 1}))` => :a
- [x] `(seq #{1 2})` returns seq of elements
- [x] `(take 3 (map inc [1 2 3 4 5]))` => (2 3 4) via lazy evaluation
- [x] `(reduce + 0 [1 2 3])` => 6
- [x] `(filter pos? [-1 0 1 2])` returns lazy seq of positive numbers
- [x] `^:mutable` fields work in deftype for LazySeq thunk caching

### Phase 13 Complete When: ✓ COMPLETE
- [x] `(defn foo ([x] x) ([x y] (+ x y)))` compiles and runs
- [x] `(fn ([x] x) ([x y] (+ x y)))` anonymous multi-arity works
- [x] 0-arity clauses work: `(fn ([] 0) ([x] x))`
- [x] Multiple fixed arities dispatch correctly
- [x] Existing single-arity functions unchanged

### Ultimate Success:
```clojure
;; This should work with ZERO compiler changes:
(defprotocol IMyCollection
  (-my-op [coll]))

(deftype MyQueue [head tail]
  IMyCollection
  (-my-op [q] ...))

(extend-type PersistentVector
  IMyCollection
  (-my-op [v] ...))
```

---

## References

### WASM Specifications

**WASM 3.0 (targeting):**
- [WASM 3.0 Core Specification](https://webassembly.github.io/spec/core/bikeshed/) - Full spec including GC and function references
- [Typed Function References Proposal](https://github.com/WebAssembly/spec/blob/wasm-3.0/proposals/function-references/Overview.md) - `ref.func`, `call_ref`, typed funcref
- [GC Proposal](https://github.com/WebAssembly/gc/blob/main/proposals/gc/Overview.md) - Structs, arrays, i31ref

**Key features for Suss:**
| Feature | WASM Instructions | Used For |
|---------|------------------|----------|
| Typed function refs | `ref.func`, `call_ref` | First-class functions, closures |
| GC structs | `struct.new`, `struct.get` | Closures, persistent collections |
| GC arrays | `array.new`, `array.get` | Vectors, HAMT nodes |
| i31ref | `ref.i31`, `i31.get_s` | Small ints, nil/bool sentinels |

### ClojureScript Reference Implementation

ClojureScript is included as a Git submodule in `clojurescript/`. Key source locations:
- `clojurescript/src/main/clojure/cljs/core.cljc` - All persistent collections
- `PersistentVector` - lines ~4000-4400
- `PersistentHashMap` - lines ~5000-5800
- `BitmapIndexedNode` - lines ~5100-5400
- `ArrayNode` - lines ~5400-5550
- `HashCollisionNode` - lines ~5550-5700
- `m3-hash-*` functions - lines ~800-900
- Protocol definitions - lines ~500-700 (ISeq, ILookup, IAssociative, etc.)
