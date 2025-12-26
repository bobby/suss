# Suss Roadmap

> **Suss requires WASM GC.** All execution uses WASM GC types (structs, arrays, i31ref). There is no non-GC fallback mode.

This roadmap aligns with ClojureScript's proven persistent data structure implementations.

---

## 🎯 Priority Zero: Compositional Primitives

> **Goal:** Stop modifying the compiler for each new feature. Build the primitives that let Suss extend itself.

### The Problem (SOLVED ✓)

Previously, every new control-flow construct required Rust code changes:

```
lower_cond()    → 50 lines of Rust in lower.rs
lower_when()    → 30 lines of Rust in lower.rs
lower_and()     → 30 lines of Rust in lower.rs
lower_case()    → 55 lines of Rust in lower.rs
```

In Clojure, these are all **macros** - code that writes code, written in Clojure itself.

**Solution implemented:** ClojureScript-style compile-time macros with:
- `expand.rs` - Macro expansion phase with syntax-quote, gensym, defmacro parsing
- `eval.rs` - Tree-walking interpreter for evaluating macro bodies at compile time
- Built-in core macros: `when`, `when-not`, `and`, `or`, `cond`, `case`
- All `lower_xxx` functions for these forms have been removed from Rust

### The Solution: Four Fundamental Primitives

Once we have these, new features become library code, not compiler changes:

| Primitive | Enables | Status |
|-----------|---------|--------|
| **First-class functions** | `map`, `filter`, `reduce`, higher-order programming | ✓ COMPLETE |
| **apply** | `(apply + [1 2 3])`, variadic dispatch | ✓ COMPLETE |
| **Macros** | `cond`, `when`, `->`, `for`, `core.async` | ✓ COMPLETE |
| **User protocols** | `defprotocol`, `extend-type`, abstraction | 🔶 Partial |

### P0.1: First-Class Functions (Closures)

Functions must become **values** that can be passed, returned, and stored.

**Key WASM features:** WASM 3.0 Typed Function References (`ref.func`, `call_ref`) enable efficient first-class functions without runtime type checks. See [References](#references) below.

**Current limitation:**
```clojure
;; This doesn't work - functions aren't values
(let [f (if condition + -)]
  (f 1 2))

(map inc [1 2 3])  ;; Can't pass `inc` as argument
```

**WASM 3.0 implementation using Typed Function References:**
```wasm
;; Closure struct: captures environment + typed function reference
(type $closure_fn (func (param (ref eq)) (param eqref) (result eqref)))
(type $Closure (struct
  (field $env (ref eq))              ;; captured variables (array or struct)
  (field $fn (ref $closure_fn))))    ;; typed funcref - no runtime check needed

;; Create closure with ref.func (non-null typed reference)
(struct.new $Closure
  (local.get $env)
  (ref.func $my_function))           ;; typed, non-null function reference

;; Call closure with call_ref (no table lookup, no runtime type check)
(call_ref $closure_fn
  (struct.get $Closure $env ...)
  (local.get $arg)
  (struct.get $Closure $fn ...))
```

**Advantages over call_indirect:**
- No dispatch table needed for closures
- No runtime type check (typed references)
- Non-null references eliminate null checks
- Functions don't need table slots to be callable

**Required changes:**
- [x] Add `$Closure` GC type for function values (CLOSURE_0 through CLOSURE_8 for arities)
- [x] `fn` forms compile to closure structs (not just WASM functions)
- [x] Function application checks: is callee a closure? → extract fn + env, call
- [x] Lambda lifting: identify free variables, capture in env struct
- [ ] Implement `IFn` protocol with `-invoke` method (closures work without protocol)

**IR additions:**
```rust
// New Expr variants
ClosureNew { func_idx: u32, captures: Vec<Expr> },
ClosureCall { closure: Box<Expr>, args: Vec<Expr> },
```

### P0.2: apply

Dynamic function invocation with argument list.

```clojure
(apply + [1 2 3])        ;; → 6
(apply f args)           ;; Call f with elements of args
(apply f a b [c d e])    ;; Mixed fixed + rest args
```

**Implementation approach:**
1. For known arities: generate dispatch based on collection length
2. For IFn protocol: call `-invoke` with appropriate arity
3. Requires first-class functions (P0.1)

**Required changes:**
- [x] Parse `apply` special form
- [x] Generate arity-dispatching code (0-8 args via VARIADIC_CLOSURE)
- [ ] Handle rest args (`& more` in fn signatures)

**Variadic operator semantics (ClojureScript-compatible):**
- `(+)` → 0, `(*)` → 1 (identity elements)
- `(- x)` → negation, `(/ x)` → reciprocal
- `(apply + [1 2 3])` → 6

### P0.3: Macros (defmacro)

Code that writes code, expanded at compile time.

**Goal:** Move these OUT of `lower.rs` and INTO Suss:
```clojure
(defmacro when [test & body]
  `(if ~test (do ~@body) nil))

