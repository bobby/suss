//! Intermediate Representation for the Suss compiler
//!
//! The IR is a simplified, typed representation of Suss code that maps
//! closely to WASM instructions.

/// WASM GC type definitions for Clojure's immutable persistent data structures.
///
/// Uses native WASM GC types (i31ref, structref, arrayref) instead of
/// tagged i64 values. This enables proper garbage collection and
/// eliminates the need for a custom allocator.
///
/// Value representation:
/// - nil, false, true: i31ref with sentinel values
/// - small integers: i31ref (30-bit signed, shifted)
/// - large integers: struct { i64 }
/// - floats: struct { f64 }
/// - strings: array<i8>
/// - vectors: PersistentVector (32-way bit-partitioned trie)
/// - lists: cons cells (struct { first, rest })
/// - maps: PersistentMap (HAMT with bitmap compression)
/// - sets: PersistentSet (HAMT with bitmap compression)
pub mod gc_types {
    // =========================================================================
    // GC Type Indices
    // These are indices into the WASM type section, assigned during codegen.
    // The actual type definitions are emitted by CodeGen::emit_gc_type_section.
    // =========================================================================

    /// struct { i64 } - for integers that don't fit in i31ref
    pub const LARGE_INT: u32 = 0;

    /// struct { f64 } - all floats are boxed
    pub const FLOAT: u32 = 1;

    /// array<i8> - UTF-8 string bytes (mutable for construction)
    pub const STRING: u32 = 2;

    /// array<eqref> - 32-element trie node for persistent vectors
    /// Used as internal nodes and leaf arrays in the bit-partitioned trie
    pub const TRIE_NODE: u32 = 3;

    /// struct { first: eqref, rest: eqref } - cons cell for persistent lists
    pub const CONS: u32 = 4;

    // =========================================================================
    // HAMT Node Types (for maps and sets)
    // =========================================================================

    /// struct { type_id: i32, bitmap: i32, arr: eqref }
    /// Sparse HAMT node with ≤16 entries
    /// arr contains [key0, val0, key1, val1, ..., null, child, ...]
    pub const BITMAP_INDEXED_NODE: u32 = 5;

    /// struct { type_id: i32, cnt: i32, arr: eqref }
    /// Dense HAMT node with >16 entries (32 slots, direct indexing)
    pub const ARRAY_NODE: u32 = 6;

    /// struct { type_id: i32, hash: i32, cnt: i32, arr: eqref }
    /// Collision node for keys with same hash
    /// arr contains [key0, val0, key1, val1, ...] for linear scan
    pub const HASH_COLLISION_NODE: u32 = 7;

    // =========================================================================
    // Collection Types
    // =========================================================================

    /// struct { type_id: i32, cnt: i32, shift: i32, root: eqref, tail: eqref }
    /// ClojureScript-style 32-way bit-partitioned vector trie
    pub const PERSISTENT_VECTOR: u32 = 8;

    /// struct { type_id: i32, cnt: i32, root: eqref }
    /// Hash Array Mapped Trie (HAMT) for O(log32 n) operations
    pub const PERSISTENT_MAP: u32 = 9;

    /// struct { type_id: i32, cnt: i32, root: eqref }
    /// HAMT-based set (same structure as map but entries are keys only)
    pub const PERSISTENT_SET: u32 = 10;

    // =========================================================================
    // Closure Function Types (typed funcrefs for efficient call_ref)
    // Must be defined BEFORE closure structs so structs can reference them.
    // Signature: (env: eqref, args...) -> eqref
    // =========================================================================

    /// Function type for closure arity 0: (env) -> result
    pub const CLOSURE_FN_0: u32 = 11;
    /// Function type for closure arity 1: (env, arg1) -> result
    pub const CLOSURE_FN_1: u32 = 12;
    /// Function type for closure arity 2: (env, arg1, arg2) -> result
    pub const CLOSURE_FN_2: u32 = 13;
    /// Function type for closure arity 3
    pub const CLOSURE_FN_3: u32 = 14;
    /// Function type for closure arity 4
    pub const CLOSURE_FN_4: u32 = 15;
    /// Function type for closure arity 5
    pub const CLOSURE_FN_5: u32 = 16;
    /// Function type for closure arity 6
    pub const CLOSURE_FN_6: u32 = 17;
    /// Function type for closure arity 7
    pub const CLOSURE_FN_7: u32 = 18;
    /// Function type for closure arity 8
    pub const CLOSURE_FN_8: u32 = 19;

    /// Get the closure function type index for a given arity (0-8)
    #[inline]
    pub const fn closure_fn_type_for_arity(arity: u32) -> u32 {
        debug_assert!(arity <= 8, "Closure arity must be 0-8");
        CLOSURE_FN_0 + arity
    }

    // =========================================================================
    // Closure Struct Types (per-arity for typed function references)
    // Each closure has: type_id, env (captured values), fn (typed funcref)
    // Using typed non-null funcrefs avoids runtime type checks on call_ref
    // =========================================================================

    /// struct { type_id: i32, env: (ref null $trie_node), fn: (ref $closure_fn_0) }
    /// Closure struct with arity 0 (no arguments)
    pub const CLOSURE_0: u32 = 20;

    /// Closure struct with arity 1
    pub const CLOSURE_1: u32 = 21;

    /// Closure struct with arity 2
    pub const CLOSURE_2: u32 = 22;

    /// Closure struct with arity 3
    pub const CLOSURE_3: u32 = 23;

    /// Closure struct with arity 4
    pub const CLOSURE_4: u32 = 24;

    /// Closure struct with arity 5
    pub const CLOSURE_5: u32 = 25;

    /// Closure struct with arity 6
    pub const CLOSURE_6: u32 = 26;

    /// Closure struct with arity 7
    pub const CLOSURE_7: u32 = 27;

    /// Closure struct with arity 8
    pub const CLOSURE_8: u32 = 28;

    // =========================================================================
    // Variadic Function Types (for variadic builtin wrappers like +, *, -, /)
    // Unlike closure function types, these don't take an env parameter.
    // Signature: (args...) -> eqref
    // =========================================================================

