//! Intermediate Representation for the Suss compiler
//!
//! The IR is a simplified, typed representation of Suss code that maps
//! closely to WASM instructions.

/// WASM GC type definitions for Suss - Irreducible Primitives Only.
///
/// The compiler provides ONLY the types that cannot be defined in Suss itself:
/// - Numeric primitives (SmallInt via i31ref, Int64, Float64)
/// - Storage primitives (String, Array, I32Array)
/// - Closure machinery (function types and structs)
///
/// All collection types (Cons, PersistentVector, PersistentMap, etc.) and
/// extended numeric types (BigInt, Ratio, BigDecimal) are defined in core.suss
/// via deftype.
///
/// Value representation:
/// - nil, false, true: i31ref with sentinel values (0, 2, 4)
/// - small integers: i31ref (30-bit signed, encoded as (n << 1) | 1)
/// - large integers: Int64 struct (when > 30 bits)
/// - floats: Float64 struct (boxed f64)
/// - strings: array<i8> (UTF-8 bytes)
/// - arrays: array<eqref> (universal mutable storage for collections)
pub mod gc_types {
    // =========================================================================
    // GC Type Indices - IRREDUCIBLE PRIMITIVES ONLY
    // These are indices into the WASM type section, assigned during codegen.
    // All collection types are now deftypes in core.suss.
    // =========================================================================

    // === Numeric Primitives (0-1) ===

    /// struct { type_id: i32, value: i64 } - for integers that don't fit in i31ref
    pub const INT64: u32 = 0;

    /// struct { type_id: i32, value: f64 } - all floats are boxed
    pub const FLOAT64: u32 = 1;

    // === Storage Primitives (2-4) ===

    /// array<i8> - UTF-8 string bytes
    pub const STRING: u32 = 2;

    /// array<eqref> - Universal mutable storage for collections
    /// Used by: vector nodes, HAMT nodes, any array-based data structure
    pub const ARRAY: u32 = 3;

    /// array<i32> - For BigInt magnitude storage (defined in core.suss)
    pub const I32_ARRAY: u32 = 4;

    // =========================================================================
    // Closure Function Types (optimized for arities 0-4, apply-style for 5+)
    // Signature: (env: eqref, args...) -> eqref
    // =========================================================================

    /// Function type for closure arity 0: (env) -> result
    pub const CLOSURE_FN_0: u32 = 5;
    /// Function type for closure arity 1: (env, arg1) -> result
    pub const CLOSURE_FN_1: u32 = 6;
    /// Function type for closure arity 2: (env, arg1, arg2) -> result
    pub const CLOSURE_FN_2: u32 = 7;
    /// Function type for closure arity 3
    pub const CLOSURE_FN_3: u32 = 8;
    /// Function type for closure arity 4
    pub const CLOSURE_FN_4: u32 = 9;
    /// Function type for closure arity 5
    pub const CLOSURE_FN_5: u32 = 10;
    /// Function type for closure arity 6
    pub const CLOSURE_FN_6: u32 = 11;
    /// Function type for closure arity 7
    pub const CLOSURE_FN_7: u32 = 12;
    /// Function type for closure arity 8
    pub const CLOSURE_FN_8: u32 = 13;
    /// Function type for closure arity N (9+): (env, args_array) -> result
    /// Uses apply-style dispatch for higher arities
    pub const CLOSURE_FN_N: u32 = 14;

    /// Get the closure function type index for a given arity
    /// Regular closures: arities 0-4 have dedicated types, 5+ use CLOSURE_FN_N
    #[inline]
    pub const fn closure_fn_type_for_arity(arity: u32) -> u32 {
        if arity <= 4 {
            CLOSURE_FN_0 + arity
        } else {
            CLOSURE_FN_N
        }
    }