(defmacro cond [& clauses]
  (when (seq clauses)
    `(if ~(first clauses)
       ~(second clauses)
       (cond ~@(nnext clauses)))))

(defmacro -> [x & forms]
  (loop [x x, forms forms]
    (if forms
      (let [form (first forms)
            threaded (if (seq? form)
                       `(~(first form) ~x ~@(next form))
                       (list form x))]
        (recur threaded (next forms)))
      x)))
```

**Implementation phases:**

**Phase A: Quote & Syntax-Quote** ✓ COMPLETE
- [x] `quote` - prevent evaluation: `'(+ 1 2)` → list, not 3
- [x] `syntax-quote` (`) - quasi-quote with namespace resolution
- [x] `unquote` (~) - evaluate inside syntax-quote
- [x] `unquote-splicing` (~@) - splice collection

**Phase B: Macro Expansion** ✓ COMPLETE
- [x] `defmacro` form in analyzer
- [x] Macro functions stored in compile-time environment
- [x] Expand macros before lowering to IR
- [x] Support recursive macro expansion

**Phase C: Bootstrap Core Macros** (Partial)
- [x] Implement `when`, `when-not` as macros
- [x] Implement `cond`, `case` as macros
- [x] Implement `and`, `or` as macros
- [x] Remove corresponding `lower_xxx` functions from Rust
- [ ] Implement `when-let`, `condp` as macros
- [ ] Implement `->`, `->>`, `as->` as macros
- [ ] Implement `for`, `doseq` as macros

**The payoff:** After Phase C, `core.async` becomes possible as a library.

### P0.4: User-Defined Protocols

Allow users to define their own abstractions.

```clojure
(defprotocol IJsonable
  (-to-json [this]))

(extend-type PersistentVector
  IJsonable
  (-to-json [this]
    (str "[" (clojure.string/join "," (map -to-json this)) "]")))
```

**Current state:** Protocol dispatch infrastructure exists, but:
- Protocol definitions are hardcoded in `ir.rs`
- Users cannot define new protocols
- Users cannot extend existing types

**Required changes:**
- [ ] Parse `defprotocol` → assign method IDs (100+)
- [ ] Parse `extend-type` / `extend-protocol`
- [ ] Generate dispatch table entries for user extensions
- [ ] Support extending built-in types with user protocols

### Dependency Graph

```
                    ┌─────────────────┐
                    │ User Protocols  │ ← enables abstraction
                    └────────┬────────┘
                             │
              ┌──────────────┴──────────────┐
              │                             │
              ▼                             ▼
     ┌─────────────────┐           ┌─────────────────┐
     │     Macros      │           │     apply       │
     └────────┬────────┘           └────────┬────────┘
              │                             │
              │   requires                  │ requires
              │                             │
              └──────────────┬──────────────┘
                             │
                             ▼
                  ┌─────────────────────┐
                  │ First-Class Fns     │ ← foundation
                  │ (Closures)          │
                  └─────────────────────┘
```

### Implementation Order

1. **First-class functions** - Everything else depends on this
2. **apply** - Needed for variadic macros
3. **Quote/Syntax-quote** - Needed for macro bodies
4. **defmacro** - Compile-time expansion
5. **Bootstrap core macros** - Move `lower_xxx` to Suss
6. **User protocols** - Can be parallel with 3-5

### Success Criteria ✓ ACHIEVED

**Before:** Adding `when-some` required:
- Modify `lower.rs` (add `lower_when_some` function)
- Add pattern match in `lower_call`
- Rebuild compiler

**After (now implemented!):** Adding `when-some` requires:
```clojure
(defmacro when-some [[sym expr] & body]
  `(let [val# ~expr]
     (when (some? val#)
       (let [~sym val#]
         ~@body))))
```

No compiler changes. Just library code.

**Implementation details:**
- `expand.rs` - MacroEnv, MacroDef, syntax-quote expansion, gensym (`symbol#` → `symbol__N__auto__`)
- `eval.rs` - Tree-walking interpreter with ~30 primitives for compile-time macro evaluation
- Pipeline: Parse → **Expand** → Analyze → Lower → Codegen

### The IR After Compositional Primitives

The IR stays **small and fixed**:

```rust
enum Expr {
    // Literals
    Unit, Bool, Int, Float, String,

    // Variables
    LocalGet, LocalSet, GlobalGet, GlobalSet,

    // Control flow (irreducible - can't be macros)
    If, Loop, Recur, Block,

    // Functions
    Call,           // static call by index
    TailCall,       // TCO static call
    ClosureNew,     // NEW: create closure value
    ClosureCall,    // NEW: call closure value
    Apply,          // NEW: dynamic invocation

    // GC operations
    StructNew, StructGet, ArrayNew, ArrayGet, ArraySet, ...

    // Collections (protocol-dispatched)
    ProtocolDispatch { obj, method_id, args },

    // Types
    Coerce, RefTest, RefNull, RefIsNull,
}
```

**What's NOT in the IR:**
- `cond` → macro, expands to nested `if`
- `when` → macro, expands to `if`
- `and`/`or` → macros, expand to `if` + `let`
- `->` → macro, expands to nested calls
- `for` → macro, expands to `loop`/`recur`

The compiler handles ~25 IR node types. Everything else is library code.

---

## Phase 1: Core Hash Function (Prerequisite) ✓ COMPLETE

All HAMT operations require consistent hashing.

### Algorithm Selection: xxHash32

**Why xxHash32 over alternatives:**

| Hash | WASM Suitability | Quality | Notes |
|------|-----------------|---------|-------|
| **xxHash32** ✓ | Excellent | Excellent | Pure 32-bit ops, WASM has native `i32.rotl` |
| wyhash | Poor | Excellent | Requires 128-bit multiply (slow in WASM) |
| wyhash (32-bit mode) | Okay | Good | Different hashes than 64-bit, breaks consistency |
| Murmur3-32 | Good | Good | Slower than xxHash32 for same quality |
| FxHash | Excellent | Poor | Bad distribution for inputs >16 bytes |

**WASM considerations:**
- 128-bit widening multiply is emulated (2-7x slower than native)
- wyhash relies on folded 128-bit multiply as its core operation
- xxHash32 uses only 32-bit ops + rotl (native WASM instruction)
- Proven: xxhash-wasm shows 90x speedup over JS implementation

**References:**
- [xxHash official](https://xxhash.com/) - passes all SMHasher tests
- [xxhash-wasm](https://github.com/jungomi/xxhash-wasm) - proven WASM implementation
- [Rust hash benchmarks](https://medium.com/@tprodanov/benchmarking-non-cryptographic-hash-functions-in-rust-2e6091077d11)
- [WASM 128-bit multiply issue](https://github.com/WebAssembly/design/issues/1522)

### xxHash32 Implementation

Core algorithm (from xxHash spec):
```
PRIME32_1 = 0x9E3779B1
PRIME32_2 = 0x85EBCA77
PRIME32_3 = 0xC2B2AE3D
PRIME32_4 = 0x27D4EB2F
PRIME32_5 = 0x165667B1

xxh32(input, seed):
  if len >= 16:
    // Process 16-byte chunks with 4 accumulators
    v1 = seed + PRIME32_1 + PRIME32_2
    v2 = seed + PRIME32_2
    v3 = seed
    v4 = seed - PRIME32_1
    for each 16-byte chunk:
      v1 = round(v1, chunk[0..4])
      v2 = round(v2, chunk[4..8])
      v3 = round(v3, chunk[8..12])
      v4 = round(v4, chunk[12..16])
    acc = rotl(v1,1) + rotl(v2,7) + rotl(v3,12) + rotl(v4,18)
  else:
    acc = seed + PRIME32_5

  acc += len
  // Process remaining bytes
  // Final avalanche
  acc ^= acc >> 15
  acc *= PRIME32_2
  acc ^= acc >> 13
  acc *= PRIME32_3
  acc ^= acc >> 16
  return acc

round(acc, input):
  acc += input * PRIME32_2
  acc = rotl(acc, 13)
  acc *= PRIME32_1
  return acc
```

### Type-Specific Hashing

```
hash(value) → i32

Type dispatch:
- nil      → 0
- boolean  → true=1231, false=1237 (Java convention)
- i31ref int → value itself (already well-distributed)
- i64      → xxh32(bytes, 0)
- f64      → xxh32(bit-repr, 0)
- string   → xxh32(utf8-bytes, 0)
- keyword  → cached hash from interned string
- symbol   → xxh32(name-bytes, namespace-hash)
- vector   → ordered hash combining element hashes
- map      → unordered hash of entry hashes
- set      → unordered hash of element hashes
```

### Implementation Tasks

- [x] Implement xxHash32 core in WASM (use `i32.rotl` instruction)
- [x] `hash-bytes` - xxHash32 over byte array (for strings)
- [x] `hash-i64` - hash 64-bit integer
- [x] `hash-f64` - hash float via bit representation
- [x] `hash-combine` - for ordered collections: `rotl(h1, 5) ^ (h2 * PRIME32_1)`
- [x] `hash-unordered` - for maps/sets: `(+ h1 h2)` (commutative)
- [x] Type dispatch wrapper matching i31ref encoding
- [x] IHash protocol method registration for all built-in types

### Seed Strategy

Use seed=0 for deterministic hashing across all platforms. This ensures:
- Same value → same hash on all WASM runtimes
- Reproducible behavior for tests
- No HashDoS protection (acceptable for non-adversarial inputs)

---

## Phase 2: Protocol System & Polymorphic Dispatch (PARTIAL)

**Completed:**
- Type ID system: All GC structs have type_id in field 0
- get-type-id function: O(n) ref.test chain (O(1) needs struct subtyping)
- Dispatch table infrastructure with call_indirect
- Table-based dispatch for all protocol methods
- Polymorphic lowerer: `nth`, `count`, `first`, `rest` work on vectors AND lists
  - Vector: `first` returns element 0 or nil; `rest` returns nil (TODO: proper seq)
  - Cons: `count` and `nth` via O(n) traversal

**Remaining:**
- User-defined protocols (`defprotocol`, `extend-type`)
- Vector `rest` returning proper seq (requires ChunkedSeq/IndexedSeq types)

### 2.1 Design: Hybrid Dispatch

Use a two-tier approach:
1. **Fast path**: Built-in protocols on built-in types → inline `ref.test` + direct struct ops
2. **Slow path**: Everything else → `get-type-id` + dispatch table + `call_ref`

```
┌─────────────────────────────────────────────────────────┐
│                    Protocol Call Site                    │
├─────────────────────────────────────────────────────────┤
│  Built-in protocol + built-in type (compile-time known)? │
│    YES → inline fast path (ref.test + direct struct ops) │
│    NO  → call get-type-id → dispatch table → call_ref    │
└─────────────────────────────────────────────────────────┘
```

This enables:
- Fast operations for common cases (nth on vector, get on map)
- Full Clojure-style extensibility (extend any type with any protocol)
- Users can extend built-in types with user-defined protocols

### 2.2 Type ID System

Built-in types have implicit IDs derived via `ref.test`. User types store explicit tags.

**Built-in type IDs** (match gc_types constants):
| Type | ID |
|------|----|
| LargeInt | 0 |
| Float | 1 |
| String | 2 |
| TrieNode | 3 |
| Cons | 4 |
| HamtNode | 5 |
| PersistentVector | 6 |
| PersistentMap | 7 |
| PersistentSet | 8 |

**User types**: Start at ID 256+ (room for future built-ins). Store tag as first struct field.

```wasm
(type $UserValue (struct (field $tag i32) ...))
```

### 2.3 `get-type-id` Function

```wasm
(func $get-type-id (param $obj (ref eq)) (result i32)
  ;; Built-ins: derive type ID from ref.test
  (if (ref.test (ref $PersistentVector) (local.get $obj))
    (then (return (i32.const 6))))

  (if (ref.test (ref $PersistentMap) (local.get $obj))
    (then (return (i32.const 7))))

  (if (ref.test (ref $PersistentSet) (local.get $obj))
    (then (return (i32.const 8))))

  (if (ref.test (ref $Cons) (local.get $obj))
    (then (return (i32.const 4))))

  (if (ref.test (ref $String) (local.get $obj))
    (then (return (i32.const 2))))

  ;; i31ref values (nil, bool, small int)
  (if (ref.test i31 (local.get $obj))
    (then (return (i32.const -1))))  ;; special: not dispatchable

  ;; User types: read tag from struct field
  (struct.get $UserValue $tag
    (ref.cast (ref $UserValue) (local.get $obj)))
)
```

### 2.4 Dispatch Table

2D table indexed by `[type_id, method_id]`:

```wasm
;; Flattened: index = type_id * NUM_METHODS + method_id
(table $protocol_dispatch funcref (elem ...))

(func $dispatch (param $obj (ref eq)) (param $method_id i32) (result (ref func))
  (local $type_id i32)
  (local.set $type_id (call $get-type-id (local.get $obj)))

  (table.get $protocol_dispatch
    (i32.add
      (i32.mul (local.get $type_id) (i32.const $NUM_METHODS))
      (local.get $method_id)))
)
```

### 2.5 Built-in Protocols

Define core protocols with method IDs:

| Protocol | Method | ID |
|----------|--------|----|
| ILookup | -lookup | 0 |
| IAssociative | -assoc | 1 |
| ICounted | -count | 2 |
| IIndexed | -nth | 3 |
| ICollection | -conj | 4 |
| ISeq | -first | 5 |
| ISeq | -rest | 6 |
| ISeqable | -seq | 7 |
| IHash | -hash | 8 |
| IEquiv | -equiv | 9 |

### 2.6 Polymorphic Core Functions

Replace static dispatch with protocol calls:

```clojure
;; Current (static):
(nth coll i) → Expr::VecNth  ; always vector

;; New (polymorphic):
(nth coll i)
  → if coll is PersistentVector at compile-time: fast path
  → else: (invoke IIndexed/-nth coll i)
```

**Fast path example** (compile-time known vector):
```wasm
;; (nth known-vec 5)
local.get $known_vec
ref.cast (ref $PersistentVector)  ;; elided if type already proven
call $vec_nth_impl                 ;; direct call, no dispatch
```

**Slow path example** (unknown collection type):
```wasm
;; (nth unknown-coll 5)
local.get $unknown_coll
i32.const 5
local.get $unknown_coll
i32.const 3                        ;; IIndexed/-nth method ID
call $dispatch                     ;; get funcref from table
call_ref                           ;; indirect call
```

### 2.7 User-Defined Protocols

```clojure
(defprotocol IJsonable
  (-to-json [this]))
```

Compiler assigns method ID (e.g., 100) from user protocol namespace.

```clojure
(extend-type PersistentVector IJsonable
  (-to-json [this] ...))
```

At compile time, populate dispatch table:
```
table[6][100] = vec-to-json-fn   ;; type_id=6 (PersistentVector), method_id=100
```

### 2.8 Implementation Tasks

**Core infrastructure:**
- [ ] Define `$UserValue` struct type with `$tag` field (for user-defined types)
- [x] Implement `$get-type-id` function
- [x] Create dispatch table in module
- [x] Implement `$dispatch` lookup function (via call_indirect)

**Built-in protocol dispatch:**
- [x] Add method IDs for core protocols (ILookup, ICounted, IIndexed, etc.)
- [x] Populate dispatch table for built-in types
- [x] Implement fast-path detection in codegen (type known at compile time)
- [x] Generate slow-path dispatch for unknown types

**Lowerer changes:**
- [x] Change `nth` to emit polymorphic dispatch (not just VecNth)
- [x] Change `get` to emit polymorphic dispatch (MapGet with HAMT)
- [x] Change `count` to emit polymorphic dispatch
- [x] Change `conj` to emit polymorphic dispatch
- [x] Change `first`/`rest` to emit polymorphic dispatch

**User protocol support:**
- [ ] Parse `defprotocol` form
- [ ] Assign method IDs to user protocol methods
- [ ] Parse `extend-type` / `extend-protocol` forms
- [ ] Generate dispatch table entries for extensions
- [ ] Support extending built-in types with user protocols

### 2.9 Dispatch Matrix

| Call Site | Type Known | Protocol | Dispatch |
|-----------|------------|----------|----------|
| `(nth vec 0)` | Yes (vector) | Built-in | **Fast**: inline |
| `(nth coll 0)` | No | Built-in | **Slow**: table lookup |
| `(-to-json vec)` | Yes (vector) | User | **Slow**: table lookup |
| `(-to-json obj)` | No | User | **Slow**: table lookup |
| `(-to-json rec)` | Yes (user type) | User | **Slow**: table lookup |

Note: User protocols always use slow path (no inline fast path), but built-in types can still be extended via the dispatch table.

---

## Phase 3: Full Vector Trie (>32 elements) ✓ COMPLETE

Large vectors (>32 elements) now work with full trie implementation:
- `nth` traverses trie to find leaf node
- `conj` handles tail overflow and tree growth
- `count` returns element count in O(1)

All helper functions implemented: `vec_tail_off`, `vec_array_for`, `vec_new_path`, `vec_push_tail`, `vec_aclone`.

### 3.1 Trie Node Structure
```
TRIE_NODE = array<eqref>[32]  ; Already defined

PersistentVector = struct {
  cnt: i32,           ; total element count
  shift: i32,         ; depth * 5 (5 bits per level)
  root: ref TRIE_NODE ; null for ≤32 elements
  tail: ref TRIE_NODE ; rightmost leaf (≤32 elements)
}
```

### 3.2 Helper Functions (from ClojureScript)

**tail-off**: Index where tail begins
```clojure
(defn tail-off [v]
  (if (< (.-cnt v) 32)
    0
    (bit-shift-left (unsigned-bit-shift-right (dec (.-cnt v)) 5) 5)))
```
- [ ] Implement as WASM helper function

**array-for**: Get the array containing index
```clojure
(defn array-for [v i]
  (if (>= i (tail-off v))
    (.-tail v)
    (loop [node (.-root v)
           level (.-shift v)]
      (if (pos? level)
        (recur (aget node (bit-and (unsigned-bit-shift-right i level) 0x1f))
               (- level 5))
        node))))
```
- [ ] Implement trie traversal with loop

**nth**: Element access
```clojure
(defn -nth [v i]
  (aget (array-for v i) (bit-and i 0x1f)))
```
- [ ] Update `generate_vec_nth` to use array-for

### 3.3 Trie Modification Functions

**new-path**: Create path from root to new node
```clojure
(defn new-path [level node]
  (if (zero? level)
    node
    (let [ret (make-array 32)]
      (aset ret 0 (new-path (- level 5) node))
      ret)))
```
- [ ] Implement path construction

**push-tail**: Insert tail into trie
```clojure
(defn push-tail [v level parent tail-node]
  (let [subidx (bit-and (unsigned-bit-shift-right (dec (.-cnt v)) level) 0x1f)
        ret (aclone parent)]
    (if (= level 5)
      (aset ret subidx tail-node)
      (let [child (aget parent subidx)]
        (if child
          (aset ret subidx (push-tail v (- level 5) child tail-node))
          (aset ret subidx (new-path (- level 5) tail-node)))))
    ret))
```
- [ ] Implement with structural sharing (aclone = array.copy)

**conj** (full implementation):
```clojure
(defn conj [v val]
  (if (< (- (.-cnt v) (tail-off v)) 32)
    ;; Room in tail
    (let [new-tail (aclone (.-tail v))]
      (aset new-tail (bit-and (.-cnt v) 0x1f) val)
      (PersistentVector. (inc (.-cnt v)) (.-shift v) (.-root v) new-tail))
    ;; Tail full - push into trie
    (let [new-root (if (> (unsigned-bit-shift-right (.-cnt v) 5)
                          (bit-shift-left 1 (.-shift v)))
                     ;; Root overflow - grow tree
                     (let [new-root (make-array 32)]
                       (aset new-root 0 (.-root v))
                       (aset new-root 1 (new-path (.-shift v) (.-tail v)))
                       new-root)
                     ;; Fits in current tree
                     (push-tail v (.-shift v) (.-root v) (.-tail v)))
          new-shift (if (> (unsigned-bit-shift-right (.-cnt v) 5)
                           (bit-shift-left 1 (.-shift v)))
                      (+ (.-shift v) 5)
                      (.-shift v))]
      (PersistentVector. (inc (.-cnt v)) new-shift new-root (array val)))))
```
- [ ] Implement full conj with root overflow handling
- [ ] Handle tree growth (shift += 5)

### 3.4 Additional Vector Operations

**assoc-n**: Update element at index
```clojure
(defn do-assoc [v level node i val]
  (let [ret (aclone node)]
    (if (zero? level)
      (aset ret (bit-and i 0x1f) val)
      (let [subidx (bit-and (unsigned-bit-shift-right i level) 0x1f)]
        (aset ret subidx (do-assoc v (- level 5) (aget node subidx) i val))))
    ret))
```
- [ ] Implement with path copying

**pop**: Remove last element
```clojure
(defn pop-tail [v level node]
  (let [subidx (bit-and (unsigned-bit-shift-right (- (.-cnt v) 2) level) 0x1f)]
    (cond
      (> level 5)
      (let [new-child (pop-tail v (- level 5) (aget node subidx))]
        (if (and (nil? new-child) (zero? subidx))
          nil
          (let [ret (aclone node)]
            (aset ret subidx new-child)
            ret)))
      (zero? subidx) nil
      :else (let [ret (aclone node)]
              (aset ret subidx nil)
              ret))))
```
- [ ] Implement pop with tree shrinkage

### 3.5 Implementation Tasks
- [ ] Add helper locals to codegen for trie traversal
- [ ] Implement `array.copy` for structural sharing
- [ ] Add loop/br_if for traversal
- [ ] Update `generate_vec_new_large` to build proper trie
- [ ] Remove error from `generate_vec_conj_inplace`
- [ ] Register PersistentVector protocol implementations in dispatch table

---

## Phase 4: HAMT for Maps and Sets ✓ COMPLETE

Full HAMT (Hash Array Mapped Trie) implementation following ClojureScript patterns.

**Protocol integration:**
- PersistentMap (type_id=9): `ILookup/-lookup`, `IAssociative/-assoc`, `ICounted/-count`
- PersistentSet (type_id=10): `ILookup/-lookup`, `ICollection/-conj`, `ICounted/-count`

### 4.1 Node Types (from ClojureScript)

ClojureScript uses three node types:

**BitmapIndexedNode** (sparse, ≤16 entries):
```
struct {
  bitmap: i32,              ; which of 32 slots are occupied
  arr: array<eqref>         ; 2*popcount(bitmap) entries: [k0,v0,k1,v1,...]
}
```

**ArrayNode** (dense, >16 entries):
```
struct {
  cnt: i32,                 ; number of non-null children
  arr: array<eqref>[32]     ; direct indexing, null for empty slots
}
```

**HashCollisionNode** (same hash, different keys):
```
struct {
  hash: i32,                ; shared hash value
  cnt: i32,                 ; number of entries
  arr: array<eqref>         ; [k0,v0,k1,v1,...] linear scan
}
```

Update gc_types:
- [ ] Change HAMT_NODE to BitmapIndexedNode structure
- [ ] Add ARRAY_NODE type index
- [ ] Add HASH_COLLISION_NODE type index

### 4.2 Core HAMT Functions

**mask**: Extract 5-bit index from hash at level
```clojure
(defn mask [hash shift]
  (bit-and (unsigned-bit-shift-right hash shift) 0x1f))
```

**bitpos**: Convert index to bitmap position
```clojure
(defn bitpos [hash shift]
  (bit-shift-left 1 (mask hash shift)))
```

**bitmap-indexed-node-index**: Sparse array index
```clojure
(defn bitmap-indexed-node-index [bitmap bit]
  (bit-count (bit-and bitmap (dec bit))))
```
- [ ] Implement `i32.popcnt` for bit-count

### 4.3 Map Operations

**inode-find** (lookup):
```clojure
;; BitmapIndexedNode
(inode-find [this shift hash key not-found]
  (let [bit (bitpos hash shift)]
    (if (zero? (bit-and bitmap bit))
      not-found
      (let [idx (bitmap-indexed-node-index bitmap bit)
            key-or-nil (aget arr (* 2 idx))
            val-or-node (aget arr (inc (* 2 idx)))]
        (cond
          (nil? key-or-nil)
          (inode-find val-or-node (+ shift 5) hash key not-found)

          (= key key-or-nil)
          val-or-node

          :else
          not-found)))))
```
- [ ] Implement with type dispatch on node kind

**inode-assoc** (insert/update):
```clojure
(inode-assoc [this shift hash key val added-leaf?]
  (let [bit (bitpos hash shift)
        idx (bitmap-indexed-node-index bitmap bit)]
    (if (zero? (bit-and bitmap bit))
      ;; New entry
      (let [n (bit-count bitmap)]
        (if (>= n 16)
          ;; Promote to ArrayNode
          (promote-to-array-node ...)
          ;; Add to BitmapIndexedNode
          (let [new-arr (array-copy-insert arr (* 2 idx) key val)]
            (BitmapIndexedNode. (bit-or bitmap bit) new-arr))))
      ;; Existing slot
      (let [key-or-nil (aget arr (* 2 idx))
            val-or-node (aget arr (inc (* 2 idx)))]
        (cond
          (nil? key-or-nil)
          ;; Recurse into child node
          (let [n (inode-assoc val-or-node (+ shift 5) hash key val added-leaf?)]
            (BitmapIndexedNode. bitmap (aset-copy arr (inc (* 2 idx)) n)))

          (= key key-or-nil)
          ;; Update existing
          (BitmapIndexedNode. bitmap (aset-copy arr (inc (* 2 idx)) val))

          :else
          ;; Hash collision - create subtree
          (create-node (+ shift 5) key-or-nil val-or-node hash key val))))))
```

### 4.4 Implementation Tasks

- [x] Add ARRAY_NODE, HASH_COLLISION_NODE to gc_types
- [x] Implement `i32.popcnt` wrapper
- [x] Implement equality check (equiv) for keys
- [x] `inode-find` for BitmapIndexedNode
- [x] `inode-find` for ArrayNode
- [x] `inode-find` for HashCollisionNode
- [x] `inode-assoc` for BitmapIndexedNode
- [x] `inode-assoc` for ArrayNode
- [x] `inode-assoc` for HashCollisionNode
- [x] Promotion: BitmapIndexedNode → ArrayNode
- [x] create-node for hash collisions
- [ ] `inode-dissoc` (remove key) - Future work
- [x] Wire up `generate_map_get` and `generate_map_assoc`

### 4.5 Set Implementation

Sets reuse HAMT but store only keys (or key=value):
- [x] `generate_set_contains` via inode-find
- [x] `generate_set_conj` via inode-assoc
- [ ] `generate_set_disj` via inode-dissoc - Future work
- [x] Register PersistentMap and PersistentSet protocol implementations in dispatch table

---

## Phase 5: Verify GC Mode End-to-End ✓ COMPLETE

GC mode is now the only mode. Legacy tagged i64 code has been removed.

- [x] ~~Add GC mode flag to Lowerer~~ (not needed - GC is always on)
- [x] ~~Propagate flag through codegen~~ (not needed - GC is always on)
- [x] Enable wasmtime GC for compiled components
- [x] Test with vectors >32 elements
- [x] Test with maps containing data
- [x] Verify protocol dispatch works for all collection types

---

## Phase 6: WIT Boundary Marshaling

Convert between internal GC refs and WIT primitives at export boundaries:

| WIT Type | To GC Ref | From GC Ref |
|----------|-----------|-------------|
| i32 | `(n << 1) \| 1` → `ref.i31` | `i31.get_s >> 1` |
| i64 | `struct.new $LARGE_INT` | `struct.get` |
| f64 | `struct.new $FLOAT` | `struct.get` |
| string | `array.new_data $STRING` | extract bytes |

- [ ] Implement marshaling for all primitive types
- [ ] Implement list<T> ↔ vector marshaling
- [ ] Implement record ↔ map marshaling (if needed)

---

## Phase 7: Cleanup ✓ COMPLETE

Legacy non-GC code has been removed. GC is now the only mode.

- [x] ~~Remove `tags` module~~ (never existed as separate module)
- [x] Remove tagged i64 IR variants (`Type::Tagged`, `Expr::MakeTagged`, etc.)
- [x] Remove heap operation IR variants (`Expr::Alloc`, `Expr::HeapStore`, `Expr::HeapLoad`)
- [x] GC mode is the default (and only) mode

---

## Phase 8: Transient Collections (Optional, Performance)

ClojureScript provides mutable "transient" variants for batch construction:

```clojure
(persistent! (conj! (conj! (transient []) 1) 2))
```

### TransientVector
- Mutable tail and root arrays
- `conj!` mutates in place
- `persistent!` freezes and returns immutable

### TransientHashMap
- Mutable HAMT nodes with edit-id for ownership
- `assoc!` mutates if owner matches
- Path copying only when ownership differs

Implementation priority: LOW (optimization)
- [ ] TransientVector with mutable tail
- [ ] TransientHashMap with edit tracking
- [ ] `transient`, `conj!`, `assoc!`, `persistent!` special forms

**Protocol integration:** Transients implement `ITransientCollection/-conj!`, `ITransientAssociative/-assoc!`, etc. Same dispatch mechanism.

---

## Phase 9: Optimization

### wasm-opt Integration
Run Binaryen's optimizer on compiled output:
- [ ] Add `--optimize` / `-O` flag to compile command
- [ ] Shell out to `wasm-opt -O3` on generated `.wasm` files
- [ ] Optionally bundle wasm-opt or require it in PATH

Expected benefit: ~1.9x speedup on WasmGC code (per V8 benchmarks).

### Collection-Specific Optimizations
- [ ] Inline small vector operations (≤32 elements)
- [ ] Specialize nth/get for known-small collections
- [ ] Escape analysis for local-only collections

---

## Phase 10: WASI CLI Commands

### `-main` Convention
Functions named `-main` compile to `wasi:cli/run` command components:

```clojure
(ns my-app.core
  (gen-world :my-app/cli))

(defn ^:export -main []
  (println "Hello, world!")
  0)  ; exit code
```

Implementation:
- [ ] Detect `-main` in analyzed module
- [ ] Generate synthetic WIT world importing `wasi:cli/*`
- [ ] Wire `-main` to `wasi:cli/run.run` export
- [ ] Handle args via `wasi:cli/environment.get-arguments`
- [ ] Handle exit code as return value

---

## Implementation Order (Recommended)

### 🎯 PRIORITY: Compositional Primitives (see Priority Zero above)

**Current focus - enables self-extending language:**

1. ✓ **First-class functions (closures)** - COMPLETE
2. ✓ **apply** - dynamic invocation - COMPLETE
3. ✓ **Quote/Syntax-quote** - code as data - COMPLETE
4. ✓ **defmacro** - compile-time expansion - COMPLETE
5. ✓ **Bootstrap core macros** - `when`, `when-not`, `and`, `or`, `cond`, `case` - COMPLETE
6. **User protocols** - `defprotocol`, `extend-type` ← NEXT

### Completed Foundation

1. ✓ **Hash function** - Prerequisite for HAMT and IHash protocol
2. ✓ **Protocol infrastructure** - `get-type-id`, dispatch table, `$dispatch` function
3. ✓ **Built-in protocol registration** - ILookup, ICounted, IIndexed, ICollection for all types
4. ✓ **Vector >32** - Full trie with IIndexed/-nth, ICounted/-count, ICollection/-conj
5. ✓ **HAMT for maps** - ILookup/-lookup, IAssociative/-assoc, ICounted/-count
6. ✓ **HAMT for sets** - ILookup/-lookup, ICollection/-conj, ICounted/-count
7. ✓ **Polymorphic lowerer** - `nth`, `get`, `count`, `conj` emit protocol dispatch
8. ✓ **GC mode integration** - GC is the only mode; legacy code removed
9. ✓ **Cleanup** - Legacy non-GC code removed

### After Compositional Primitives

10. **WIT marshaling** - Required for component exports
11. **Transients** - ITransientCollection, ITransientAssociative (optimization)
12. **wasm-opt** - Final optimization pass

---

## Phase 11: List/Seq Operations

**Protocol integration:** Cons cells (type_id=4) implement:
- `ISeq/-first`, `ISeq/-rest` - Core sequence operations
- `ISeqable/-seq` - Returns self
- `ICounted/-count` - Linear traversal (or cached)

Additionally:
- [ ] Implement `ISeqable/-seq` for PersistentVector (returns chunked seq or indexed seq)
- [ ] Implement `ISeqable/-seq` for PersistentMap (returns entry seq)
- [ ] Implement `ISeqable/-seq` for PersistentSet (returns element seq)
- [ ] `first`, `rest`, `seq` in lowerer emit ISeq/ISeqable dispatch

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

ClojureScript source locations for reference:
- `src/main/cljs/cljs/core.cljs` - All persistent collections
- `PersistentVector` - lines ~4000-4400
- `PersistentHashMap` - lines ~5000-5800
- `BitmapIndexedNode` - lines ~5100-5400
- `ArrayNode` - lines ~5400-5550
- `HashCollisionNode` - lines ~5550-5700
- `m3-hash-*` functions - lines ~800-900
- Protocol definitions - lines ~500-700 (ISeq, ILookup, IAssociative, etc.)