    /// Function type for variadic arity 0: () -> result
    pub const VARIADIC_FN_0: u32 = 29;
    /// Function type for variadic arity 1: (arg1) -> result
    pub const VARIADIC_FN_1: u32 = 30;
    /// Function type for variadic arity 2: (arg1, arg2) -> result
    pub const VARIADIC_FN_2: u32 = 31;
    /// Function type for variadic arity 3
    pub const VARIADIC_FN_3: u32 = 32;
    /// Function type for variadic arity 4
    pub const VARIADIC_FN_4: u32 = 33;
    /// Function type for variadic arity 5
    pub const VARIADIC_FN_5: u32 = 34;
    /// Function type for variadic arity 6
    pub const VARIADIC_FN_6: u32 = 35;
    /// Function type for variadic arity 7
    pub const VARIADIC_FN_7: u32 = 36;
    /// Function type for variadic arity 8
    pub const VARIADIC_FN_8: u32 = 37;

    /// Get the variadic function type index for a given arity (0-8)
    #[inline]
    pub const fn variadic_fn_type_for_arity(arity: u32) -> u32 {
        debug_assert!(arity <= 8, "Variadic arity must be 0-8");
        VARIADIC_FN_0 + arity
    }

    // =========================================================================
    // Variadic Closure Struct Type (for variadic builtins used as values)
    // Contains 9 funcrefs, one for each arity 0-8.
    // =========================================================================

    /// struct { type_id: i32, fn0: (ref $variadic_fn_0), ..., fn8: (ref $variadic_fn_8) }
    /// Variadic closure struct with all 9 arities
    pub const VARIADIC_CLOSURE: u32 = 38;

    // =========================================================================
    // Variadic Closure Field Indices
    // =========================================================================

    /// Variadic closure type_id field
    pub const VC_TYPE_ID: u32 = 0;
    /// Variadic closure fn0 field (arity 0)
    pub const VC_FN0: u32 = 1;
    /// Variadic closure fn1 field (arity 1)
    pub const VC_FN1: u32 = 2;
    /// Variadic closure fn2 field (arity 2)
    pub const VC_FN2: u32 = 3;
    /// Variadic closure fn3 field (arity 3)
    pub const VC_FN3: u32 = 4;
    /// Variadic closure fn4 field (arity 4)
    pub const VC_FN4: u32 = 5;
    /// Variadic closure fn5 field (arity 5)
    pub const VC_FN5: u32 = 6;
    /// Variadic closure fn6 field (arity 6)
    pub const VC_FN6: u32 = 7;
    /// Variadic closure fn7 field (arity 7)
    pub const VC_FN7: u32 = 8;
    /// Variadic closure fn8 field (arity 8)
    pub const VC_FN8: u32 = 9;

    /// Number of GC types defined (for type index offset calculation)
    /// Includes: 11 base types + 9 closure fn types + 9 closure struct types
    ///         + 9 variadic fn types + 1 variadic closure struct = 39
    pub const NUM_GC_TYPES: u32 = 39;

    /// Get the closure struct type index for a given arity (0-8)
    #[inline]
    pub const fn closure_type_for_arity(arity: u32) -> u32 {
        debug_assert!(arity <= 8, "Closure arity must be 0-8");
        CLOSURE_0 + arity
    }

    // =========================================================================
    // Closure Field Indices
    // =========================================================================

    /// Closure type_id field (identifies arity for protocol dispatch)
    pub const CL_TYPE_ID: u32 = 0;
    /// Closure environment field (array of captured values)
    pub const CL_ENV: u32 = 1;
    /// Closure function field (typed funcref)
    pub const CL_FN: u32 = 2;

    // =========================================================================
    // i31ref Sentinel Values
    // These are stored directly in i31ref (31-bit signed integer reference).
    //
    // Encoding scheme:
    // - Sentinels use EVEN values (0, 2, 4): nil=0, false=2, true=4
    // - Small integers use ODD values: (n << 1) | 1
    //
    // This allows efficient discrimination:
    // - is_small_int: (x & 1) != 0
    // - is_falsy: x == 0 || x == 2 (nil or false)
    // =========================================================================

    /// nil sentinel value in i31ref
    pub const NIL_SENTINEL: i32 = 0;

    /// false sentinel value in i31ref
    pub const FALSE_SENTINEL: i32 = 2;

    /// true sentinel value in i31ref
    pub const TRUE_SENTINEL: i32 = 4;

    // =========================================================================
    // Small Integer Encoding
    // Small integers are stored in i31ref with tag bit 0 set.
    // Format: (value << 1) | 1
    // This gives us 30 bits of signed integer range.
    // =========================================================================

    /// Minimum value that fits in a small integer (i31ref)
    pub const SMALL_INT_MIN: i64 = -(1 << 29); // -536,870,912

    /// Maximum value that fits in a small integer (i31ref)
    pub const SMALL_INT_MAX: i64 = (1 << 29) - 1; // 536,870,911

    /// Check if an integer fits in a small integer (i31ref)
    #[inline]
    pub const fn fits_in_small_int(n: i64) -> bool {
        n >= SMALL_INT_MIN && n <= SMALL_INT_MAX
    }

    /// Encode a small integer for i31ref storage
    /// Returns the i32 value to pass to ref.i31
    #[inline]
    pub const fn encode_small_int(n: i64) -> i32 {
        ((n as i32) << 1) | 1
    }

    /// Decode a small integer from i31ref
    /// Takes the i32 value from i31.get_s
    #[inline]
    pub const fn decode_small_int(encoded: i32) -> i64 {
        (encoded >> 1) as i64
    }

    /// Check if an i31ref value is a small integer (has tag bit 0 set)
    #[inline]
    pub const fn is_small_int(i31_value: i32) -> bool {
        (i31_value & 1) != 0
    }

    /// Check if an i31ref value is truthy (not nil and not false)
    /// Falsy values are 0 (nil) and 2 (false), all others truthy
    #[inline]
    pub const fn is_truthy_sentinel(i31_value: i32) -> bool {
        // Small integers (odd) are always truthy
        // Even values: 0 (nil) and 2 (false) are falsy, 4 (true) and higher are truthy
        i31_value != NIL_SENTINEL && i31_value != FALSE_SENTINEL
    }

    // =========================================================================
    // xxHash32 Constants (for HAMT operations)
    // Used for hashing values in persistent maps and sets.
    // See: https://xxhash.com/ and ROADMAP.md Phase 1
    // =========================================================================