    /// Get the variadic function type index for a given arity
    /// Variadic builtins: arities 0-8 have dedicated types
    #[inline]
    pub const fn variadic_fn_type_for_arity_new(arity: u32) -> u32 {
        match arity {
            0 => CLOSURE_FN_0,
            1 => CLOSURE_FN_1,
            2 => CLOSURE_FN_2,
            3 => CLOSURE_FN_3,
            4 => CLOSURE_FN_4,
            5 => CLOSURE_FN_5,
            6 => CLOSURE_FN_6,
            7 => CLOSURE_FN_7,
            8 => CLOSURE_FN_8,
            _ => CLOSURE_FN_N,
        }
    }

    // =========================================================================
    // Closure Struct Types (per-arity for typed function references)
    // Each closure has: type_id, env (captured values), fn (typed funcref)
    // =========================================================================

    /// struct { type_id: i32, env: (ref null $array), fn: (ref $closure_fn_0) }
    pub const CLOSURE_0: u32 = 15;
    /// Closure struct with arity 1
    pub const CLOSURE_1: u32 = 16;
    /// Closure struct with arity 2
    pub const CLOSURE_2: u32 = 17;
    /// Closure struct with arity 3
    pub const CLOSURE_3: u32 = 18;
    /// Closure struct with arity 4
    pub const CLOSURE_4: u32 = 19;
    /// Closure struct for arity 5+ (uses apply-style dispatch)
    pub const CLOSURE_N: u32 = 20;

    /// Get the closure struct type index for a given arity
    /// Arities 0-4 have dedicated types, 5+ use CLOSURE_N
    #[inline]
    pub const fn closure_type_for_arity(arity: u32) -> u32 {
        if arity <= 4 {
            CLOSURE_0 + arity
        } else {
            CLOSURE_N
        }
    }

    // =========================================================================
    // Variadic Function Type (for variadic builtins like +, *, -, /)
    // Single type handles all arities via runtime dispatch
    // =========================================================================

    /// Function type for variadic functions: () -> result (arity 0 fallback)
    pub const VARIADIC_FN: u32 = 21;

    /// struct { type_id: i32, fn0..fn8: funcrefs }
    /// Variadic closure struct (for when +, *, etc. used as values)
    /// Contains typed funcrefs for arities 0-8 using CLOSURE_FN_* types
    pub const VARIADIC_CLOSURE: u32 = 22;

    // =========================================================================
    // Keyword Type
    // Keywords are interned values with pre-computed hash for O(1) equality
    // =========================================================================

    /// struct { type_id: i32, hash: i32, name_idx: i32 }
    /// Interned keyword with pre-computed hash for fast map lookups
    pub const KEYWORD: u32 = 23;

    // =========================================================================
    // Symbol Type
    // First-class symbols with optional namespace and pre-computed hash
    // =========================================================================

    /// struct { type_id: i32, hash: i32, ns_idx: i32, name_idx: i32 }
    /// First-class symbol with namespace support and pre-computed hash.
    /// - ns_idx: -1 if no namespace, otherwise index into string table
    /// - name_idx: index into string table for the symbol name
    pub const SYMBOL: u32 = 24;

    // =========================================================================
    // Var Type
    // First-class variables with metadata support
    // =========================================================================

    /// struct { type_id: i32, root: eqref, meta: eqref, sym: eqref }
    /// First-class Var containing:
    /// - root: The bound value
    /// - meta: Metadata map (or nil)
    /// - sym: The SYMBOL for this var's name
    pub const VAR: u32 = 25;

    /// Get the variadic function type index (always VARIADIC_FN)
    /// This is a compatibility shim - variadic functions now use a single type
    #[deprecated(note = "Use VARIADIC_FN directly - all arities use the same type")]
    #[inline]
    pub const fn variadic_fn_type_for_arity(_arity: u32) -> u32 {
        VARIADIC_FN
    }

    /// Number of GC types defined (for type index offset calculation)
    /// 5 primitives + 10 closure fn types + 6 closure struct types + 2 variadic + 1 keyword + 1 symbol + 1 var = 26
    pub const NUM_GC_TYPES: u32 = 26;

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
    // Variadic Closure Field Indices
    // =========================================================================

    /// Variadic closure type_id field
    pub const VC_TYPE_ID: u32 = 0;
    /// Variadic closure fn field
    pub const VC_FN: u32 = 1;

