# Suss Metadata Design Document

This document outlines the design for implementing comprehensive Clojure-style metadata in Suss, based on the ClojureScript implementation.

## Current State

### What Works Now
- `^:export` - Marks functions for WIT export
- `^:type-id N` - Reserves GC type index for deftypes
- `^:mutable` - Marks deftype fields as mutable
- `^i32`, `^i64`, `^f64`, `^eqref` - Type hints for return types and fields

### Limitations
- **No real metadata structure** - Edn types have no metadata field
- **Parser treats metadata as symbols** - `^:export` is parsed as a symbol, not attached to the next form
- **No runtime access** - Cannot call `(meta x)` or `(with-meta x m)`
- **Compile-only** - Metadata consumed during compilation, not preserved

## ClojureScript Reference Implementation

### Var Type
**File:** `clojurescript/src/main/cljs/cljs/core.cljs` (lines 1187-1250)

```clojure
(deftype Var [val sym _meta]
  IDeref
  (-deref [_] (val))

  IMeta
  (-meta [_] _meta)

  IWithMeta
  (-with-meta [_ new-meta]
    (Var. val sym new-meta))
  ...)
```

### Metadata Keys

| Key | Purpose | Set By |
|-----|---------|--------|
| `:doc` | Documentation string | `defn`, `def` |
| `:arglists` | Function signatures | Auto-computed by `defn` |
| `:tag` | Type hint | `^Type` syntax |
| `:private` | Namespace privacy | `defn-` or `^:private` |
| `:dynamic` | Rebindable var | `^:dynamic` |
| `:const` | Compile-time constant | `^:const` |
| `:macro` | Marks as macro | `defmacro` |
| `:export` | WIT/JS export name | `^:export` |
| `:protocol` | Protocol membership | `defprotocol` |
| `:test` | Test function | `deftest` |

### Reader Syntax

```clojure
;; Keyword metadata (shorthand for {:keyword true})
^:private
^:dynamic
^:export

;; Map metadata (full form)
^{:doc "docstring" :private true}

;; Type hint (shorthand for {:tag Type})
^String
^i32

;; Combined
^:private ^String (defn foo [x] x)
```

### Runtime Functions

**File:** `clojurescript/src/main/cljs/cljs/core.cljs`

```clojure
;; Get metadata (lines 2174-2179)
(defn meta [o]
  (when (and (not (nil? o))
             (satisfies? IMeta o))
    (-meta o)))

;; Create with new metadata (lines 2165-2172)
(defn with-meta [o meta]
  (if (js-fn? o)
    (MetaFn. o meta)
    (when-not (nil? o)
      (-with-meta o meta))))

;; Modify metadata (lines 4171-4185)
(defn vary-meta [obj f & args]
  (with-meta obj (apply f (meta obj) args)))
```

### Compile-Time Processing

**File:** `clojurescript/src/main/clojure/cljs/analyzer.cljc`

The analyzer extracts metadata from symbols at parse time (lines 2032-2036):
```clojure
const? (-> sym meta :const)
sym-meta (meta sym)
tag (-> sym meta :tag)
dynamic (-> sym meta :dynamic)
```

And stores it in the compiler state (lines 2122-2175).

## Proposed Implementation

### Phase 1: Add Metadata to Edn Types

**Files:** `crates/suss-core/src/edn.rs`

Add metadata field to key types:

```rust
// Option 1: Wrapper type
pub struct WithMeta<T> {
    pub value: T,
    pub meta: Option<Box<Edn>>,
}

// Option 2: Direct on Edn
pub enum Edn {
    Nil,
    Bool(bool),
    Number(Number),
    String(String),
    Symbol(Symbol),
    Keyword(Keyword),
    List(Vec<Edn>),
    Vector(Vec<Edn>),
    Map(HashMap<Edn, Edn>),
    Set(HashSet<Edn>),
    // New: metadata attachment
    WithMeta(Box<Edn>, Box<Edn>),  // (value, meta-map)
}
```

**Recommendation:** Use `WithMeta` variant for explicit metadata attachment. This is cleaner than adding `meta` field to every variant.

### Phase 2: Reader Metadata Parsing

**Files:** `crates/suss-reader/src/parser.rs`

Add parsing rules:

```rust
// Parse ^:keyword → {:keyword true}
fn parse_keyword_meta(&mut self) -> ParseResult<Edn> {
    self.expect('^')?;
    self.expect(':')?;
    let kw = self.parse_keyword()?;
    Ok(Edn::Map([(Edn::Keyword(kw), Edn::Bool(true))].into()))
}

// Parse ^{...} → metadata map
fn parse_map_meta(&mut self) -> ParseResult<Edn> {
    self.expect('^')?;
    self.parse_map()
}

// Parse ^Type → {:tag Type}
fn parse_type_meta(&mut self) -> ParseResult<Edn> {
    self.expect('^')?;
    let sym = self.parse_symbol()?;
    Ok(Edn::Map([(Edn::Keyword("tag".into()), Edn::Symbol(sym))].into()))
}

// Attach metadata to next form
fn parse_with_possible_meta(&mut self) -> ParseResult<Edn> {
    let mut meta_maps = Vec::new();
    while self.peek() == Some('^') {
        meta_maps.push(self.parse_meta()?);
    }
    let form = self.parse_form()?;
    if meta_maps.is_empty() {
        Ok(form)
    } else {
        let merged = merge_meta_maps(meta_maps);
        Ok(Edn::WithMeta(Box::new(form), Box::new(merged)))
    }
}
```

### Phase 3: Runtime Protocols

**Files:** `crates/suss-compile/src/core.sus`

```clojure
(defprotocol IMeta
  (-meta [o] "Returns the metadata of o"))

(defprotocol IWithMeta
  (-with-meta [o meta] "Returns a new object with given metadata"))

;; User-facing functions
(defn meta [o]
  (when (satisfies? IMeta o)
    (-meta o)))

(defn with-meta [o m]
  (cond
    (nil? o) nil
    (satisfies? IWithMeta o) (-with-meta o m)
    :else (throw "Object does not support metadata")))

(defn vary-meta [o f & args]
  (with-meta o (apply f (meta o) args)))
```

### Phase 4: Compiler Enhancements

**Files:** `crates/suss-compile/src/analyze.rs`, `expand.rs`

#### Preserve Metadata Through Expansion

```rust
// In expand.rs - when expanding macros, preserve metadata
fn expand(&mut self, expr: Edn) -> CompileResult<Edn> {
    match expr {
        Edn::WithMeta(inner, meta) => {
            let expanded = self.expand(*inner)?;
            Ok(Edn::WithMeta(Box::new(expanded), meta))
        }
        // ... rest of expansion
    }
}
```

#### Support `:doc` in defn

```rust
// Extract docstring into metadata
fn analyze_defn(&mut self, items: &[Edn]) -> CompileResult<()> {
    // ... existing parsing

    let doc = if let Edn::String(s) = &items[doc_idx] {
        Some(s.clone())
    } else {
        None
    };

    // Add to function metadata for IDE/REPL access
    self.functions.push(AnalyzedFunction {
        // ... existing fields
        doc,  // New field
    });
}
```

#### Support `:private`

```rust
// Private functions not exported, not visible outside namespace
fn analyze_def(&mut self, items: &[Edn]) -> CompileResult<()> {
    let private = metadata.iter().any(|s| s == "private");

    // Skip WIT export validation for private functions
    // Don't add to public namespace exports
}
```

### Phase 5: WASM GC Implementation

For runtime metadata access, each metadata-bearing type needs a `_meta` field in its GC struct:

```
struct PersistentVector {
    type_id: i32,
    cnt: i32,
    shift: i32,
    root: eqref,
    tail: eqref,
    _meta: eqref,  // New: metadata map or nil
}
```

The `with-meta` function creates a new struct with same fields but different `_meta`.

## Implementation Order

1. **Core Types** - Add `WithMeta` to Edn enum
2. **Reader** - Parse `^:keyword` and `^{map}` syntax
3. **Analyzer** - Preserve and extract metadata
4. **Expand** - Preserve metadata through macro expansion
5. **Protocols** - Add IMeta/IWithMeta to core.sus
6. **Runtime** - Add _meta field to collection types
7. **Functions** - Implement meta/with-meta/vary-meta

## Migration Notes

- Current `^:export` syntax continues to work (backwards compatible)
- Current type hints (`^i32`) continue to work
- New map syntax `^{:key val}` is additive
- Runtime functions require collection type updates

## Open Questions

1. Should all Edn variants support metadata, or only collections/symbols?
2. How to handle metadata in REPL state persistence?
3. Performance implications of _meta field on every struct?
4. Should metadata be preserved across serialization (pr-str/read)?

## References

- ClojureScript core.cljs: `clojurescript/src/main/cljs/cljs/core.cljs`
- ClojureScript analyzer: `clojurescript/src/main/clojure/cljs/analyzer.cljc`
- ClojureScript core macros: `clojurescript/src/main/clojure/cljs/core.cljc`