    /// xxHash32 prime constant 1
    pub const PRIME32_1: u32 = 0x9E3779B1;
    /// xxHash32 prime constant 2
    pub const PRIME32_2: u32 = 0x85EBCA77;
    /// xxHash32 prime constant 3
    pub const PRIME32_3: u32 = 0xC2B2AE3D;
    /// xxHash32 prime constant 4
    pub const PRIME32_4: u32 = 0x27D4EB2F;
    /// xxHash32 prime constant 5
    pub const PRIME32_5: u32 = 0x165667B1;

    /// Hash value for true (Java convention)
    pub const HASH_TRUE: i32 = 1231;
    /// Hash value for false (Java convention)
    pub const HASH_FALSE: i32 = 1237;

    /// Combine two hashes for ordered collections
    /// Uses rotl(h1, 5) ^ (h2 * PRIME32_1)
    #[inline]
    pub const fn hash_combine(h1: i32, h2: i32) -> i32 {
        h1.rotate_left(5) ^ h2.wrapping_mul(PRIME32_1 as i32)
    }

    /// Combine two hashes for unordered collections (commutative)
    /// Simple addition is commutative, suitable for sets/maps
    #[inline]
    pub const fn hash_unordered(h1: i32, h2: i32) -> i32 {
        h1.wrapping_add(h2)
    }

    // =========================================================================
    // Collection Field Indices
    // For accessing struct fields in codegen
    // =========================================================================

    /// All dispatchable types have type_id as field 0
    /// This enables O(1) type lookup for protocol dispatch
    pub const TYPE_ID: u32 = 0;

    /// PERSISTENT_VECTOR field indices (after type_id)
    pub const PV_CNT: u32 = 1;   // was 0
    pub const PV_SHIFT: u32 = 2; // was 1
    pub const PV_ROOT: u32 = 3;  // was 2
    pub const PV_TAIL: u32 = 4;  // was 3

    /// PERSISTENT_MAP field indices (after type_id)
    pub const PM_CNT: u32 = 1;   // was 0
    pub const PM_ROOT: u32 = 2;  // was 1

    /// PERSISTENT_SET field indices (after type_id)
    pub const PS_CNT: u32 = 1;   // was 0
    pub const PS_ROOT: u32 = 2;  // was 1

    /// BITMAP_INDEXED_NODE field indices
    pub const BIN_TYPE_ID: u32 = 0;
    pub const BIN_BITMAP: u32 = 1;
    pub const BIN_ARR: u32 = 2;

    /// ARRAY_NODE field indices
    pub const AN_TYPE_ID: u32 = 0;
    pub const AN_CNT: u32 = 1;
    pub const AN_ARR: u32 = 2;

    /// HASH_COLLISION_NODE field indices
    pub const HCN_TYPE_ID: u32 = 0;
    pub const HCN_HASH: u32 = 1;
    pub const HCN_CNT: u32 = 2;
    pub const HCN_ARR: u32 = 3;

    /// CONS field indices (after type_id)
    pub const CONS_FIRST: u32 = 1; // was 0
    pub const CONS_REST: u32 = 2;  // was 1

    /// LARGE_INT field indices (after type_id)
    pub const LI_VALUE: u32 = 1; // was 0 (the i64 value)

    /// FLOAT field indices (after type_id)
    pub const FL_VALUE: u32 = 1; // was 0 (the f64 value)

    /// STRING field indices (after type_id)
    pub const STR_PTR: u32 = 1;  // was 0
    pub const STR_LEN: u32 = 2;  // was 1

    // =========================================================================
    // Function Index Offsets
    // User-defined functions are emitted after runtime helper and protocol
    // implementation functions. These constants are used by the lowerer to
    // calculate correct function indices.
    // =========================================================================

    /// Number of runtime helper functions emitted before user functions.
    /// These include: hash_string, get_type_id, vector trie helpers, HAMT helpers, etc.
    pub const NUM_RUNTIME_HELPERS: u32 = 25;

    /// Number of protocol implementation wrapper functions (vec_count, map_lookup, etc.)
    pub const NUM_PROTOCOL_IMPLS: u32 = 14;

    /// Total offset for user-defined functions (after imports)
    pub const USER_FUNC_OFFSET: u32 = NUM_RUNTIME_HELPERS + NUM_PROTOCOL_IMPLS;
}

/// Type IDs for protocol dispatch.
///
/// These match the GC type indices for built-in types, enabling efficient
/// type-based dispatch via `ref.test` followed by table lookup.
pub mod type_ids {
    /// i31ref values (nil, bool, small int) - not dispatchable to most protocols
    pub const I31REF: i32 = -1;

    // Built-in types use their GC type indices
    pub const LARGE_INT: i32 = super::gc_types::LARGE_INT as i32;
    pub const FLOAT: i32 = super::gc_types::FLOAT as i32;
    pub const STRING: i32 = super::gc_types::STRING as i32;
    pub const TRIE_NODE: i32 = super::gc_types::TRIE_NODE as i32;
    pub const CONS: i32 = super::gc_types::CONS as i32;

    // HAMT node types
    pub const BITMAP_INDEXED_NODE: i32 = super::gc_types::BITMAP_INDEXED_NODE as i32;
    pub const ARRAY_NODE: i32 = super::gc_types::ARRAY_NODE as i32;
    pub const HASH_COLLISION_NODE: i32 = super::gc_types::HASH_COLLISION_NODE as i32;

    // Collection types
    pub const PERSISTENT_VECTOR: i32 = super::gc_types::PERSISTENT_VECTOR as i32;
    pub const PERSISTENT_MAP: i32 = super::gc_types::PERSISTENT_MAP as i32;
    pub const PERSISTENT_SET: i32 = super::gc_types::PERSISTENT_SET as i32;

    // Closure types (one per arity for IFn protocol dispatch)
    pub const CLOSURE_0: i32 = super::gc_types::CLOSURE_0 as i32;
    pub const CLOSURE_1: i32 = super::gc_types::CLOSURE_1 as i32;
    pub const CLOSURE_2: i32 = super::gc_types::CLOSURE_2 as i32;
    pub const CLOSURE_3: i32 = super::gc_types::CLOSURE_3 as i32;
    pub const CLOSURE_4: i32 = super::gc_types::CLOSURE_4 as i32;
    pub const CLOSURE_5: i32 = super::gc_types::CLOSURE_5 as i32;
    pub const CLOSURE_6: i32 = super::gc_types::CLOSURE_6 as i32;
    pub const CLOSURE_7: i32 = super::gc_types::CLOSURE_7 as i32;
    pub const CLOSURE_8: i32 = super::gc_types::CLOSURE_8 as i32;