    // =========================================================================
    // Keyword Field Indices
    // =========================================================================

    /// Keyword type_id field (for protocol dispatch)
    pub const KW_TYPE_ID: u32 = 0;
    /// Keyword pre-computed hash (for fast map operations)
    pub const KW_HASH: u32 = 1;
    /// Keyword name index (into interned string table)
    pub const KW_NAME_IDX: u32 = 2;

    // =========================================================================
    // Symbol Field Indices
    // =========================================================================

    /// Symbol type_id field (for protocol dispatch)
    pub const SYM_TYPE_ID: u32 = 0;
    /// Symbol pre-computed hash (for fast map operations)
    pub const SYM_HASH: u32 = 1;
    /// Symbol namespace index (-1 for no namespace, otherwise string table index)
    pub const SYM_NS_IDX: u32 = 2;
    /// Symbol name index (into interned string table)
    pub const SYM_NAME_IDX: u32 = 3;

    // =========================================================================
    // Var Field Indices
    // =========================================================================

    /// Var type_id field (for protocol dispatch)
    pub const VAR_TYPE_ID: u32 = 0;
    /// Var root value (the bound value)
    pub const VAR_ROOT: u32 = 1;
    /// Var metadata (map or nil)
    pub const VAR_META: u32 = 2;
    /// Var symbol (the name of this var)
    pub const VAR_SYM: u32 = 3;

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

    /// Compute xxHash32 of a byte slice at compile time
    /// Used for pre-computing keyword hashes
    pub fn xxhash32(data: &[u8]) -> i32 {
        let len = data.len();
        let mut h: u32 = if len >= 16 {
            // Process 16-byte chunks
            let mut v1 = 0u32.wrapping_add(PRIME32_1).wrapping_add(PRIME32_2);
            let mut v2 = PRIME32_2;
            let mut v3 = 0u32;
            let mut v4 = 0u32.wrapping_sub(PRIME32_1);
            let mut i = 0;
            while i + 16 <= len {
                let k1 = u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
                v1 = v1
                    .wrapping_add(k1.wrapping_mul(PRIME32_2))
                    .rotate_left(13)
                    .wrapping_mul(PRIME32_1);
                let k2 =
                    u32::from_le_bytes([data[i + 4], data[i + 5], data[i + 6], data[i + 7]]);
                v2 = v2
                    .wrapping_add(k2.wrapping_mul(PRIME32_2))
                    .rotate_left(13)
                    .wrapping_mul(PRIME32_1);
                let k3 =
                    u32::from_le_bytes([data[i + 8], data[i + 9], data[i + 10], data[i + 11]]);
                v3 = v3
                    .wrapping_add(k3.wrapping_mul(PRIME32_2))
                    .rotate_left(13)
                    .wrapping_mul(PRIME32_1);
                let k4 =
                    u32::from_le_bytes([data[i + 12], data[i + 13], data[i + 14], data[i + 15]]);
                v4 = v4
                    .wrapping_add(k4.wrapping_mul(PRIME32_2))
                    .rotate_left(13)
                    .wrapping_mul(PRIME32_1);
                i += 16;
            }
            v1.rotate_left(1)
                .wrapping_add(v2.rotate_left(7))
                .wrapping_add(v3.rotate_left(12))
                .wrapping_add(v4.rotate_left(18))
        } else {
            PRIME32_5
        };

        h = h.wrapping_add(len as u32);

        // Process remaining 4-byte chunks
        let mut i = (len / 16) * 16;
        while i + 4 <= len {
            let k = u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
            h = h
                .wrapping_add(k.wrapping_mul(PRIME32_3))
                .rotate_left(17)
                .wrapping_mul(PRIME32_4);
            i += 4;
        }

        // Process remaining bytes
        while i < len {
            h = h
                .wrapping_add((data[i] as u32).wrapping_mul(PRIME32_5))
                .rotate_left(11)
                .wrapping_mul(PRIME32_1);
            i += 1;
        }

        // Final avalanche
        h ^= h >> 15;
        h = h.wrapping_mul(PRIME32_2);
        h ^= h >> 13;
        h = h.wrapping_mul(PRIME32_3);
        h ^= h >> 16;

        h as i32
    }

    /// Compute hash for a keyword (namespace/name or just name)
    pub fn hash_keyword(namespace: Option<&str>, name: &str) -> i32 {
        let s = match namespace {
            Some(ns) => format!(":{}/{}", ns, name),
            None => format!(":{}", name),
        };
        xxhash32(s.as_bytes())
    }

    /// Compute hash for a symbol (namespace/name or just name)
    pub fn hash_symbol(namespace: Option<&str>, name: &str) -> i32 {
        let s = match namespace {
            Some(ns) => format!("{}/{}", ns, name),
            None => name.to_string(),
        };
        xxhash32(s.as_bytes())
    }

    // =========================================================================
    // Primitive Type Field Indices
    // Collection types are now deftypes - their field indices come from core.suss
    // =========================================================================

    /// All dispatchable types have type_id as field 0
    /// This enables O(1) type lookup for protocol dispatch
    pub const TYPE_ID: u32 = 0;

    /// INT64 field indices
    pub const I64_VALUE: u32 = 1; // the i64 value

    /// FLOAT64 field indices
    pub const F64_VALUE: u32 = 1; // the f64 value

    // =========================================================================
    // LEGACY ALIASES - TO BE REMOVED
    // These exist for backward compatibility during the type system migration.
    // Collection types are now deftypes in core.suss, not compiler primitives.
    // =========================================================================

    // Renamed types
    #[deprecated(note = "Use INT64 instead")]
    pub const LARGE_INT: u32 = INT64;
    #[deprecated(note = "Use FLOAT64 instead")]
    pub const FLOAT: u32 = FLOAT64;
    #[deprecated(note = "Use ARRAY instead")]
    pub const TRIE_NODE: u32 = ARRAY;

    // Renamed field indices
    #[deprecated(note = "Use I64_VALUE instead")]
    pub const LI_VALUE: u32 = I64_VALUE;
    #[deprecated(note = "Use F64_VALUE instead")]
    pub const FL_VALUE: u32 = F64_VALUE;

    // Collection types - NOW DEFTYPES IN CORE.SUSS
    // These placeholders keep codegen compiling during migration.
    // They map to indices that will be overwritten by deftype registration.
    // TODO: Remove once codegen resolves these from DeftypeDefs.
    #[deprecated(note = "Now a deftype in core.suss - resolve dynamically")]
    pub const CONS: u32 = 100;
    #[deprecated(note = "Now a deftype in core.suss - resolve dynamically")]
    pub const BITMAP_INDEXED_NODE: u32 = 101;
    #[deprecated(note = "Now a deftype in core.suss - resolve dynamically")]
    pub const ARRAY_NODE: u32 = 102;
    #[deprecated(note = "Now a deftype in core.suss - resolve dynamically")]
    pub const HASH_COLLISION_NODE: u32 = 103;
    #[deprecated(note = "Now a deftype in core.suss - resolve dynamically")]
    pub const PERSISTENT_VECTOR: u32 = 104;
    #[deprecated(note = "Now a deftype in core.suss - resolve dynamically")]
    pub const PERSISTENT_MAP: u32 = 105;
    #[deprecated(note = "Now a deftype in core.suss - resolve dynamically")]
    pub const PERSISTENT_SET: u32 = 106;

    // Collection field indices are now resolved dynamically from DeftypeDef.
    // HAMT node field indices are now resolved dynamically from DeftypeDef.

    // =========================================================================
    // Function Index Offsets
    // With collection algorithms in core.suss, these are greatly reduced.
    // User functions now start right after imports and core.suss functions.
    // =========================================================================

    /// Number of runtime helper functions emitted before user functions.
    /// Reduced: only hash_string and get_type_id remain in codegen.
    pub const NUM_RUNTIME_HELPERS: u32 = 2;

    /// Number of protocol implementation wrapper functions
    /// Reduced: protocol impls now in core.suss via extend-type
    pub const NUM_PROTOCOL_IMPLS: u32 = 0;