    // Variadic closure (for variadic builtins like +, *, -, /)
    pub const VARIADIC_CLOSURE: i32 = super::gc_types::VARIADIC_CLOSURE as i32;

    /// User-defined types start at 256 (room for future built-ins)
    pub const USER_TYPE_BASE: i32 = 256;
}

/// Protocol method IDs for dispatch table indexing.
///
/// Built-in protocol methods use IDs 0-99.
/// User-defined protocol methods start at 100.
pub mod method_ids {
    /// ILookup/-lookup: (coll, key) -> value
    pub const LOOKUP: u32 = 0;
    /// IAssociative/-assoc: (coll, key, val) -> coll'
    pub const ASSOC: u32 = 1;
    /// ICounted/-count: (coll) -> i32
    pub const COUNT: u32 = 2;
    /// IIndexed/-nth: (coll, index) -> value
    pub const NTH: u32 = 3;
    /// ICollection/-conj: (coll, val) -> coll'
    pub const CONJ: u32 = 4;
    /// ISeq/-first: (seq) -> value
    pub const FIRST: u32 = 5;
    /// ISeq/-rest: (seq) -> seq
    pub const REST: u32 = 6;
    /// ISeqable/-seq: (coll) -> seq
    pub const SEQ: u32 = 7;
    /// IHash/-hash: (value) -> i32
    pub const HASH: u32 = 8;
    /// IEquiv/-equiv: (a, b) -> bool
    pub const EQUIV: u32 = 9;

    /// Number of built-in protocol methods
    pub const NUM_BUILTIN: u32 = 10;

    /// User-defined protocol methods start here
    pub const USER_START: u32 = 100;
}

/// Protocol function type indices for call_ref.
///
/// These are offsets from the base protocol type index in the type section.
/// Actual type indices are calculated as: GC_TYPES + HELPER_TYPES + offset
pub mod protocol_types {
    /// (eqref) -> eqref - for first, rest, seq
    pub const ARITY_1_REF: u32 = 0;
    /// (eqref) -> i32 - for count, hash
    pub const ARITY_1_I32: u32 = 1;
    /// (eqref, eqref) -> eqref - for lookup, nth, conj
    pub const ARITY_2_REF: u32 = 2;
    /// (eqref, eqref) -> i32 - for equiv
    pub const ARITY_2_I32: u32 = 3;
    /// (eqref, eqref, eqref) -> eqref - for assoc
    pub const ARITY_3_REF: u32 = 4;

    /// Number of protocol function types
    pub const NUM_PROTOCOL_TYPES: u32 = 5;

    /// Get the protocol type index for a method ID
    pub const fn type_for_method(method_id: u32) -> u32 {
        use super::method_ids;
        match method_id {
            method_ids::LOOKUP => ARITY_2_REF, // (coll, key) -> value
            method_ids::ASSOC => ARITY_3_REF,  // (coll, key, val) -> coll'
            method_ids::COUNT => ARITY_1_I32,  // (coll) -> i32
            method_ids::NTH => ARITY_2_REF,    // (coll, index) -> value
            method_ids::CONJ => ARITY_2_REF,   // (coll, val) -> coll'
            method_ids::FIRST => ARITY_1_REF,  // (seq) -> value
            method_ids::REST => ARITY_1_REF,   // (seq) -> seq
            method_ids::SEQ => ARITY_1_REF,    // (coll) -> seq
            method_ids::HASH => ARITY_1_I32,   // (value) -> i32
            method_ids::EQUIV => ARITY_2_I32,  // (a, b) -> bool
            _ => ARITY_1_REF,                  // Default for user methods
        }
    }
}

/// Dispatch table configuration for protocol method dispatch.
///
/// The dispatch table is a WASM funcref table indexed by:
/// `type_id * NUM_BUILTIN_METHODS + method_id`
pub mod dispatch_table {
    use super::method_ids;

    /// Number of type slots in the dispatch table.
    /// Provides room for built-in types (0-8) and some future expansion.
    pub const NUM_TYPE_SLOTS: u32 = 16;

    /// Total size of the dispatch table (type slots × methods per type)
    pub const TABLE_SIZE: u32 = NUM_TYPE_SLOTS * method_ids::NUM_BUILTIN;

    /// Index of the dispatch table in the module's table section
    pub const TABLE_INDEX: u32 = 0;

    /// Calculate dispatch table index for a (type_id, method_id) pair.
    ///
    /// # Arguments
    /// * `type_id` - Runtime type ID (from $get-type-id)
    /// * `method_id` - Protocol method ID
    ///
    /// # Returns
    /// Index into the dispatch table funcref array
    #[inline]
    pub const fn index(type_id: u32, method_id: u32) -> u32 {
        type_id * method_ids::NUM_BUILTIN + method_id
    }
}

// =========================================================================
// Protocol Definitions
// =========================================================================

/// A protocol definition with its methods
#[derive(Debug, Clone)]
pub struct ProtocolDef {
    /// Protocol name (e.g., "IJsonable")
    pub name: String,
    /// Methods: (name, method_id)
    pub methods: Vec<(String, u32)>,
}

/// A dispatch table entry mapping (type_id, method_id) to a function
#[derive(Debug, Clone)]
pub struct DispatchEntry {
    /// Type ID (from type_ids module)
    pub type_id: u32,
    /// Method ID (from method_ids module or user-defined)
    pub method_id: u32,
    /// Function index in the module
    pub func_idx: u32,
}

// =========================================================================
// User-Defined Types (deftype)
// =========================================================================

/// Field storage type for deftype fields
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    /// 32-bit integer (stored directly, boxed on access as i31ref)
    I32,
    /// 64-bit integer (stored directly, boxed on access as LARGE_INT)
    I64,
    /// 64-bit float (stored directly, boxed on access as FLOAT)
    F64,
    /// GC reference (stored and accessed as eqref)
    GcRef,
}

/// A field in a deftype definition
#[derive(Debug, Clone)]
pub struct DeftypeFieldDef {
    /// Field name (e.g., "x")
    pub name: String,
    /// Field storage type
    pub field_type: FieldType,
}

/// A user-defined type from deftype
#[derive(Debug, Clone)]
pub struct DeftypeDef {
    /// Type name (e.g., "Point")
    pub name: String,
    /// Fields with types
    pub fields: Vec<DeftypeFieldDef>,
    /// WASM GC struct type index (assigned during lowering)
    pub gc_type_idx: u32,
    /// Runtime type ID for protocol dispatch (256+ for user types)
    pub type_id: i32,
}