    /// Total offset for user-defined functions (after imports)
    pub const USER_FUNC_OFFSET: u32 = NUM_RUNTIME_HELPERS + NUM_PROTOCOL_IMPLS;
}

/// Type IDs for protocol dispatch.
///
/// Compiler primitives have fixed type IDs matching their GC type indices.
/// Collection types (Cons, PersistentVector, etc.) are deftypes in core.suss
/// and get type IDs starting at USER_TYPE_BASE (256).
pub mod type_ids {
    /// i31ref values (nil, bool, small int) - not dispatchable to most protocols
    pub const I31REF: i32 = -1;

    // === Compiler Primitives (type_id = gc_type index) ===
    pub const INT64: i32 = super::gc_types::INT64 as i32;
    pub const FLOAT64: i32 = super::gc_types::FLOAT64 as i32;
    pub const STRING: i32 = super::gc_types::STRING as i32;
    pub const ARRAY: i32 = super::gc_types::ARRAY as i32;
    pub const I32_ARRAY: i32 = super::gc_types::I32_ARRAY as i32;

    // === Closure Types (for IFn protocol dispatch) ===
    pub const CLOSURE_0: i32 = super::gc_types::CLOSURE_0 as i32;
    pub const CLOSURE_1: i32 = super::gc_types::CLOSURE_1 as i32;
    pub const CLOSURE_2: i32 = super::gc_types::CLOSURE_2 as i32;
    pub const CLOSURE_3: i32 = super::gc_types::CLOSURE_3 as i32;
    pub const CLOSURE_4: i32 = super::gc_types::CLOSURE_4 as i32;
    pub const CLOSURE_N: i32 = super::gc_types::CLOSURE_N as i32;

    // Variadic closure (for variadic builtins like +, *, -, /)
    pub const VARIADIC_CLOSURE: i32 = super::gc_types::VARIADIC_CLOSURE as i32;

    // Keywords (interned with pre-computed hash)
    pub const KEYWORD: i32 = super::gc_types::KEYWORD as i32;

    // Symbols (interned with pre-computed hash and namespace support)
    pub const SYMBOL: i32 = super::gc_types::SYMBOL as i32;

    // Vars (first-class variables with metadata)
    pub const VAR: i32 = super::gc_types::VAR as i32;

    /// User-defined types start at 5 (after INT64=0, FLOAT64=1, STRING=2, ARRAY=3, I32_ARRAY=4).
    /// Type IDs 5-14 are available for deftypes (before CLOSURE_0 at 15).
    /// This includes: Cons, PersistentVector, PersistentMap, PersistentSet,
    /// BitmapIndexedNode, ArrayNode, HashCollisionNode (7 types = IDs 5-11).
    pub const USER_TYPE_BASE: i32 = 5;

    // =========================================================================
    // LEGACY TYPE IDS - TO BE REMOVED
    // Collection types are now deftypes with dynamic type IDs.
    // These placeholders keep codegen compiling during migration.
    // =========================================================================

    #[deprecated(note = "Use INT64 instead")]
    pub const LARGE_INT: i32 = INT64;
    #[deprecated(note = "Use FLOAT64 instead")]
    pub const FLOAT: i32 = FLOAT64;
    #[deprecated(note = "Use ARRAY instead")]
    pub const TRIE_NODE: i32 = ARRAY;

    #[deprecated(note = "Now a deftype - resolve dynamically")]
    pub const CONS: i32 = 260;
    #[deprecated(note = "Now a deftype - resolve dynamically")]
    pub const BITMAP_INDEXED_NODE: i32 = 261;
    #[deprecated(note = "Now a deftype - resolve dynamically")]
    pub const ARRAY_NODE: i32 = 262;
    #[deprecated(note = "Now a deftype - resolve dynamically")]
    pub const HASH_COLLISION_NODE: i32 = 263;
    #[deprecated(note = "Now a deftype - resolve dynamically")]
    pub const PERSISTENT_VECTOR: i32 = 264;
    #[deprecated(note = "Now a deftype - resolve dynamically")]
    pub const PERSISTENT_MAP: i32 = 265;
    #[deprecated(note = "Now a deftype - resolve dynamically")]
    pub const PERSISTENT_SET: i32 = 266;
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

/// A dispatch table entry mapping (dispatch_slot, method_id) to a function
#[derive(Debug, Clone)]
pub struct DispatchEntry {
    /// Dispatch table slot (0-4 for primitives, 5+ for deftypes in definition order)
    pub dispatch_slot: u32,
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
    /// Interned keywords (namespace, name) pairs
    pub keywords: Vec<(Option<String>, String)>,
    /// Interned symbols (namespace, name) pairs
    pub symbols: Vec<(Option<String>, String)>,
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
            keywords: Vec::new(),
            symbols: Vec::new(),
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

    /// Intern a keyword, returning its index
    /// Keywords with the same namespace and name return the same index
    pub fn intern_keyword(&mut self, namespace: Option<&str>, name: &str) -> u32 {
        let ns_owned = namespace.map(|s| s.to_string());
        if let Some(idx) = self
            .keywords
            .iter()
            .position(|(ns, n)| ns.as_deref() == namespace && n == name)
        {
            return idx as u32;
        }
        let idx = self.keywords.len() as u32;
        self.keywords.push((ns_owned, name.to_string()));
        idx
    }