/// A compiled module containing all definitions
#[derive(Debug)]
pub struct Module {
    /// Imported functions from WIT interfaces
    pub imports: Vec<Import>,
    /// All function definitions
    pub functions: Vec<Function>,
    /// Global constant definitions
    pub globals: Vec<Global>,
    /// String literals (stored in data section)
    pub strings: Vec<String>,
    /// Protocol definitions (user-defined protocols)
    pub protocols: Vec<ProtocolDef>,
    /// Dispatch table entries for protocol methods
    pub dispatch_entries: Vec<DispatchEntry>,
    /// User-defined types (from deftype declarations)
    pub deftypes: Vec<DeftypeDef>,
}

/// An imported function from a WIT interface
#[derive(Debug, Clone)]
pub struct Import {
    /// Local binding name (alias from :as or function name from :refer)
    pub local_name: String,
    /// Full WIT interface path (e.g., "wasi:random/random")
    pub wit_interface: String,
    /// Function name in the WIT interface
    pub function_name: String,
    /// Parameter types
    pub params: Vec<Type>,
    /// Return type
    pub return_type: Type,
}

impl Module {
    pub fn new() -> Self {
        Self {
            imports: Vec::new(),
            functions: Vec::new(),
            globals: Vec::new(),
            strings: Vec::new(),
            protocols: Vec::new(),
            dispatch_entries: Vec::new(),
            deftypes: Vec::new(),
        }
    }

    /// Intern a string literal, returning its index
    pub fn intern_string(&mut self, s: &str) -> u32 {
        if let Some(idx) = self.strings.iter().position(|x| x == s) {
            return idx as u32;
        }
        let idx = self.strings.len() as u32;
        self.strings.push(s.to_string());
        idx
    }
}

impl Default for Module {
    fn default() -> Self {
        Self::new()
    }
}

/// A function definition
#[derive(Debug)]
pub struct Function {
    /// Function name
    pub name: String,
    /// Whether this function is exported via WIT
    pub exported: bool,
    /// Export name (may differ from internal name)
    pub export_name: Option<String>,
    /// Parameter types (name, type)
    pub params: Vec<(String, Type)>,
    /// Return type
    pub return_type: Type,
    /// Local variable types (including parameters)
    pub locals: Vec<Type>,
    /// Function body
    pub body: Expr,
}

/// A global constant
#[derive(Debug)]
pub struct Global {
    /// Name
    pub name: String,
    /// Type
    pub ty: Type,
    /// Initializer (must be constant)
    pub init: Expr,
}

/// Types supported by the compiler
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// Unit/nil type
    Unit,
    /// Boolean
    Bool,
    /// 32-bit signed integer
    I32,
    /// 64-bit signed integer
    I64,
    /// 64-bit float
    F64,
    /// String (pointer + length in linear memory)
    String,
    /// List of elements (pointer + length)
    List(Box<Type>),
    /// Fixed-size vector
    Vector(Box<Type>),
    /// Map type
    Map(Box<Type>, Box<Type>),
    /// Set type
    Set(Box<Type>),
    /// Function type
    Func {
        params: Vec<Type>,
        result: Box<Type>,
    },
    /// GC reference type (eqref in WASM GC)
    /// Used for all values in GC mode - nil, bools, ints, floats, collections
    GcRef,
    /// Unknown type (for type inference)
    Unknown,
}

impl Type {
    /// Get the WASM representation size in bytes
    pub fn wasm_size(&self) -> u32 {
        match self {
            Type::Unit => 0,
            Type::Bool | Type::I32 => 4,
            Type::I64 => 8,
            Type::F64 => 8,
            Type::String => 8, // ptr + len
            Type::List(_) => 8, // ptr + len
            Type::Vector(_) => 8, // ptr + len
            Type::Map(_, _) => 8, // ptr + len
            Type::Set(_) => 8, // ptr + len
            Type::Func { .. } => 4, // function table index
            Type::GcRef => 4, // GC reference (eqref)
            Type::Unknown => 0,
        }
    }

    /// Check if this is a numeric type
    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::I32 | Type::I64 | Type::F64)
    }
}