    /// Intern a symbol, returning its index
    /// Symbols with the same namespace and name return the same index
    pub fn intern_symbol(&mut self, namespace: Option<&str>, name: &str) -> u32 {
        let ns_owned = namespace.map(|s| s.to_string());
        if let Some(idx) = self
            .symbols
            .iter()
            .position(|(ns, n)| ns.as_deref() == namespace && n == name)
        {
            return idx as u32;
        }
        let idx = self.symbols.len() as u32;
        self.symbols.push((ns_owned, name.to_string()));
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
    /// Parameter types (name, type) - for variadic fns, these are the fixed params
    pub params: Vec<(String, Type)>,
    /// Rest parameter name for variadic functions (e.g., "args" in [a b & args])
    pub rest_param: Option<String>,
    /// Return type
    pub return_type: Type,
    /// Whether the return type was explicitly specified via ^type hint
    /// When true, codegen emits unboxed primitive types (i32, i64, f64)
    /// When false, all returns are boxed as eqref
    pub has_explicit_return_type: bool,
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
            Expr::Unbox32(_) => Type::I32,
            Expr::Float(_) => Type::F64,
            Expr::String(_) => Type::String,
            Expr::Keyword { .. } => Type::GcRef,
            Expr::Symbol { .. } => Type::GcRef,
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
            Expr::RefCastI31(_) => Type::GcRef, // Ref cast to i31 returns GcRef (specifically i31ref subtype)
            Expr::StructNew { .. } => Type::GcRef,
            Expr::StructGet { .. } => Type::GcRef, // Field could be any type, but in GC mode it's eqref
            Expr::StructGetI32 { .. } => Type::GcRef, // Returns encoded i31ref
            Expr::ArrayNew { .. } => Type::GcRef,
            Expr::ArrayNewData { .. } => Type::GcRef,
            Expr::ArrayLen(_) => Type::GcRef, // Returns boxed i31ref
            Expr::ArrayGet { .. } => Type::GcRef,
            Expr::ArraySet { .. } => Type::GcRef, // Returns the value that was set
            Expr::ArrayNewDefault { .. } => Type::GcRef,
            Expr::ArrayClone { .. } => Type::GcRef,
            Expr::ArrayCopy { .. } => Type::GcRef, // Returns nil (side-effect only)
            Expr::BitCount(_) => Type::GcRef, // Returns boxed i31ref
            Expr::RefTestI31(_) => Type::I32, // Boolean result
            Expr::RefTest { .. } => Type::I32, // Boolean result
            Expr::RefNull(_) => Type::GcRef,
            Expr::RefIsNull(_) => Type::I32, // Boolean result
            Expr::NilCheck(_) => Type::GcRef, // Returns boxed boolean (true/false as i31ref)

            // Persistent collections all return GC refs
            Expr::VecNew(_) => Type::GcRef,
            Expr::VecNth { .. } => Type::GcRef,
            Expr::VecCount(_) => Type::I32,

            Expr::MapNew(_) => Type::GcRef,
            Expr::MapCount(_) => Type::I32,

            Expr::SetNew(_) => Type::GcRef,
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

            // Symbol/keyword introspection
            Expr::GetName(_) => Type::GcRef, // Returns STRING
            Expr::GetNamespace(_) => Type::GcRef, // Returns STRING or nil
            Expr::SymbolFromString { .. } => Type::GcRef, // Returns SYMBOL

            // Var operations
            Expr::VarNew { .. } => Type::GcRef, // Returns VAR
            Expr::VarDeref(_) => Type::GcRef, // Returns the var's root value
            Expr::VarMeta(_) => Type::GcRef, // Returns metadata map or nil
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

    /// Unbox a tagged integer to raw i32: (>> (i31.get_s (ref.cast i31 x)) 1)
    /// Used for extracting values for ^i32 struct fields
    Unbox32(Box<Expr>),

    /// Float literal
    Float(f64),

    /// String literal (index into string table)
    String(u32),

    /// Keyword literal (index into keyword table, hash pre-computed)
    Keyword { idx: u32, hash: i32 },

    /// Symbol literal with namespace support
    /// - hash: pre-computed xxHash32 for map key usage
    /// - ns_str_idx: -1 for no namespace, otherwise index into Module.strings
    /// - name_str_idx: index into Module.strings for the symbol name
    Symbol { hash: i32, ns_str_idx: i32, name_str_idx: u32 },

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

    /// Cast an eqref to i31ref (ref.cast i31)
    /// Used when we know the value is an i31 but have an eqref
    RefCastI31(Box<Expr>),

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

    /// Get an i32 field from a GC struct and encode as tagged i31ref
    /// This is used for user-defined type fields with ^i32 type annotation.
    /// Codegen will: struct.get -> encode (value << 1) | 1 -> ref.i31
    StructGetI32 {
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

    /// Copy elements between arrays (WASM GC array.copy)
    /// Returns nil (the statement has side effects only)
    ArrayCopy {
        type_idx: u32,
        dst: Box<Expr>,
        dst_offset: Box<Expr>,
        src: Box<Expr>,
        src_offset: Box<Expr>,
        len: Box<Expr>,
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

    /// Get count of persistent vector
    VecCount(Box<Expr>),

    // =========================================================================
    // Persistent Map Operations
    // HAMT (Hash Array Mapped Trie)
    // =========================================================================

    /// Create a new persistent map from key-value pairs
    MapNew(Vec<(Expr, Expr)>),

    // MapDissoc removed - now uses core.suss dissoc function

    /// Get count of persistent map
    MapCount(Box<Expr>),

    // =========================================================================
    // Persistent Set Operations
    // HAMT-based (same structure as map, keys only)
    // =========================================================================

    /// Create a new persistent set from elements
    SetNew(Vec<Expr>),

    // SetDisj removed - now uses core.suss disj function

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

    // =========================================================================
    // Symbol/Keyword Introspection
    // =========================================================================

    /// Get the name string from a symbol or keyword.
    /// For symbols: extracts name_str_idx and returns that string
    /// For keywords: extracts name from keyword table
    GetName(Box<Expr>),

    /// Get the namespace string from a symbol or keyword.
    /// For symbols: extracts ns_str_idx (-1 means nil)
    /// For keywords: extracts namespace from keyword table
    /// Returns nil if no namespace.
    GetNamespace(Box<Expr>),

    /// Create a symbol from string(s).
    /// - ns: Optional namespace string expression
    /// - name: Name string expression
    SymbolFromString {
        ns: Option<Box<Expr>>,
        name: Box<Expr>,
    },

    // =========================================================================
    // Var Operations
    // =========================================================================

    /// Create a new Var.
    /// - root: The initial bound value
    /// - meta: Metadata map (or nil)
    /// - sym: The symbol naming this var
    VarNew {
        root: Box<Expr>,
        meta: Box<Expr>,
        sym: Box<Expr>,
    },

    /// Dereference a Var to get its root value.
    VarDeref(Box<Expr>),

    /// Get metadata from a Var.
    VarMeta(Box<Expr>),
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
        // Verify type indices are unique and contiguous (26 types total)
        let indices = [
            // Primitive types (0-4)
            gc_types::INT64,
            gc_types::FLOAT64,
            gc_types::STRING,
            gc_types::ARRAY,
            gc_types::I32_ARRAY,
            // Closure function types (5-14)
            gc_types::CLOSURE_FN_0,
            gc_types::CLOSURE_FN_1,
            gc_types::CLOSURE_FN_2,
            gc_types::CLOSURE_FN_3,
            gc_types::CLOSURE_FN_4,
            gc_types::CLOSURE_FN_5,
            gc_types::CLOSURE_FN_6,
            gc_types::CLOSURE_FN_7,
            gc_types::CLOSURE_FN_8,
            gc_types::CLOSURE_FN_N,
            // Closure struct types (15-20)
            gc_types::CLOSURE_0,
            gc_types::CLOSURE_1,
            gc_types::CLOSURE_2,
            gc_types::CLOSURE_3,
            gc_types::CLOSURE_4,
            gc_types::CLOSURE_N,
            // Variadic types (21-22)
            gc_types::VARIADIC_FN,
            gc_types::VARIADIC_CLOSURE,
            // Keyword type (23)
            gc_types::KEYWORD,
            // Symbol type (24)
            gc_types::SYMBOL,
            // Var type (25)
            gc_types::VAR,
        ];
        for (i, idx) in indices.iter().enumerate() {
            assert_eq!(*idx, i as u32, "type index {} should be {}", idx, i);
        }
        assert_eq!(gc_types::NUM_GC_TYPES, indices.len() as u32);
    }

    #[test]
    fn test_closure_type_for_arity() {
        // Regular closure function types (0-4 have dedicated types, 5+ use CLOSURE_FN_N)
        assert_eq!(gc_types::closure_fn_type_for_arity(0), gc_types::CLOSURE_FN_0);
        assert_eq!(gc_types::closure_fn_type_for_arity(1), gc_types::CLOSURE_FN_1);
        assert_eq!(gc_types::closure_fn_type_for_arity(4), gc_types::CLOSURE_FN_4);
        assert_eq!(gc_types::closure_fn_type_for_arity(5), gc_types::CLOSURE_FN_N);
        assert_eq!(gc_types::closure_fn_type_for_arity(8), gc_types::CLOSURE_FN_N);

        // Variadic function types (0-8 have dedicated types, 9+ use CLOSURE_FN_N)
        assert_eq!(gc_types::variadic_fn_type_for_arity_new(0), gc_types::CLOSURE_FN_0);
        assert_eq!(gc_types::variadic_fn_type_for_arity_new(1), gc_types::CLOSURE_FN_1);
        assert_eq!(gc_types::variadic_fn_type_for_arity_new(4), gc_types::CLOSURE_FN_4);
        assert_eq!(gc_types::variadic_fn_type_for_arity_new(5), gc_types::CLOSURE_FN_5);
        assert_eq!(gc_types::variadic_fn_type_for_arity_new(8), gc_types::CLOSURE_FN_8);
        assert_eq!(gc_types::variadic_fn_type_for_arity_new(9), gc_types::CLOSURE_FN_N);

        // Closure struct types (0-4 have dedicated types, 5+ use CLOSURE_N)
        assert_eq!(gc_types::closure_type_for_arity(0), gc_types::CLOSURE_0);
        assert_eq!(gc_types::closure_type_for_arity(1), gc_types::CLOSURE_1);
        assert_eq!(gc_types::closure_type_for_arity(4), gc_types::CLOSURE_4);
        assert_eq!(gc_types::closure_type_for_arity(5), gc_types::CLOSURE_N);
        assert_eq!(gc_types::closure_type_for_arity(8), gc_types::CLOSURE_N);
    }
}