impl Expr {
    /// Get the type of this expression (for truthiness checks)
    ///
    /// Returns the known type when determinable at compile time,
    /// or Type::Unknown when the type cannot be statically determined.
    pub fn expr_type(&self) -> Type {
        match self {
            Expr::Unit => Type::Unit,
            Expr::Bool(_) => Type::Bool,
            Expr::Int(n) => {
                if *n >= i32::MIN as i64 && *n <= i32::MAX as i64 {
                    Type::I32
                } else {
                    Type::I64
                }
            }
            Expr::RawI32(_) => Type::I32,
            Expr::Float(_) => Type::F64,
            Expr::String(_) => Type::String,
            Expr::BinOp { ty, .. } => ty.clone(),
            Expr::UnOp { ty, .. } => ty.clone(),
            Expr::If { ty, .. } => ty.clone(),
            Expr::Coerce { to, .. } => to.clone(),
            Expr::StrConcat(_) => Type::String,
            // LocalGet/Set now carry type info
            Expr::LocalGet { ty, .. } => ty.clone(),
            Expr::LocalSet { ty, .. } => ty.clone(),
            // These require context to determine type
            Expr::GlobalGet(_) | Expr::GlobalSet(_, _) => Type::Unknown,
            Expr::Call { .. } | Expr::TailCall { .. } => Type::Unknown,
            Expr::Block(exprs) => exprs.last().map(|e| e.expr_type()).unwrap_or(Type::Unit),
            Expr::Let { body, .. } => body.expr_type(),
            Expr::Loop { body, .. } => body.expr_type(),
            Expr::Recur(_) => Type::Unknown, // Never returns normally

            // GC operations - all produce GC references or i32
            Expr::I31New(_) => Type::GcRef,
            Expr::I31GetS(_) => Type::I32,
            Expr::StructNew { .. } => Type::GcRef,
            Expr::StructGet { .. } => Type::GcRef, // Field could be any type, but in GC mode it's eqref
            Expr::ArrayNew { .. } => Type::GcRef,
            Expr::ArrayNewData { .. } => Type::GcRef,
            Expr::ArrayLen(_) => Type::GcRef, // Returns boxed i31ref
            Expr::ArrayGet { .. } => Type::GcRef,
            Expr::ArraySet { .. } => Type::GcRef, // Returns the value that was set
            Expr::ArrayNewDefault { .. } => Type::GcRef,
            Expr::ArrayClone { .. } => Type::GcRef,
            Expr::BitCount(_) => Type::GcRef, // Returns boxed i31ref
            Expr::RefTestI31(_) => Type::I32, // Boolean result
            Expr::RefTest { .. } => Type::I32, // Boolean result
            Expr::RefNull(_) => Type::GcRef,
            Expr::RefIsNull(_) => Type::I32, // Boolean result
            Expr::NilCheck(_) => Type::GcRef, // Returns boxed boolean (true/false as i31ref)

            // Persistent collections all return GC refs
            Expr::VecNew(_) => Type::GcRef,
            Expr::VecNth { .. } => Type::GcRef,
            Expr::VecConj { .. } => Type::GcRef,
            Expr::VecCount(_) => Type::I32,

            Expr::MapNew(_) => Type::GcRef,
            Expr::MapGet { .. } => Type::GcRef,
            Expr::MapAssoc { .. } => Type::GcRef,
            Expr::MapDissoc { .. } => Type::GcRef,
            Expr::MapCount(_) => Type::I32,

            Expr::SetNew(_) => Type::GcRef,
            Expr::SetContains { .. } => Type::I32, // Boolean result
            Expr::SetConj { .. } => Type::GcRef,
            Expr::SetDisj { .. } => Type::GcRef,
            Expr::SetCount(_) => Type::I32,

            Expr::ListFirst(_) => Type::GcRef,
            Expr::ListRest(_) => Type::GcRef,

            // Hash returns i32 internally, wrapped as GcRef for user code
            Expr::Hash(_) => Type::GcRef,

            // Protocol dispatch returns GcRef (the method result)
            Expr::ProtocolDispatch { .. } => Type::GcRef,
            // GetTypeId returns i32 type ID
            Expr::GetTypeId(_) => Type::I32,

            // Closure operations return GcRef
            Expr::ClosureNew { .. } => Type::GcRef,
            Expr::VariadicClosureNew { .. } => Type::GcRef,
            Expr::ClosureCall { .. } => Type::GcRef,
            Expr::Apply { .. } => Type::GcRef,

            // Type conversions
            Expr::ToFloat(_) => Type::GcRef, // Returns boxed FLOAT
        }
    }
}

/// Expressions in the IR
#[derive(Debug, Clone)]
pub enum Expr {
    /// Unit/nil literal
    Unit,

    /// Boolean literal
    Bool(bool),

    /// Integer literal (fits in i64)
    Int(i64),

    /// Raw i32 constant (not GC-encoded, for struct fields like type_id)
    RawI32(i32),

    /// Float literal
    Float(f64),

    /// String literal (index into string table)
    String(u32),

    /// Local variable reference with type info for proper truthiness
    LocalGet { local: u32, ty: Type },

    /// Local variable assignment
    LocalSet { local: u32, value: Box<Expr>, ty: Type },

    /// Global variable reference
    GlobalGet(u32),

    /// Global variable assignment
    GlobalSet(u32, Box<Expr>),

    /// Binary operation
    BinOp {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
        ty: Type,
    },

    /// Unary operation
    UnOp {
        op: UnOp,
        operand: Box<Expr>,
        ty: Type,
    },

    /// Function call
    Call {
        func: u32, // function index
        args: Vec<Expr>,
    },

    /// Tail call (uses return_call instruction for TCO)
    TailCall {
        func: u32, // function index
        args: Vec<Expr>,
    },

    /// Conditional
    If {
        cond: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
        ty: Type,
    },

    /// Sequence of expressions (returns last)
    Block(Vec<Expr>),

    /// Let binding
    Let {
        bindings: Vec<(u32, Expr)>, // local index, value
        body: Box<Expr>,
    },

    /// Loop with recur
    Loop {
        bindings: Vec<(u32, Expr)>,
        body: Box<Expr>,
    },

    /// Recur (jump back to loop) - includes target local indices
    Recur(Vec<(u32, Expr)>),

    /// String concatenation
    StrConcat(Vec<Expr>),

    /// Type coercion
    Coerce {
        expr: Box<Expr>,
        from: Type,
        to: Type,
    },

    // =========================================================================
    // WASM GC Operations
    // These use native GC types instead of tagged i64 values.
    // =========================================================================

    /// Create i31ref from i32 value (for small ints and sentinels)
    /// The i32 should already be encoded (shifted for ints, or sentinel value)
    I31New(Box<Expr>),

    /// Extract signed i32 from i31ref
    I31GetS(Box<Expr>),

    /// Create a GC struct instance
    /// Fields are evaluated in order and passed to struct.new
    StructNew {
        type_idx: u32,
        fields: Vec<Expr>,
    },

    /// Get a field from a GC struct
    StructGet {
        type_idx: u32,
        field_idx: u32,
        value: Box<Expr>,
    },

    /// Create a GC array with given elements
    ArrayNew {
        type_idx: u32,
        elements: Vec<Expr>,
    },

    /// Create a GC array from data section (for string literals)
    ArrayNewData {
        type_idx: u32,
        data_idx: u32,
        offset: Box<Expr>,
        length: Box<Expr>,
    },

    /// Get array length
    ArrayLen(Box<Expr>),

    /// Get array element at index
    ArrayGet {
        type_idx: u32,
        array: Box<Expr>,
        index: Box<Expr>,
    },

    /// Set array element at index
    ArraySet {
        type_idx: u32,
        array: Box<Expr>,
        index: Box<Expr>,
        value: Box<Expr>,
    },

    /// Create an array with default values (null/0) of given size
    ArrayNewDefault {
        type_idx: u32,
        size: Box<Expr>,
    },

    /// Clone an array (shallow copy)
    ArrayClone {
        type_idx: u32,
        array: Box<Expr>,
    },

    /// Population count (number of 1 bits) - for HAMT bitmap operations
    BitCount(Box<Expr>),

    /// Test if a reference is an i31ref (returns i32 boolean)
    RefTestI31(Box<Expr>),

    /// Test if a reference is a specific struct/array type
    RefTest {
        type_idx: u32,
        value: Box<Expr>,
    },

    /// Null reference of a given type
    RefNull(u32),

    /// Test if reference is null
    RefIsNull(Box<Expr>),

    /// Check if value is nil (compares with NIL_SENTINEL i31ref(0))
    /// Returns true (boxed) if value is nil, false otherwise
    NilCheck(Box<Expr>),

    // =========================================================================
    // Persistent Vector Operations
    // ClojureScript-style 32-way bit-partitioned trie
    // =========================================================================

    /// Create a new persistent vector from elements
    /// Builds proper trie structure based on element count
    VecNew(Vec<Expr>),

    /// Get element at index from persistent vector
    /// Uses tail optimization + trie traversal
    VecNth {
        vec: Box<Expr>,
        index: Box<Expr>,
    },

    /// Add element to end of persistent vector (returns new vector)
    /// Structural sharing via path copying
    VecConj {
        vec: Box<Expr>,
        val: Box<Expr>,
    },

    /// Get count of persistent vector
    VecCount(Box<Expr>),

    // =========================================================================
    // Persistent Map Operations
    // HAMT (Hash Array Mapped Trie)
    // =========================================================================

    /// Create a new persistent map from key-value pairs
    MapNew(Vec<(Expr, Expr)>),

    /// Get value for key from persistent map (returns nil if not found)
    MapGet {
        map: Box<Expr>,
        key: Box<Expr>,
    },

    /// Associate key with value in map (returns new map)
    MapAssoc {
        map: Box<Expr>,
        key: Box<Expr>,
        val: Box<Expr>,
    },

    /// Remove key from map (returns new map)
    MapDissoc {
        map: Box<Expr>,
        key: Box<Expr>,
    },

    /// Get count of persistent map
    MapCount(Box<Expr>),

    // =========================================================================
    // Persistent Set Operations
    // HAMT-based (same structure as map, keys only)
    // =========================================================================

    /// Create a new persistent set from elements
    SetNew(Vec<Expr>),

    /// Test if set contains element
    SetContains {
        set: Box<Expr>,
        key: Box<Expr>,
    },

    /// Add element to set (returns new set)
    SetConj {
        set: Box<Expr>,
        val: Box<Expr>,
    },

    /// Remove element from set (returns new set)
    SetDisj {
        set: Box<Expr>,
        val: Box<Expr>,
    },

    /// Get count of persistent set
    SetCount(Box<Expr>),

    // =========================================================================
    // List Operations (cons cells)
    // =========================================================================

    /// Get first element of list
    ListFirst(Box<Expr>),

    /// Get rest of list (cdr)
    ListRest(Box<Expr>),

    // =========================================================================
    // Hash Operations
    // =========================================================================

    /// Hash a value using xxHash32
    /// Returns hash code wrapped in i31ref for user code,
    /// or raw i32 for internal HAMT operations.
    Hash(Box<Expr>),

    // =========================================================================
    // Protocol Dispatch Operations
    // =========================================================================

    /// Polymorphic protocol method dispatch (slow path).
    ///
    /// At runtime:
    /// 1. Evaluates obj to get the target value
    /// 2. Calls $get-type-id to determine the type
    /// 3. Looks up (type_id, method_id) in dispatch table
    /// 4. Calls the implementation via call_ref
    ///
    /// Use this when the type is not known at compile time.
    /// For known types, use direct IR operations (VecNth, MapGet, etc.)
    ProtocolDispatch {
        /// The object to dispatch on (becomes 'this' argument)
        obj: Box<Expr>,
        /// Protocol method ID (from method_ids module)
        method_id: u32,
        /// Additional arguments after 'this'
        args: Vec<Expr>,
        /// Whether this dispatch is in tail position (enables return_call optimization)
        in_tail_position: bool,
    },

    /// Get the runtime type ID of a value.
    ///
    /// Returns an i32 type ID:
    /// - -1 for i31ref values (nil, bool, small int)
    /// - 0-8 for built-in GC types
    /// - 256+ for user-defined types
    ///
    /// Used internally by ProtocolDispatch; rarely needed directly.
    GetTypeId(Box<Expr>),

    // =========================================================================
    // First-Class Functions (Closures)
    // Uses WASM 3.0 typed function references for efficient invocation
    // =========================================================================

    /// Create a closure from a function index and captured values.
    ///
    /// The wrapper function has signature: (env: eqref, args...) -> eqref
    /// where env is an array containing the captured values.
    ///
    /// Generates:
    /// 1. Create env array from captures
    /// 2. Create Closure struct with typed funcref
    ClosureNew {
        /// Index of the wrapper function (takes env as first param)
        func_idx: u32,
        /// Number of parameters (excluding env)
        arity: u32,
        /// Expressions for values to capture in the environment
        captures: Vec<Expr>,
    },

    /// Call a closure value.
    ///
    /// At runtime:
    /// 1. Extract env and fn from closure struct
    /// 2. Push env as first argument
    /// 3. Push actual arguments
    /// 4. call_ref with typed funcref (no runtime type check!)
    ClosureCall {
        /// Expression that evaluates to a closure
        closure: Box<Expr>,
        /// Arguments to pass to the closure
        args: Vec<Expr>,
        /// Whether this call is in tail position (enables return_call_ref)
        in_tail_position: bool,
    },

    /// Create a variadic closure for builtin operators like +, *, -, /.
    ///
    /// Unlike regular closures, variadic closures contain 9 funcrefs (one per arity 0-8),
    /// allowing them to be called with any number of arguments via apply.
    ///
    /// Generates:
    /// 1. Create VARIADIC_CLOSURE struct with type_id and 9 funcrefs
    VariadicClosureNew {
        /// Which variadic builtin this is ("+", "*", "-", "/")
        op: String,
        /// Function indices for arities 0-8 (9 indices total)
        func_indices: [u32; 9],
    },

    /// Dynamic function application with argument vector.
    ///
    /// At runtime:
    /// 1. Evaluate func to get a closure
    /// 2. Evaluate args to get a vector
    /// 3. Extract vector count (determines arity)
    /// 4. Dispatch to appropriate closure call based on arity
    ///
    /// Example: (apply + [1 2 3]) calls + with three args from the vector.
    Apply {
        /// Expression that evaluates to a closure
        func: Box<Expr>,
        /// Expression that evaluates to a vector of arguments
        args: Box<Expr>,
    },

    // =========================================================================
    // Type Conversions
    // =========================================================================

    /// Convert a boxed numeric value to float (f64).
    ///
    /// At runtime:
    /// 1. Check if value is INTEGER -> extract and convert to f64
    /// 2. Check if value is FLOAT -> extract f64 directly
    /// 3. Check if value is i31ref small int -> decode and convert to f64
    ToFloat(Box<Expr>),
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Rem,

    // Comparison
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,

    // Logical
    And,
    Or,

    // Bitwise
    BitAnd,
    BitOr,
    BitXor,
    Shl,    // Left shift
    ShrS,   // Signed right shift (arithmetic)
    ShrU,   // Unsigned right shift (logical)
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[cfg(test)]
mod tests {
    use super::gc_types;

    #[test]
    fn test_gc_small_int_range() {
        // Test boundary values
        assert!(gc_types::fits_in_small_int(0));
        assert!(gc_types::fits_in_small_int(1));
        assert!(gc_types::fits_in_small_int(-1));
        assert!(gc_types::fits_in_small_int(gc_types::SMALL_INT_MAX));
        assert!(gc_types::fits_in_small_int(gc_types::SMALL_INT_MIN));

        // Just outside range
        assert!(!gc_types::fits_in_small_int(gc_types::SMALL_INT_MAX + 1));
        assert!(!gc_types::fits_in_small_int(gc_types::SMALL_INT_MIN - 1));

        // Large values
        assert!(!gc_types::fits_in_small_int(i64::MAX));
        assert!(!gc_types::fits_in_small_int(i64::MIN));
    }

    #[test]
    fn test_gc_small_int_encode_decode() {
        // Test round-trip encoding
        for n in [-1000, -1, 0, 1, 42, 1000, gc_types::SMALL_INT_MAX, gc_types::SMALL_INT_MIN] {
            let encoded = gc_types::encode_small_int(n);
            let decoded = gc_types::decode_small_int(encoded);
            assert_eq!(decoded, n, "round-trip failed for {}", n);
        }
    }

    #[test]
    fn test_gc_small_int_tag_bit() {
        // Encoded small ints should have tag bit set
        assert!(gc_types::is_small_int(gc_types::encode_small_int(0)));
        assert!(gc_types::is_small_int(gc_types::encode_small_int(42)));
        assert!(gc_types::is_small_int(gc_types::encode_small_int(-1)));

        // Sentinels should NOT have tag bit set
        assert!(!gc_types::is_small_int(gc_types::NIL_SENTINEL));
        assert!(!gc_types::is_small_int(gc_types::FALSE_SENTINEL));
        assert!(!gc_types::is_small_int(gc_types::TRUE_SENTINEL));
    }

    #[test]
    fn test_gc_sentinel_truthiness() {
        // nil and false are falsy
        assert!(!gc_types::is_truthy_sentinel(gc_types::NIL_SENTINEL));
        assert!(!gc_types::is_truthy_sentinel(gc_types::FALSE_SENTINEL));

        // true is truthy
        assert!(gc_types::is_truthy_sentinel(gc_types::TRUE_SENTINEL));

        // All small integers (with tag bit) are truthy, including 0
        assert!(gc_types::is_truthy_sentinel(gc_types::encode_small_int(0)));
        assert!(gc_types::is_truthy_sentinel(gc_types::encode_small_int(42)));
        assert!(gc_types::is_truthy_sentinel(gc_types::encode_small_int(-1)));
    }

    #[test]
    fn test_gc_type_indices() {
        // Verify type indices are unique and contiguous
        let indices = [
            // Base types (0-10)
            gc_types::LARGE_INT,
            gc_types::FLOAT,
            gc_types::STRING,
            gc_types::TRIE_NODE,
            gc_types::CONS,
            gc_types::BITMAP_INDEXED_NODE,
            gc_types::ARRAY_NODE,
            gc_types::HASH_COLLISION_NODE,
            gc_types::PERSISTENT_VECTOR,
            gc_types::PERSISTENT_MAP,
            gc_types::PERSISTENT_SET,
            // Closure function types (11-19)
            gc_types::CLOSURE_FN_0,
            gc_types::CLOSURE_FN_1,
            gc_types::CLOSURE_FN_2,
            gc_types::CLOSURE_FN_3,
            gc_types::CLOSURE_FN_4,
            gc_types::CLOSURE_FN_5,
            gc_types::CLOSURE_FN_6,
            gc_types::CLOSURE_FN_7,
            gc_types::CLOSURE_FN_8,
            // Closure struct types (20-28)
            gc_types::CLOSURE_0,
            gc_types::CLOSURE_1,
            gc_types::CLOSURE_2,
            gc_types::CLOSURE_3,
            gc_types::CLOSURE_4,
            gc_types::CLOSURE_5,
            gc_types::CLOSURE_6,
            gc_types::CLOSURE_7,
            gc_types::CLOSURE_8,
            // Variadic function types (29-37)
            gc_types::VARIADIC_FN_0,
            gc_types::VARIADIC_FN_1,
            gc_types::VARIADIC_FN_2,
            gc_types::VARIADIC_FN_3,
            gc_types::VARIADIC_FN_4,
            gc_types::VARIADIC_FN_5,
            gc_types::VARIADIC_FN_6,
            gc_types::VARIADIC_FN_7,
            gc_types::VARIADIC_FN_8,
            // Variadic closure struct (38)
            gc_types::VARIADIC_CLOSURE,
        ];
        for (i, idx) in indices.iter().enumerate() {
            assert_eq!(*idx, i as u32, "type index {} should be {}", idx, i);
        }
        assert_eq!(gc_types::NUM_GC_TYPES, indices.len() as u32);
    }

    #[test]
    fn test_closure_type_for_arity() {
        // Closure function types
        assert_eq!(gc_types::closure_fn_type_for_arity(0), gc_types::CLOSURE_FN_0);
        assert_eq!(gc_types::closure_fn_type_for_arity(1), gc_types::CLOSURE_FN_1);
        assert_eq!(gc_types::closure_fn_type_for_arity(8), gc_types::CLOSURE_FN_8);

        // Closure struct types
        assert_eq!(gc_types::closure_type_for_arity(0), gc_types::CLOSURE_0);
        assert_eq!(gc_types::closure_type_for_arity(1), gc_types::CLOSURE_1);
        assert_eq!(gc_types::closure_type_for_arity(2), gc_types::CLOSURE_2);
        assert_eq!(gc_types::closure_type_for_arity(8), gc_types::CLOSURE_8);
    }
}
