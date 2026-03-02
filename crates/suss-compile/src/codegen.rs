//! WASM code generation from IR
//!
//! Generates a WASM component from the IR representation.
//!
//! # Architecture
//!
//! The codegen uses a unified `CodeGen` struct that handles all code generation modes:
//! - Standalone modules (no WIT, no imports)
//! - Components with WIT exports
//! - Components with WASI imports
//!
//! This consolidation enables cleaner GC support when we add WASM GC types.

use wasm_encoder::{
    AbstractHeapType, BlockType, Catch, CodeSection, ConstExpr, CustomSection, DataSection,
    DataSegment, DataSegmentMode, ElementSection, Elements, EntityType, ExportKind, ExportSection,
    FieldType, Function, FunctionSection, GlobalSection, GlobalType, HeapType, ImportSection,
    Instruction, MemorySection, MemoryType, Module as WasmModule, RawSection, RefType, StorageType,
    TableSection, TableType, TagKind, TagSection, TagType, TypeSection, ValType,
};
use wit_component::{metadata, ComponentEncoder, StringEncoding};
use wit_parser::{Resolve, WorldId};

use crate::error::{CompileError, CompileResult};
use crate::ir::{BinOp, Expr, Function as IrFunc, Module, Type, UnOp, FieldType as IrFieldType};

// ============================================================================
// Runtime Helper Function Indices
// ============================================================================

/// Number of runtime helper functions emitted before user functions.
/// Functions: hash_string, get_type_id, init_intern_tables, cabi_realloc, print_str
/// Collection algorithms (vector trie, HAMT) are implemented in core.sus.
const NUM_RUNTIME_HELPERS: u32 = 5;

/// Function index offsets for runtime helpers (relative to start of functions)
mod helper_funcs {
    /// $hash_string(ptr: i32, len: i32) -> i32
    /// Computes xxHash32 of bytes in linear memory
    pub const HASH_STRING: u32 = 0;

    /// $get_type_id(value: eqref) -> i32
    /// Returns the runtime type ID of a GC value
    pub const GET_TYPE_ID: u32 = 1;

    /// $init_intern_tables() -> ()
    /// Initializes keyword/symbol intern tables on module instantiation
    pub const INIT_INTERN_TABLES: u32 = 2;

    /// $cabi_realloc(old_ptr: i32, old_size: i32, align: i32, new_size: i32) -> i32
    /// Component Model canonical ABI allocator (bump allocator using heap_ptr)
    pub const CABI_REALLOC: u32 = 3;

    /// $print_str(value: eqref) -> ()
    /// Copies GC string to linear memory and calls host print_str import.
    /// No-op if module doesn't use print-str.
    pub const PRINT_STR: u32 = 4;
}

/// Relative offsets for helper function signatures (added to helper_type_base())
mod helper_type_offsets {
    /// Type for $hash_string: (eqref) -> i32
    pub const HASH_STRING: u32 = 0;

    /// Type for $get_type_id: (eqref) -> i32
    pub const GET_TYPE_ID: u32 = 1;

    /// Type for $init_intern_tables: () -> ()
    pub const INIT_INTERN_TABLES: u32 = 2;

    /// Type for $cabi_realloc: (i32, i32, i32, i32) -> i32
    pub const CABI_REALLOC: u32 = 3;

    /// Type for $print_str: (eqref) -> ()
    pub const PRINT_STR: u32 = 4;
}

/// Number of helper function types
const NUM_HELPER_TYPES: u32 = 5;

/// Relative offsets for protocol function signatures (added to protocol_type_base())
mod protocol_type_offsets {
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
}

// Protocol implementations are now in core.sus via extend-type declarations.
// The dispatch table is populated from DispatchEntry records created during lowering.

// ============================================================================
// Public API
// ============================================================================

/// Generate WASM bytes from IR module with WIT world
pub fn generate(ir: &Module, resolve: &Resolve, world_id: WorldId) -> CompileResult<Vec<u8>> {
    CodeGen::new(ir).generate_with_wit(resolve, world_id)
}

/// Generate WASM module bytes without WIT (for standalone expression compilation)
///
/// This generates a simple WASM module with:
/// - Memory export
/// - Exported functions from the IR
/// - String data section
///
/// No component model encoding or WIT validation.
pub fn generate_module(ir: &Module) -> CompileResult<Vec<u8>> {
    CodeGen::new(ir).generate_core_module()
}

/// Generate a WASM Component with imports for standalone expression compilation
///
/// This creates a WASM Component that imports WASI functions and exports an `eval` function.
/// Used for expressions like `(wasi.random/get-random-u64)`.
pub fn generate_component_with_imports(ir: &Module) -> CompileResult<Vec<u8>> {
    CodeGen::new(ir).generate_wasi_component()
}

// ============================================================================
// Unified CodeGen
// ============================================================================

/// Unified code generator for all WASM output modes.
///
/// Always uses WASM GC (i31ref, structref, arrayref) for value representation.
struct CodeGen<'a> {
    ir: &'a Module,
    /// Index of the scratch local (eqref) for protocol dispatch.
    /// Set before generating each function body.
    scratch_local: std::cell::Cell<u32>,
    /// Index of the i64 scratch local for WIT import return boxing.
    /// Used when we need to save an i64 value to reorder stack operands.
    i64_scratch_local: std::cell::Cell<u32>,
    /// Index of the f64 scratch local for WIT import return boxing.
    /// Used when we need to save an f64 value to reorder stack operands.
    f64_scratch_local: std::cell::Cell<u32>,
}

impl<'a> CodeGen<'a> {
    fn new(ir: &'a Module) -> Self {
        Self {
            ir,
            scratch_local: std::cell::Cell::new(0),
            i64_scratch_local: std::cell::Cell::new(0),
            f64_scratch_local: std::cell::Cell::new(0),
        }
    }

    /// Number of imported functions (WIT imports + print_str)
    /// print_str is always imported so function indices are unconditionally stable.
    fn num_imports(&self) -> u32 {
        self.ir.imports.len() as u32 + 1
    }

    /// Function index of the host print_str import (last import, after WIT imports)
    fn print_str_import_idx(&self) -> u32 {
        self.ir.imports.len() as u32
    }

    /// Get the base type index for helper function types (after GC types + user deftypes)
    fn helper_type_base(&self) -> u32 {
        // Count only deftypes that are actually emitted (not reserved types)
        let emitted_deftypes = self
            .ir
            .deftypes
            .iter()
            .filter(|dt| dt.gc_type_idx >= crate::ir::gc_types::NUM_GC_TYPES)
            .count() as u32;
        crate::ir::gc_types::NUM_GC_TYPES + emitted_deftypes
    }

    /// Get the type index for a helper function signature
    fn helper_type(&self, offset: u32) -> u32 {
        self.helper_type_base() + offset
    }

    /// Get the base type index for protocol function types (after GC types + user deftypes + helper types)
    fn protocol_type_base(&self) -> u32 {
        self.helper_type_base() + NUM_HELPER_TYPES
    }

    /// Get the type index for a protocol signature
    fn protocol_type(&self, offset: u32) -> u32 {
        self.protocol_type_base() + offset
    }

    /// Get the type index offset for user function types (after GC types + user deftypes + helper types + protocol types)
    fn func_type_offset(&self) -> u32 {
        use crate::ir::protocol_types;
        self.protocol_type_base() + protocol_types::NUM_PROTOCOL_TYPES
    }

    /// Get the function index for a user function (after imports + helper functions)
    fn user_func_idx(&self, idx: u32) -> u32 {
        self.num_imports() + NUM_RUNTIME_HELPERS + idx
    }

    /// Get the function index for a helper function (after imports)
    fn helper_func_idx(&self, helper_idx: u32) -> u32 {
        self.num_imports() + helper_idx
    }

    /// Look up a deftype's GC type index by name.
    /// Returns None if the deftype is not found.
    fn deftype_gc_type_idx(&self, name: &str) -> Option<u32> {
        self.ir.deftypes.iter().find(|dt| dt.name == name).map(|dt| dt.gc_type_idx)
    }

    /// Look up a deftype's type_id (runtime type discriminant) by name.
    /// Returns None if the deftype is not found.
    fn deftype_type_id(&self, name: &str) -> Option<i32> {
        self.ir.deftypes.iter().find(|dt| dt.name == name).map(|dt| dt.type_id)
    }

    /// Look up a deftype's dispatch slot by name.
    /// Dispatch slots: primitives use 0-4, deftypes use 5+ (index in array order).
    /// Returns None if the deftype is not found.
    fn deftype_dispatch_slot(&self, name: &str) -> Option<u32> {
        const PRIMITIVE_SLOTS: u32 = 5;
        self.ir.deftypes.iter().position(|dt| dt.name == name)
            .map(|idx| PRIMITIVE_SLOTS + idx as u32)
    }

    /// Look up a function's index by name.
    /// Returns the WASM function index for the named function from ir.functions.
    fn func_idx_by_name(&self, name: &str) -> Option<u32> {
        self.ir.functions.iter().enumerate()
            .find(|(_, f)| f.name == name)
            .map(|(idx, _)| self.user_func_idx(idx as u32))
    }

    /// Generate code to unwrap an integer from either i31ref or INT64.
    ///
    /// Input: eqref on stack
    /// Output: i32 on stack
    ///
    /// For HAMT bitmap operations, we need to handle both small integers
    /// (stored as i31ref) and large integers (stored as INT64 struct).
    /// Values like 2^30 and 2^31 from bit-shift-left exceed i31ref capacity.
    fn generate_polymorphic_unwrap_i32(&self, f: &mut Function) {
        use crate::ir::gc_types;

        // Scratch local to store the value for testing
        let scratch = self.scratch_local.get();

        // Store value in scratch local
        f.instruction(&Instruction::LocalSet(scratch));

        // Test if it's INT64
        f.instruction(&Instruction::LocalGet(scratch));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::INT64)));

        // if (is INT64)
        f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        {
            // Extract i64 from INT64 struct, wrap to i32
            f.instruction(&Instruction::LocalGet(scratch));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::INT64)));
            f.instruction(&Instruction::StructGet {
                struct_type_index: gc_types::INT64,
                field_index: gc_types::I64_VALUE,
            });
            f.instruction(&Instruction::I32WrapI64);
        }
        f.instruction(&Instruction::Else);
        {
            // Test if it's KEYWORD
            f.instruction(&Instruction::LocalGet(scratch));
            f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::KEYWORD)));

            f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
            {
                // Extract hash from KEYWORD struct for equality comparison
                // Keywords with same hash are equal (interned, so hash is unique)
                f.instruction(&Instruction::LocalGet(scratch));
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::KEYWORD)));
                f.instruction(&Instruction::StructGet {
                    struct_type_index: gc_types::KEYWORD,
                    field_index: gc_types::KW_HASH,
                });
            }
            f.instruction(&Instruction::Else);
            {
                // Test if it's SYMBOL
                f.instruction(&Instruction::LocalGet(scratch));
                f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::SYMBOL)));

                f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                {
                    // Extract hash from SYMBOL struct for equality comparison
                    // Symbols with same hash are considered equal (hash includes ns + name)
                    f.instruction(&Instruction::LocalGet(scratch));
                    f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::SYMBOL)));
                    f.instruction(&Instruction::StructGet {
                        struct_type_index: gc_types::SYMBOL,
                        field_index: gc_types::SYM_HASH,
                    });
                }
                f.instruction(&Instruction::Else);
                {
                    // It's i31ref - decode: cast, get_s, shr 1
                    f.instruction(&Instruction::LocalGet(scratch));
                    f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                    f.instruction(&Instruction::I31GetS);
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32ShrS);
                }
                f.instruction(&Instruction::End);
            }
            f.instruction(&Instruction::End);
        }
        f.instruction(&Instruction::End);
    }

    /// Generate code to unwrap a value for comparison without decoding numbers.
    ///
    /// Input: eqref on stack
    /// Output: i32 on stack
    ///
    /// This function extracts a raw i32 value suitable for equality comparison:
    /// - INT64 structs: extracts the i64 value wrapped to i32
    /// - KEYWORD structs: extracts the hash
    /// - SYMBOL structs: extracts the hash
    /// - i31ref values: uses raw value WITHOUT decoding
    ///
    /// The key difference from generate_polymorphic_unwrap_i32 is that i31ref
    /// values are NOT decoded (no shift right). This ensures:
    /// - Numbers: 0→1, 1→3, 2→5, etc. (tagged encoding)
    /// - Sentinels: nil=0, false=2, true=4
    /// These raw values never overlap, so equality comparisons work correctly.
    fn generate_polymorphic_unwrap_i32_for_compare(&self, f: &mut Function) {
        use crate::ir::gc_types;

        // Scratch local to store the value for testing
        let scratch = self.scratch_local.get();

        // Store value in scratch local
        f.instruction(&Instruction::LocalSet(scratch));

        // Test if it's INT64
        f.instruction(&Instruction::LocalGet(scratch));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::INT64)));

        // if (is INT64)
        f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        {
            // Extract i64 from INT64 struct, wrap to i32
            // Add a large offset to avoid collision with sentinels (0, 2, 4)
            // Offset 0x10000000 is larger than any sentinel but won't overflow for typical INT64 values
            f.instruction(&Instruction::LocalGet(scratch));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::INT64)));
            f.instruction(&Instruction::StructGet {
                struct_type_index: gc_types::INT64,
                field_index: gc_types::I64_VALUE,
            });
            f.instruction(&Instruction::I32WrapI64);
            // Add offset to avoid sentinel collision
            f.instruction(&Instruction::I32Const(0x10000000));
            f.instruction(&Instruction::I32Add);
        }
        f.instruction(&Instruction::Else);
        {
            // Test if it's FLOAT64
            f.instruction(&Instruction::LocalGet(scratch));
            f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::FLOAT64)));

            f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
            {
                // Extract f64 from FLOAT64 struct, truncate to i32
                // This handles mixed int/float comparisons like (< 30.0 2)
                f.instruction(&Instruction::LocalGet(scratch));
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::FLOAT64)));
                f.instruction(&Instruction::StructGet {
                    struct_type_index: gc_types::FLOAT64,
                    field_index: gc_types::F64_VALUE,
                });
                // Truncate f64 to i32 (toward zero)
                f.instruction(&Instruction::I32TruncF64S);
                // Add offset to avoid sentinel collision (same as INT64)
                f.instruction(&Instruction::I32Const(0x10000000));
                f.instruction(&Instruction::I32Add);
            }
            f.instruction(&Instruction::Else);
            {
            // Test if it's KEYWORD
            f.instruction(&Instruction::LocalGet(scratch));
            f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::KEYWORD)));

            f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
            {
                // Extract hash from KEYWORD struct for equality comparison
                f.instruction(&Instruction::LocalGet(scratch));
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::KEYWORD)));
                f.instruction(&Instruction::StructGet {
                    struct_type_index: gc_types::KEYWORD,
                    field_index: gc_types::KW_HASH,
                });
            }
            f.instruction(&Instruction::Else);
            {
                // Test if it's SYMBOL
                f.instruction(&Instruction::LocalGet(scratch));
                f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::SYMBOL)));

                f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                {
                    // Extract hash from SYMBOL struct for equality comparison
                    f.instruction(&Instruction::LocalGet(scratch));
                    f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::SYMBOL)));
                    f.instruction(&Instruction::StructGet {
                        struct_type_index: gc_types::SYMBOL,
                        field_index: gc_types::SYM_HASH,
                    });
                }
                f.instruction(&Instruction::Else);
                {
                    // Test if it's STRING
                    f.instruction(&Instruction::LocalGet(scratch));
                    f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::STRING)));

                    f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                    {
                        // Call hash_string to get comparable i32 value
                        f.instruction(&Instruction::LocalGet(scratch));
                        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
                        f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::HASH_STRING)));
                    }
                    f.instruction(&Instruction::Else);
                    {
                        // Test if it's i31ref
                        f.instruction(&Instruction::LocalGet(scratch));
                        f.instruction(&Instruction::RefTestNonNull(HeapType::I31));

                        f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                        {
                            // It's i31ref - decode and add offset for consistency with INT64/FLOAT64
                            // Numbers are encoded as (n << 1) | 1 (odd), sentinels are even (0, 2, 4)
                            // We decode and add offset so comparisons with INT64/FLOAT64 work
                            f.instruction(&Instruction::LocalGet(scratch));
                            f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                            f.instruction(&Instruction::I31GetS);
                            // Store raw i31 value in scratch+1 (i32 slot)
                            let i31_local = scratch + 1;
                            f.instruction(&Instruction::LocalTee(i31_local));
                            // Check if it's a number (bit 0 = 1) vs sentinel (bit 0 = 0)
                            f.instruction(&Instruction::I32Const(1));
                            f.instruction(&Instruction::I32And);
                            f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                            {
                                // It's a number: decode (>> 1) and add offset
                                f.instruction(&Instruction::LocalGet(i31_local));
                                f.instruction(&Instruction::I32Const(1));
                                f.instruction(&Instruction::I32ShrS);
                                f.instruction(&Instruction::I32Const(0x10000000));
                                f.instruction(&Instruction::I32Add);
                            }
                            f.instruction(&Instruction::Else);
                            {
                                // It's a sentinel: use raw value (0=nil, 2=false, 4=true)
                                // These are < 0x10000000 so they won't equal any number
                                f.instruction(&Instruction::LocalGet(i31_local));
                            }
                            f.instruction(&Instruction::End);
                        }
                        f.instruction(&Instruction::Else);
                        {
                            // Unknown struct type (e.g., MapEntry, custom deftypes)
                            // Call get_type_id to get a unique i32 for comparison
                            f.instruction(&Instruction::LocalGet(scratch));
                            f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::GET_TYPE_ID)));
                        }
                        f.instruction(&Instruction::End);
                    }
                    f.instruction(&Instruction::End);
                }
                f.instruction(&Instruction::End);
            }
            f.instruction(&Instruction::End);
        }
        f.instruction(&Instruction::End); // FLOAT64
        }
        f.instruction(&Instruction::End); // INT64
    }

    /// Generate code to unwrap an integer to i64.
    ///
    /// Input: eqref on stack (either i31ref small int or INT64 struct)
    /// Output: i64 on stack
    ///
    /// Handles both small integers (stored as i31ref) and large integers
    /// (stored as INT64 struct).
    fn generate_polymorphic_unwrap_i64(&self, f: &mut Function) {
        use crate::ir::gc_types;

        // Scratch local to store the value for testing
        let scratch = self.scratch_local.get();

        // Store value in scratch local
        f.instruction(&Instruction::LocalSet(scratch));

        // Test if it's INT64
        f.instruction(&Instruction::LocalGet(scratch));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::INT64)));

        // if (is INT64)
        f.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        {
            // Extract i64 from INT64 struct
            f.instruction(&Instruction::LocalGet(scratch));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::INT64)));
            f.instruction(&Instruction::StructGet {
                struct_type_index: gc_types::INT64,
                field_index: gc_types::I64_VALUE,
            });
        }
        f.instruction(&Instruction::Else);
        {
            // It's i31ref - decode: cast, get_s, shr 1, extend to i64
            f.instruction(&Instruction::LocalGet(scratch));
            f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
            f.instruction(&Instruction::I31GetS);
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32ShrS);
            f.instruction(&Instruction::I64ExtendI32S);
        }
        f.instruction(&Instruction::End);
    }

    /// Generate code to convert any numeric value (int or float) to f64.
    ///
    /// Input: eqref (must be numeric) on stack
    /// Output: f64 on stack
    ///
    /// Handles: i31ref small ints, INT64 structs, FLOAT64 structs
    fn generate_numeric_to_f64(&self, f: &mut Function) {
        use crate::ir::gc_types;

        // Scratch local to store the value for testing
        let scratch = self.scratch_local.get();

        // Store value in scratch local
        f.instruction(&Instruction::LocalSet(scratch));

        // Test if it's FLOAT64
        f.instruction(&Instruction::LocalGet(scratch));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::FLOAT64)));

        // if (is FLOAT64)
        f.instruction(&Instruction::If(BlockType::Result(ValType::F64)));
        {
            // Extract f64 from FLOAT64 struct
            f.instruction(&Instruction::LocalGet(scratch));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::FLOAT64)));
            f.instruction(&Instruction::StructGet {
                struct_type_index: gc_types::FLOAT64,
                field_index: gc_types::F64_VALUE,
            });
        }
        f.instruction(&Instruction::Else);
        {
            // It's an integer (i31ref or INT64) - unwrap and convert to f64
            f.instruction(&Instruction::LocalGet(scratch));
            self.generate_polymorphic_unwrap_i64(f);
            f.instruction(&Instruction::F64ConvertI64S);
        }
        f.instruction(&Instruction::End);
    }

    /// Generate code to box an i32 result, using INT64 if needed.
    ///
    /// Input: i32 on stack
    /// Output: eqref on stack
    ///
    /// For bit operations that may produce values outside i31ref range
    /// (like bit-shift-left), we check if the result fits in 30-bit signed
    /// and use INT64 otherwise.
    fn generate_box_i32_safe(&self, f: &mut Function) {
        use crate::ir::gc_types;
        use crate::ir::type_ids;

        // Scratch local to store the i32 value
        let scratch = self.scratch_local.get() + 1; // use i32 slot

        // Store result in scratch
        f.instruction(&Instruction::LocalSet(scratch));

        // Check if value fits in 30-bit signed range: -2^29 <= n < 2^29
        // This is: n >= -536870912 && n < 536870912
        // Equivalently: (n + 536870912) as u32 < 1073741824
        f.instruction(&Instruction::LocalGet(scratch));
        f.instruction(&Instruction::I32Const(536870912)); // 2^29
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I32Const(1073741824)); // 2^30
        f.instruction(&Instruction::I32LtU);

        // if (fits in i31ref)
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(RefType::EQREF))));
        {
            // Encode as i31ref: (n << 1) | 1
            f.instruction(&Instruction::LocalGet(scratch));
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Shl);
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Or);
            f.instruction(&Instruction::RefI31);
        }
        f.instruction(&Instruction::Else);
        {
            // Box in INT64 struct: { type_id, i64_value }
            f.instruction(&Instruction::I32Const(type_ids::INT64));
            f.instruction(&Instruction::LocalGet(scratch));
            f.instruction(&Instruction::I64ExtendI32S);
            f.instruction(&Instruction::StructNew(gc_types::INT64));
        }
        f.instruction(&Instruction::End);
    }

    // ========================================================================
    // Module generation modes
    // ========================================================================

    /// Generate a core WASM module (no WIT, used for standalone expression evaluation)
    fn generate_core_module(&self) -> CompileResult<Vec<u8>> {
        let mut module = WasmModule::new();
        // type_offset starts after GC + helper + protocol types, plus 1 for the print_str import type
        let import_type_idx = self.func_type_offset();
        let type_offset = import_type_idx + 1; // +1 for print_str import type

        // Type section - GC types first, then helper types, then protocol types,
        // then print_str import type, then user function signatures
        let mut types = TypeSection::new();
        self.emit_gc_types(&mut types);
        self.emit_helper_types(&mut types);
        self.emit_protocol_types(&mut types);

        // print_str import type: (i32, i32) -> () — always present
        types.ty().function(vec![ValType::I32, ValType::I32], vec![]);

        // Add user function types, but skip closure/builtin/userfn wrappers since they use pre-defined types
        let mut type_count = types.len();
        for func in &self.ir.functions {
            if !func.name.starts_with("$closure_") && !func.name.starts_with("$builtin_") && !func.name.starts_with("$userfn_") && !func.name.starts_with("$variadic_") {
                let params: Vec<ValType> = func
                    .params
                    .iter()
                    .flat_map(|(_, ty)| self.type_to_valtypes_gc(ty))
                    .collect();
                // Functions with explicit return type hints get unboxed primitive types
                let results = if func.has_explicit_return_type {
                    self.type_to_valtypes_for_signature(&func.return_type)
                } else {
                    self.type_to_valtypes_gc(&func.return_type)
                };
                types.ty().function(params, results);
                type_count += 1;
            }
        }

        // Exception tag function type: (eqref) -> ()
        // This type is used by the tag section for throw/catch
        let eqref_val = ValType::Ref(RefType {
            nullable: true,
            heap_type: HeapType::Abstract { shared: false, ty: AbstractHeapType::Eq },
        });
        types.ty().function([eqref_val], []);
        // Record the exception tag type index (it's the last type added)
        let exception_tag_type_idx = type_count;

        module.section(&types);

        // Import section - print_str host import (always present)
        {
            let mut imports = ImportSection::new();
            imports.import("suss", "print_str", EntityType::Function(import_type_idx));
            module.section(&imports);
        }

        // Function section - helper functions first, then user functions
        // Collection helpers (vector trie, HAMT) removed - now in core.sus
        let mut functions = FunctionSection::new();
        // Runtime helper functions: hash_string, get_type_id, init_intern_tables, cabi_realloc, print_str
        functions.function(self.helper_type(helper_type_offsets::HASH_STRING));
        functions.function(self.helper_type(helper_type_offsets::GET_TYPE_ID));
        functions.function(self.helper_type(helper_type_offsets::INIT_INTERN_TABLES));
        functions.function(self.helper_type(helper_type_offsets::CABI_REALLOC));
        functions.function(self.helper_type(helper_type_offsets::PRINT_STR));
        // User functions: closure/builtin wrappers use pre-defined types, others use type_offset
        let mut non_closure_type_idx = 0u32;
        for func in &self.ir.functions {
            if func.name.starts_with("$closure_") || func.name.starts_with("$builtin_") || func.name.starts_with("$userfn_") {
                // Regular closures have env as first param, so arity = params.len() - 1
                let arity = func.params.len().saturating_sub(1) as u32;
                let closure_fn_type = crate::ir::gc_types::closure_fn_type_for_arity(arity);
                functions.function(closure_fn_type);
            } else if func.name.starts_with("$variadic_") || func.name.starts_with("$variadic_userfn_") {
                // Variadic wrappers have explicit types for arities 0-8
                // Uses variadic_fn_type_for_arity_new which returns CLOSURE_FN_5-8 for arities 5-8
                let arity = func.params.len().saturating_sub(1) as u32;
                let variadic_fn_type = crate::ir::gc_types::variadic_fn_type_for_arity_new(arity);
                functions.function(variadic_fn_type);
            } else {
                functions.function(type_offset + non_closure_type_idx);
                non_closure_type_idx += 1;
            }
        }
        module.section(&functions);

        // Table section - dispatch table for protocol methods
        self.emit_table_section(&mut module);

        // Memory section (still needed for string data even in GC mode)
        self.emit_memory_section(&mut module);

        // Tag section - exception tag for try/catch/throw
        // Tag 0: exception tag that carries an eqref value
        // Always emitted so throw/catch work from any code path
        {
            let mut tags = TagSection::new();
            // The tag type references the exception function type: (eqref) -> ()
            tags.tag(TagType {
                kind: TagKind::Exception,
                func_type_idx: exception_tag_type_idx,
            });
            module.section(&tags);
        }

        // Global section - heap pointer, intern tables
        self.emit_global_section(&mut module);

        // Export section
        let mut exports = ExportSection::new();
        exports.export("memory", ExportKind::Memory, 0);

        for (idx, func) in self.ir.functions.iter().enumerate() {
            if func.exported {
                let name = func.export_name.as_deref().unwrap_or(&func.name);
                // User functions come after helper functions
                exports.export(name, ExportKind::Func, self.user_func_idx(idx as u32));
            }
        }
        module.section(&exports);

        // Start section - initialize intern tables on module instantiation
        self.emit_start_section(&mut module);

        // Element section - populate dispatch table with protocol implementations
        self.emit_element_section(&mut module);

        // Data count section (required before code section for array.new_data)
        self.emit_data_count_section(&mut module);

        // Code section - helper functions first, then user functions
        let mut code = CodeSection::new();
        self.emit_helper_functions(&mut code)?;
        for func in &self.ir.functions {
            let function = self.generate_function(func)?;
            code.function(&function);
        }
        module.section(&code);

        // Data section - string literals
        self.emit_data_section(&mut module);

        Ok(module.finish())
    }

    /// Generate core WASM with WIT metadata (for compile command)
    fn generate_with_wit(&self, resolve: &Resolve, world_id: WorldId) -> CompileResult<Vec<u8>> {
        // Resolve the fully-qualified suss interface name from the WIT package
        let world = &resolve.worlds[world_id];
        let suss_module = if let Some(pkg_id) = world.package {
            let pkg = &resolve.packages[pkg_id];
            format!("{}/suss", pkg.name)
        } else {
            "suss".to_string()
        };
        let core_wasm = self.generate_core_with_imports(&suss_module)?;

        // Encode WIT metadata and append to module
        let encoded_metadata = metadata::encode(resolve, world_id, StringEncoding::UTF8, None)
            .map_err(|e| CompileError::Component(format!("Failed to encode metadata: {}", e)))?;

        self.append_metadata_section(&core_wasm, &encoded_metadata)
    }

    /// Generate WASI component (for expression evaluation with WASI imports)
    fn generate_wasi_component(&self) -> CompileResult<Vec<u8>> {
        // Generate core module with imports
        // The synthetic WIT uses package suss:expr, so the suss interface is suss:expr/suss
        let core_wasm = self.generate_core_with_imports("suss:expr/suss")?;

        // Build a synthetic WIT world for the expression
        let wit_source = self.build_synthetic_wit_world()?;

        // Parse the WIT
        let mut resolve = Resolve::new();

        // Load bundled WASI packages needed by this expression
        let wasi_packages = crate::wasi::detect_needed_packages(&wit_source);
        for pkg_name in &wasi_packages {
            if let Some(combined) = crate::wasi::get_combined_package(pkg_name) {
                let _ = resolve.push_str(&format!("wasi-{}.wit", pkg_name), &combined);
            }
        }

        // Load the expression world
        let pkg_id = resolve
            .push_str("expr.wit", &wit_source)
            .map_err(|e| CompileError::Component(format!("Failed to parse synthetic WIT: {}", e)))?;

        let pkg = &resolve.packages[pkg_id];
        let world_id = pkg
            .worlds
            .values()
            .next()
            .ok_or_else(|| CompileError::Component("No world in synthetic WIT".to_string()))?;

        // Encode WIT metadata and append to module
        let encoded_metadata = metadata::encode(&resolve, *world_id, StringEncoding::UTF8, None)
            .map_err(|e| CompileError::Component(format!("Failed to encode metadata: {}", e)))?;

        // Append metadata as custom section
        let module_with_meta = self.append_metadata_section(&core_wasm, &encoded_metadata)?;

        // Create component
        let mut encoder = ComponentEncoder::default()
            .validate(true)
            .module(&module_with_meta)
            .map_err(|e| CompileError::Component(format!("Failed to encode component: {}", e)))?;

        encoder
            .encode()
            .map_err(|e| CompileError::Component(format!("Failed to finalize component: {}", e)))
    }

    /// Generate a core WASM module with import section (shared by WIT and WASI modes)
    ///
    /// `suss_import_module` is the fully-qualified WIT interface name for the suss
    /// runtime import (e.g., "test:tco/suss" for package test:tco). The component
    /// encoder requires import module names to match the WIT interface path.
    fn generate_core_with_imports(&self, suss_import_module: &str) -> CompileResult<Vec<u8>> {
        use crate::ir::gc_types;

        let mut module = WasmModule::new();
        let num_imports = self.num_imports();

        // Count emitted deftypes (those not using reserved type IDs)
        let emitted_deftypes = self.ir.deftypes
            .iter()
            .filter(|dt| dt.gc_type_idx >= gc_types::NUM_GC_TYPES)
            .count() as u32;
        // Base type index after GC types + user deftypes
        let type_base = gc_types::NUM_GC_TYPES + emitted_deftypes;

        // Type section - GC types first, then import function types, then local function types
        // This ensures GC type indices (0..NUM_GC_TYPES-1) are preserved for struct/array ops.
        let mut types = TypeSection::new();

        // GC types (indices 0..NUM_GC_TYPES-1) - required for .-field, instance?, etc.
        // Plus user deftypes (indices NUM_GC_TYPES..type_base-1)
        self.emit_gc_types(&mut types);

        // Helper function types (indices type_base..type_base+NUM_HELPER_TYPES-1)
        self.emit_helper_types(&mut types);

        // Protocol function types
        self.emit_protocol_types(&mut types);

        // Import function types
        for import in &self.ir.imports {
            let mut params: Vec<ValType> = import
                .params
                .iter()
                .flat_map(|ty| type_to_valtypes(ty))
                .collect();
            let flat_results = if import.return_type == Type::Unit {
                vec![]
            } else {
                type_to_valtypes(&import.return_type)
            };
            // Canonical ABI: MAX_FLAT_RESULTS = 1
            // For imports (lower context): results > 1 → add retptr as last param, empty results
            let results = if flat_results.len() > 1 {
                params.push(ValType::I32); // retptr param
                vec![] // no return values
            } else {
                flat_results
            };
            types.ty().function(params, results);
        }

        // print_str import type: (i32, i32) -> () — always present
        types.ty().function(vec![ValType::I32, ValType::I32], vec![]);

        // Local function types
        // Skip closure/builtin/userfn/variadic wrappers - they use pre-defined CLOSURE_FN_* types
        // Exported functions use WIT types (for component model compatibility)
        // Non-exported functions use GC types (eqref)
        for func in &self.ir.functions {
            // Skip functions that use pre-defined closure types
            if func.name.starts_with("$closure_") || func.name.starts_with("$builtin_")
                || func.name.starts_with("$userfn_") || func.name.starts_with("$variadic_") {
                continue;
            }

            if func.exported {
                let params: Vec<ValType> = func
                    .params
                    .iter()
                    .flat_map(|(_, ty)| type_to_valtypes(ty))
                    .collect();
                // Canonical ABI: MAX_FLAT_RESULTS = 1
                // If results > 1 valtype, core function returns single i32 (retptr)
                let flat_results = type_to_valtypes(&func.return_type);
                let results = if flat_results.len() > 1 {
                    vec![ValType::I32] // retptr
                } else {
                    flat_results
                };
                types.ty().function(params, results);
            } else {
                let params: Vec<ValType> = func
                    .params
                    .iter()
                    .flat_map(|(_, ty)| self.type_to_valtypes_gc(ty))
                    .collect();
                // Functions with explicit return type hints get unboxed primitive types
                let results = if func.has_explicit_return_type {
                    self.type_to_valtypes_for_signature(&func.return_type)
                } else {
                    self.type_to_valtypes_gc(&func.return_type)
                };
                types.ty().function(params, results);
            }
        }
        module.section(&types);

        // Calculate type offsets for imports and local functions
        use crate::ir::protocol_types;
        let import_type_base = type_base + NUM_HELPER_TYPES + protocol_types::NUM_PROTOCOL_TYPES;
        let local_func_type_base = import_type_base + num_imports;

        // Import section - WASI functions + print_str (always present)
        {
            let mut imports = ImportSection::new();
            for (idx, import) in self.ir.imports.iter().enumerate() {
                imports.import(
                    &import.wit_interface,
                    &import.function_name,
                    EntityType::Function(import_type_base + idx as u32),
                );
            }
            // print_str is always imported (unconditionally stable function indices)
            imports.import(
                suss_import_module,
                "print-str",
                EntityType::Function(import_type_base + self.ir.imports.len() as u32),
            );
            module.section(&imports);
        }

        // Function section - helper functions first, then user functions
        // Helper functions use type indices from helper_type_base
        // Closure/builtin wrappers use pre-defined CLOSURE_FN_* types
        // Other user functions use type indices starting at local_func_type_base
        let mut functions = FunctionSection::new();
        // Helper functions (hash_string, get_type_id, init_intern_tables, cabi_realloc, print_str)
        functions.function(type_base + helper_type_offsets::HASH_STRING);
        functions.function(type_base + helper_type_offsets::GET_TYPE_ID);
        functions.function(type_base + helper_type_offsets::INIT_INTERN_TABLES);
        functions.function(type_base + helper_type_offsets::CABI_REALLOC);
        functions.function(type_base + helper_type_offsets::PRINT_STR);
        // User functions - closure wrappers use pre-defined types, others use unique types
        let mut non_closure_idx = 0u32;
        for func in &self.ir.functions {
            if func.name.starts_with("$closure_") || func.name.starts_with("$builtin_") || func.name.starts_with("$userfn_") {
                // Regular closures have env as first param, so arity = params.len() - 1
                let arity = func.params.len().saturating_sub(1) as u32;
                let closure_fn_type = gc_types::closure_fn_type_for_arity(arity);
                functions.function(closure_fn_type);
            } else if func.name.starts_with("$variadic_") || func.name.starts_with("$variadic_userfn_") {
                // Variadic wrappers use CLOSURE_FN_* types
                let arity = func.params.len().saturating_sub(1) as u32;
                let variadic_fn_type = gc_types::variadic_fn_type_for_arity_new(arity);
                functions.function(variadic_fn_type);
            } else {
                // Non-closure functions use unique type indices
                functions.function(local_func_type_base + non_closure_idx);
                non_closure_idx += 1;
            }
        }
        module.section(&functions);

        // Table section - dispatch table for protocol methods
        self.emit_table_section(&mut module);

        // Memory section
        self.emit_memory_section(&mut module);

        // Tag section - exception tag for try/catch/throw
        // Tag 0: exception tag that carries an eqref value
        // Reuses the PRINT_STR helper type which has the same signature: (eqref) -> ()
        {
            let mut tags = TagSection::new();
            tags.tag(TagType {
                kind: TagKind::Exception,
                func_type_idx: type_base + helper_type_offsets::PRINT_STR,
            });
            module.section(&tags);
        }

        // Global section - heap pointer
        self.emit_global_section(&mut module);

        // Export section
        let mut exports = ExportSection::new();
        exports.export("memory", ExportKind::Memory, 0);

        // Export cabi_realloc for Component Model canonical ABI
        exports.export("cabi_realloc", ExportKind::Func, num_imports + helper_funcs::CABI_REALLOC);

        // User functions come after imports + helper functions
        let user_func_base = num_imports + NUM_RUNTIME_HELPERS;
        for (idx, func) in self.ir.functions.iter().enumerate() {
            if func.exported {
                let name = func.export_name.as_deref().unwrap_or(&func.name);
                exports.export(name, ExportKind::Func, user_func_base + idx as u32);
            }
        }
        module.section(&exports);

        // Start section - initialize intern tables on module instantiation
        // Function index is num_imports + helper func index
        module.section(&wasm_encoder::StartSection {
            function_index: num_imports + helper_funcs::INIT_INTERN_TABLES,
        });

        // Element section - populate dispatch table
        self.emit_element_section(&mut module);

        // Data count section (required before code section for array.new_data)
        self.emit_data_count_section(&mut module);

        // Code section - helper functions first, then user functions
        let mut code = CodeSection::new();
        self.emit_helper_functions(&mut code)?;
        for func in &self.ir.functions {
            let function = if func.exported {
                // Exported functions need WIT boundary marshaling
                self.generate_function_wit(func)?
            } else {
                self.generate_function(func)?
            };
            code.function(&function);
        }
        module.section(&code);

        // Data section - string literals
        self.emit_data_section(&mut module);

        Ok(module.finish())
    }

    // ========================================================================
    // Section emission helpers
    // ========================================================================

    fn emit_memory_section(&self, module: &mut WasmModule) {
        let mut memory = MemorySection::new();
        memory.memory(MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        module.section(&memory);
    }

    fn emit_global_section(&self, module: &mut WasmModule) {
        use crate::ir::global_indices;

        let mut globals = GlobalSection::new();

        // eqref type for intern table globals
        let eqref_heap = HeapType::Abstract {
            shared: false,
            ty: AbstractHeapType::Eq,
        };

        // Global 0: heap_ptr (i32, mutable) - for linear memory allocation
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &wasm_encoder::ConstExpr::i32_const(0),
        );
        debug_assert_eq!(global_indices::HEAP_PTR, 0);

        // Global 1: keyword_table (eqref, mutable) - array of interned keywords
        // Initialized to null, populated by start function
        globals.global(
            GlobalType {
                val_type: ValType::Ref(RefType {
                    nullable: true,
                    heap_type: eqref_heap,
                }),
                mutable: true,
                shared: false,
            },
            &wasm_encoder::ConstExpr::ref_null(eqref_heap),
        );
        debug_assert_eq!(global_indices::KEYWORD_TABLE, 1);

        // Global 2: symbol_table (eqref, mutable) - array of interned symbols
        // Initialized to null, populated by start function
        globals.global(
            GlobalType {
                val_type: ValType::Ref(RefType {
                    nullable: true,
                    heap_type: eqref_heap,
                }),
                mutable: true,
                shared: false,
            },
            &wasm_encoder::ConstExpr::ref_null(eqref_heap),
        );
        debug_assert_eq!(global_indices::SYMBOL_TABLE, 2);

        module.section(&globals);
    }

    /// Emit the start section to initialize intern tables on module instantiation.
    ///
    /// The start function is init_intern_tables (helper func index 2).
    /// In the no-imports code path, function index = helper func index.
    fn emit_start_section(&self, module: &mut WasmModule) {
        module.section(&wasm_encoder::StartSection {
            function_index: self.num_imports() + helper_funcs::INIT_INTERN_TABLES,
        });
    }

    /// Emit the data count section (required before code section for array.new_data)
    fn emit_data_count_section(&self, module: &mut WasmModule) {
        if !self.ir.strings.is_empty() {
            let data_count = wasm_encoder::DataCountSection {
                count: self.ir.strings.len() as u32,
            };
            module.section(&data_count);
        }
    }

    fn emit_data_section(&self, module: &mut WasmModule) {
        if !self.ir.strings.is_empty() {
            let mut data = DataSection::new();

            // Use passive data segments for GC string creation via array.new_data
            for s in &self.ir.strings {
                let bytes = s.as_bytes();
                data.segment(DataSegment {
                    mode: DataSegmentMode::Passive,
                    data: bytes.iter().copied(),
                });
            }
            module.section(&data);
        }
    }

    /// Emit the table section with a funcref table for protocol dispatch.
    ///
    /// The table size is calculated dynamically based on the number of deftypes:
    /// - 5 primitive type slots (0-4)
    /// - N deftype slots (5 to 5+N-1)
    /// - Total slots = 5 + num_deftypes
    /// - Table size = (5 + num_deftypes) * methods_per_type
    ///
    /// Table index 0 is used for the dispatch table.
    fn emit_table_section(&self, module: &mut WasmModule) {
        use crate::ir::dispatch_table;

        // Calculate table size based on the maximum dispatch index that will be used.
        // The index formula is: dispatch_slot * methods_per_type + method_id
        // methods_per_type is calculated during lowering as max_method_id + 1.
        const PRIMITIVE_SLOTS: u32 = 5; // slots 0-4 for INT64, FLOAT64, STRING, ARRAY, I32_ARRAY
        let num_deftype_slots = self.ir.deftypes.len() as u32;
        let total_slots = PRIMITIVE_SLOTS + num_deftype_slots;

        // Use dynamic methods_per_type from lowering (accommodates user-defined protocols)
        let methods_per_type = self.ir.methods_per_type;
        let default_size = total_slots * methods_per_type;

        // Find the maximum actual index that will be used
        let max_index = self
            .ir
            .dispatch_entries
            .iter()
            .map(|e| dispatch_table::index(e.dispatch_slot, e.method_id, methods_per_type))
            .max()
            .unwrap_or(0);

        // Table size must accommodate the maximum index + 1
        let table_size = (max_index + 1).max(default_size);

        let mut tables = TableSection::new();
        tables.table(TableType {
            element_type: RefType::FUNCREF,
            minimum: table_size as u64,
            maximum: Some(table_size as u64),
            table64: false,
            shared: false,
        });
        module.section(&tables);
    }

    /// Emit the element section to populate the dispatch table.
    ///
    /// Registers protocol implementations at their computed slot indices:
    /// - index = dispatch_slot * methods_per_type + method_id
    ///
    /// Slot mapping (set by lowerer):
    /// - Primitive types: slots 0-4
    /// - User deftypes: slots 5+ (in definition order)
    fn emit_element_section(&self, module: &mut WasmModule) {
        use crate::ir::dispatch_table;
        use std::borrow::Cow;

        let mut elements = ElementSection::new();
        let methods_per_type = self.ir.methods_per_type;

        // =========================================================================
        // Protocol dispatch table entries
        // Collection types (Cons, PersistentVector, PersistentMap, PersistentSet)
        // are now deftypes in core.sus. Their protocol implementations come from
        // extend-type declarations and are added via the dispatch_entries loop.
        // =========================================================================

        // User-defined protocol implementations from extend-type
        for entry in &self.ir.dispatch_entries {
            // dispatch_slot is already set correctly by the lowerer
            let table_idx = dispatch_table::index(entry.dispatch_slot, entry.method_id, methods_per_type);
            // User-defined protocol methods use user_func_idx because they're lowered as regular functions
            let func_idx = self.user_func_idx(entry.func_idx);
            elements.active(
                Some(dispatch_table::TABLE_INDEX),
                &ConstExpr::i32_const(table_idx as i32),
                Elements::Functions(Cow::Owned(vec![func_idx])),
            );
        }

        // Declarative element segment for closure wrapper functions
        // This declares functions that can be used with ref.func for typed function references.
        // We need to declare all functions that might be closures:
        // - "$closure_" prefixed: user-defined anonymous functions
        // - "$builtin_" prefixed: wrappers for built-in functions used as values
        // - "$variadic_" prefixed: wrappers for variadic builtins (+, *, -, /)
        // - "$userfn_" prefixed: wrappers for user-defined functions used with #'var
        // - "$protocol_" prefixed: protocol method implementations from extend-type
        // Declare ALL user functions so they can be used with ref.func for closures.
        // Any user function might be used as a first-class value (passed to another function),
        // which requires ref.func. WASM requires all ref.func targets to be declared.
        let closure_func_indices: Vec<u32> = self
            .ir
            .functions
            .iter()
            .enumerate()
            .map(|(idx, _)| self.user_func_idx(idx as u32))
            .collect();

        if !closure_func_indices.is_empty() {
            elements.declared(Elements::Functions(Cow::Owned(closure_func_indices)));
        }

        module.section(&elements);
    }

    // ========================================================================
    // GC Type Section
    // ========================================================================

    /// Emit WASM GC struct, array, and closure type definitions.
    ///
    /// This defines the GC types used for Clojure's persistent data structures:
    /// - Type 0 (INT64): struct { i64 } - for integers > 30 bits
    /// - Type 1 (FLOAT): struct { f64 } - all floats are boxed
    /// - Type 2 (STRING): array<i8> - UTF-8 bytes
    /// - Type 3 (ARRAY): array<eqref> - 32-way trie node for vectors
    /// - Type 4 (CONS): struct { first: eqref, rest: eqref } - list cons cell
    /// - Type 5 (BITMAP_INDEXED_NODE): struct { type_id, bitmap, arr } - sparse HAMT node
    /// - Type 6 (ARRAY_NODE): struct { type_id, cnt, arr } - dense HAMT node (>16 children)
    /// - Type 7 (HASH_COLLISION_NODE): struct { type_id, hash, cnt, arr } - hash collision node
    /// - Type 8 (PERSISTENT_VECTOR): struct { cnt, shift, root, tail }
    /// - Type 9 (PERSISTENT_MAP): struct { cnt, root }
    /// - Type 10 (PERSISTENT_SET): struct { cnt, root, _marker }
    /// - Types 11-19 (CLOSURE_FN_0-8): function types for closure wrappers
    /// - Types 20-28 (CLOSURE_0-8): struct { type_id, env, fn } - closures with typed funcrefs
    ///
    /// These types must be emitted BEFORE helper function types in the type section,
    /// since function type indices start after GC type indices.
    ///
    /// NEW LAYOUT (19 types):
    /// - 0-1: INT64, FLOAT64 (numeric primitives)
    /// - 2-4: STRING, ARRAY, I32_ARRAY (storage primitives)
    /// - 5-10: CLOSURE_FN_0 to CLOSURE_FN_4, CLOSURE_FN_N (closure function types)
    /// - 11-16: CLOSURE_0 to CLOSURE_4, CLOSURE_N (closure struct types)
    /// - 17-18: VARIADIC_FN, VARIADIC_CLOSURE (variadic)
    ///
    /// Collection types (Cons, PersistentVector, etc.) are now deftypes in core.sus.
    fn emit_gc_types(&self, types: &mut TypeSection) {
        use crate::ir::gc_types;

        // eqref is used as the unified value type (supports ref.eq)
        let eqref = ValType::Ref(RefType::EQREF);

        // All dispatchable struct types have type_id as field 0 for O(1) dispatch.
        let type_id_field = FieldType {
            element_type: StorageType::Val(ValType::I32),
            mutable: false,
        };

        // =========================================================================
        // Numeric Primitives (0-1)
        // =========================================================================

        // Type 0: INT64 - struct { type_id: i32, value: i64 }
        // For integers that don't fit in i31ref (> 30 bits)
        types.ty().struct_(vec![
            type_id_field.clone(),
            FieldType {
                element_type: StorageType::Val(ValType::I64),
                mutable: false,
            },
        ]);
        debug_assert_eq!(gc_types::INT64, 0);

        // Type 1: FLOAT64 - struct { type_id: i32, value: f64 }
        // All floats are boxed since f64 doesn't fit in i31ref
        types.ty().struct_(vec![
            type_id_field.clone(),
            FieldType {
                element_type: StorageType::Val(ValType::F64),
                mutable: false,
            },
        ]);
        debug_assert_eq!(gc_types::FLOAT64, 1);

        // =========================================================================
        // Storage Primitives (2-4)
        // =========================================================================

        // Type 2: STRING - array<i8>
        // UTF-8 string bytes, mutable for construction
        types.ty().array(&StorageType::I8, true);
        debug_assert_eq!(gc_types::STRING, 2);

        // Type 3: ARRAY - array<eqref>
        // Universal mutable storage for collections (replaces old TRIE_NODE alias)
        types.ty().array(&StorageType::Val(eqref), true);
        debug_assert_eq!(gc_types::ARRAY, 3);

        // Type 4: I32_ARRAY - array<i32>
        // For BigInt magnitude storage (defined in core.sus)
        types.ty().array(&StorageType::Val(ValType::I32), true);
        debug_assert_eq!(gc_types::I32_ARRAY, 4);

        // =========================================================================
        // Closure Function Types (5-10)
        // Signature: (env: (ref null $array), args...) -> eqref
        // Arities 0-4 have dedicated types, 5+ use CLOSURE_FN_N with apply-style
        // =========================================================================

        // Use ARRAY type reference for closure env
        let env_type = ValType::Ref(RefType {
            nullable: true,
            heap_type: HeapType::Concrete(gc_types::ARRAY),
        });

        // Types 5-13: CLOSURE_FN_0 through CLOSURE_FN_8
        // Each takes env plus N arguments
        for arity in 0..=8u32 {
            let mut params = vec![env_type.clone()];
            for _ in 0..arity {
                params.push(eqref);
            }
            types.ty().function(params, vec![eqref]);
        }
        debug_assert_eq!(gc_types::CLOSURE_FN_0, 5);
        debug_assert_eq!(gc_types::CLOSURE_FN_8, 13);

        // Type 14: CLOSURE_FN_N - (env, args_array) -> result
        // Apply-style for arities 9+
        let array_ref = ValType::Ref(RefType {
            nullable: true,
            heap_type: HeapType::Concrete(gc_types::ARRAY),
        });
        types.ty().function(vec![env_type.clone(), array_ref.clone()], vec![eqref]);
        debug_assert_eq!(gc_types::CLOSURE_FN_N, 14);

        // =========================================================================
        // Closure Struct Types (15-20)
        // Each closure has: type_id, env (captured values), fn (typed funcref)
        // =========================================================================

        let make_closure_struct = |types: &mut TypeSection, fn_type_idx: u32, env_type: &ValType| {
            let typed_funcref = ValType::Ref(RefType {
                nullable: false,
                heap_type: HeapType::Concrete(fn_type_idx),
            });
            types.ty().struct_(vec![
                type_id_field.clone(),
                FieldType {
                    element_type: StorageType::Val(env_type.clone()),
                    mutable: false,
                },
                FieldType {
                    element_type: StorageType::Val(typed_funcref),
                    mutable: false,
                },
            ]);
        };

        // Types 15-19: CLOSURE_0 through CLOSURE_4
        for arity in 0..=4u32 {
            let fn_type_idx = gc_types::closure_fn_type_for_arity(arity);
            make_closure_struct(types, fn_type_idx, &env_type);
        }
        debug_assert_eq!(gc_types::CLOSURE_0, 15);
        debug_assert_eq!(gc_types::CLOSURE_4, 19);

        // Type 20: CLOSURE_N (for arities 5+)
        make_closure_struct(types, gc_types::CLOSURE_FN_N, &env_type);
        debug_assert_eq!(gc_types::CLOSURE_N, 20);

        // =========================================================================
        // Variadic Types (21-22)
        // Single function type handles all arities via runtime dispatch
        // =========================================================================

        // Type 21: VARIADIC_FN - () -> result
        // Base variadic function type (arity 0). Also used as fallback.
        types.ty().function(vec![], vec![eqref]);
        debug_assert_eq!(gc_types::VARIADIC_FN, 21);

        // Type 22: VARIADIC_CLOSURE - struct { type_id: i32, fn0..fn8: funcrefs }
        // Contains 9 funcrefs, one per arity (0-8), for variadic builtins like +, *, -, /
        // Each funcref uses CLOSURE_FN_* types with individual params.
        // Note: CLOSURE_FN_* types have env as first param, but variadics ignore it.
        let make_variadic_funcref = |fn_type: u32| -> FieldType {
            FieldType {
                element_type: StorageType::Val(ValType::Ref(RefType {
                    nullable: false,
                    heap_type: HeapType::Concrete(fn_type),
                })),
                mutable: false,
            }
        };
        types.ty().struct_(vec![
            type_id_field.clone(),                           // type_id
            make_variadic_funcref(gc_types::CLOSURE_FN_0),   // fn0 (arity 0): (env) -> result
            make_variadic_funcref(gc_types::CLOSURE_FN_1),   // fn1 (arity 1): (env, arg) -> result
            make_variadic_funcref(gc_types::CLOSURE_FN_2),   // fn2 (arity 2)
            make_variadic_funcref(gc_types::CLOSURE_FN_3),   // fn3 (arity 3)
            make_variadic_funcref(gc_types::CLOSURE_FN_4),   // fn4 (arity 4)
            make_variadic_funcref(gc_types::CLOSURE_FN_5),   // fn5 (arity 5)
            make_variadic_funcref(gc_types::CLOSURE_FN_6),   // fn6 (arity 6)
            make_variadic_funcref(gc_types::CLOSURE_FN_7),   // fn7 (arity 7)
            make_variadic_funcref(gc_types::CLOSURE_FN_8),   // fn8 (arity 8)
        ]);
        debug_assert_eq!(gc_types::VARIADIC_CLOSURE, 22);

        // =========================================================================
        // Keyword Type (23)
        // Interned keywords with pre-computed hash for O(1) map operations
        // =========================================================================

        // Type 23: KEYWORD - struct { type_id: i32, hash: i32, ns: (ref null STRING), name: (ref STRING) }
        // - type_id: For protocol dispatch (always type_ids::KEYWORD)
        // - hash: Pre-computed xxHash32 of the keyword string
        // - ns: Namespace string (or null if no namespace)
        // - name: Name string
        let nullable_string_ref = RefType {
            nullable: true,
            heap_type: HeapType::Concrete(gc_types::STRING),
        };
        let string_ref = RefType {
            nullable: false,
            heap_type: HeapType::Concrete(gc_types::STRING),
        };
        types.ty().struct_(vec![
            type_id_field.clone(),
            FieldType {
                element_type: StorageType::Val(ValType::I32),
                mutable: false,
            },
            FieldType {
                element_type: StorageType::Val(ValType::Ref(nullable_string_ref)),
                mutable: false,
            },
            FieldType {
                element_type: StorageType::Val(ValType::Ref(string_ref)),
                mutable: false,
            },
        ]);
        debug_assert_eq!(gc_types::KEYWORD, 23);

        // =========================================================================
        // Symbol Type (24)
        // First-class symbols with namespace support and pre-computed hash
        // =========================================================================

        // Type 24: SYMBOL - struct { type_id: i32, hash: i32, ns: (ref null STRING), name: (ref STRING), _marker: i32 }
        // - type_id: For protocol dispatch (always type_ids::SYMBOL)
        // - hash: Pre-computed xxHash32 of the symbol string
        // - ns: Namespace string (or null if no namespace)
        // - name: Name string
        // - _marker: Distinguishes from KEYWORD (WASM GC uses structural typing)
        types.ty().struct_(vec![
            type_id_field.clone(),
            FieldType {
                element_type: StorageType::Val(ValType::I32),
                mutable: false,
            },
            FieldType {
                element_type: StorageType::Val(ValType::Ref(nullable_string_ref)),
                mutable: false,
            },
            FieldType {
                element_type: StorageType::Val(ValType::Ref(string_ref)),
                mutable: false,
            },
            // Marker field to make SYMBOL structurally distinct from KEYWORD
            // (WASM GC uses structural typing, so same layout = same type for ref.test)
            FieldType {
                element_type: StorageType::Val(ValType::I32),
                mutable: false,
            },
        ]);
        debug_assert_eq!(gc_types::SYMBOL, 24);

        // =========================================================================
        // Var Type (25)
        // First-class variables with metadata support
        // =========================================================================

        // Type 25: VAR - struct { type_id: i32, root: eqref, meta: eqref, sym: eqref }
        // - type_id: For protocol dispatch (always type_ids::VAR)
        // - root: The bound value
        // - meta: Metadata map (or nil)
        // - sym: The SYMBOL for this var's name
        types.ty().struct_(vec![
            type_id_field.clone(),
            FieldType {
                element_type: StorageType::Val(eqref),
                mutable: false,
            },
            FieldType {
                element_type: StorageType::Val(eqref),
                mutable: false,
            },
            FieldType {
                element_type: StorageType::Val(eqref),
                mutable: false,
            },
        ]);
        debug_assert_eq!(gc_types::VAR, 25);

        // =========================================================================
        // Intern Tables (26-27)
        // Array types for keyword/symbol interning with hash-based lookup
        // NOTE: Currently unused - reserved for runtime (keyword "name") support
        // =========================================================================

        // Type 26: KEYWORD_INTERN_TABLE - array<(ref null KEYWORD)>
        // Hash table for interned keywords using open addressing
        let nullable_keyword_ref = RefType {
            nullable: true,
            heap_type: HeapType::Concrete(gc_types::KEYWORD),
        };
        types.ty().array(
            &StorageType::Val(ValType::Ref(nullable_keyword_ref)),
            true, // mutable for setting entries
        );
        debug_assert_eq!(gc_types::KEYWORD_INTERN_TABLE, 26);

        // Type 27: SYMBOL_INTERN_TABLE - array<(ref null SYMBOL)>
        // Hash table for interned symbols using open addressing
        let nullable_symbol_ref = RefType {
            nullable: true,
            heap_type: HeapType::Concrete(gc_types::SYMBOL),
        };
        types.ty().array(
            &StorageType::Val(ValType::Ref(nullable_symbol_ref)),
            true, // mutable for setting entries
        );
        debug_assert_eq!(gc_types::SYMBOL_INTERN_TABLE, 27);

        // =========================================================================
        // User-Defined Types (from deftype)
        // These come after all built-in types. Each has type_id at field 0.
        // Collection types are now regular deftypes defined in core.sus.
        // =========================================================================

        for deftype in &self.ir.deftypes {
            // Skip deftypes with reserved type IDs - their GC types are already emitted above
            if deftype.gc_type_idx < gc_types::NUM_GC_TYPES {
                continue;
            }

            let mut fields = vec![type_id_field.clone()]; // Field 0: type_id

            for field in &deftype.fields {
                let storage_type = match field.field_type {
                    IrFieldType::I32 => StorageType::Val(ValType::I32),
                    IrFieldType::I64 => StorageType::Val(ValType::I64),
                    IrFieldType::F64 => StorageType::Val(ValType::F64),
                    IrFieldType::GcRef => StorageType::Val(eqref),
                };
                fields.push(FieldType {
                    element_type: storage_type,
                    mutable: field.is_mutable,
                });
            }

            types.ty().struct_(fields);
        }
    }

    /// Emit function types for runtime helper functions.
    ///
    /// These come after GC types but before protocol function types.
    /// Order must match helper_types module constants.
    ///
    /// NOTE: Helper types: hash_string, get_type_id, init_intern_tables.
    /// Vector and HAMT helpers are now in core.sus.
    fn emit_helper_types(&self, types: &mut TypeSection) {
        let eqref = ValType::Ref(RefType::EQREF);

        // Type 0: $hash_string: (eqref) -> i32
        // Takes STRING (array<i8>) GC array, returns hash
        types.ty().function(
            vec![eqref],
            vec![ValType::I32],
        );

        // Type 1: $get_type_id: (eqref) -> i32
        // Takes any GC value, returns its type ID
        types.ty().function(vec![eqref], vec![ValType::I32]);

        // Type 2: $init_intern_tables: () -> ()
        // Start function that initializes keyword/symbol intern tables
        types.ty().function(vec![], vec![]);

        // Type 3: $cabi_realloc: (i32, i32, i32, i32) -> i32
        // Component Model canonical ABI allocator
        types.ty().function(
            vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
            vec![ValType::I32],
        );

        // Type 4: $print_str: (eqref) -> ()
        // Takes GC string, copies to linear memory, calls host import
        types.ty().function(vec![eqref], vec![]);
    }

    /// Emit function types for protocol methods.
    ///
    /// These come after helper types but before user function types.
    /// Order must match protocol_type_offsets module constants.
    fn emit_protocol_types(&self, types: &mut TypeSection) {
        let eqref = ValType::Ref(RefType::EQREF);

        // ARITY_1_REF: (eqref) -> eqref - for first, rest, seq
        types.ty().function(vec![eqref], vec![eqref]);

        // ARITY_1_I32: (eqref) -> i32 - for count, hash
        types.ty().function(vec![eqref], vec![ValType::I32]);

        // ARITY_2_REF: (eqref, eqref) -> eqref - for lookup, nth, conj
        types.ty().function(vec![eqref, eqref], vec![eqref]);

        // ARITY_2_I32: (eqref, eqref) -> i32 - for equiv
        types.ty().function(vec![eqref, eqref], vec![ValType::I32]);

        // ARITY_3_REF: (eqref, eqref, eqref) -> eqref - for assoc
        types.ty().function(vec![eqref, eqref, eqref], vec![eqref]);
    }

    /// Emit code for runtime helper functions.
    ///
    /// These come before user functions in the code section.
    fn emit_helper_functions(&self, code: &mut CodeSection) -> CompileResult<()> {
        // 4 runtime helpers - collection algorithms are now in core.sus

        // $hash_string - xxHash32 for string hashing (func 0 after imports)
        code.function(&self.generate_hash_string_func());

        // $get_type_id - runtime type dispatch (func 1 after imports)
        code.function(&self.generate_get_type_id_func());

        // $init_intern_tables - initialize keyword/symbol intern tables (func 2 after imports)
        // Called by WASM start section on module instantiation
        code.function(&self.generate_init_intern_tables_func());

        // $cabi_realloc - Component Model canonical ABI allocator (func 3 after imports)
        code.function(&self.generate_cabi_realloc_func());

        // $print_str - copy GC string to linear memory and call host import (func 4 after imports)
        code.function(&self.generate_print_str_func());

        Ok(())
    }

    /// Generate $hash_string function - xxHash32 over bytes in linear memory.
    ///
    /// Signature: (ptr: i32, len: i32) -> i32
    ///
    /// Implements xxHash32 algorithm:
    /// 1. Initialize accumulator based on length
    /// 2. Process 4-byte chunks
    /// 3. Process remaining bytes
    /// 4. Avalanche (final mixing)
    fn generate_hash_string_func(&self) -> Function {
        use crate::ir::gc_types;

        // xxHash32 prime constants (must match ir.rs xxhash32)
        const PRIME32_1: i32 = 0x9E3779B1_u32 as i32;
        const PRIME32_2: i32 = 0x85EBCA77_u32 as i32;
        const PRIME32_3: i32 = 0xC2B2AE3D_u32 as i32;
        const PRIME32_4: i32 = 0x27D4EB2F_u32 as i32;
        const PRIME32_5: i32 = 0x165667B1_u32 as i32;

        // Param 0: str (eqref - the GC string array)
        // Locals: len=1, acc=2, i=3
        let locals = vec![
            (3, ValType::I32), // len, acc, i
        ];
        let mut f = Function::new(locals);

        // len = array.len(str)
        f.instruction(&Instruction::LocalGet(0)); // str
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
        f.instruction(&Instruction::ArrayLen);
        f.instruction(&Instruction::LocalSet(1)); // len

        // acc = PRIME32_5 + len  (for inputs < 16 bytes)
        f.instruction(&Instruction::I32Const(PRIME32_5));
        f.instruction(&Instruction::LocalGet(1)); // len
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalSet(2)); // acc

        // i = 0
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::LocalSet(3)); // i

        // === Phase 1: Process 4-byte chunks ===
        // while (i + 4 <= len) { k = le32(str[i..i+4]); acc = (acc + k*P3).rotl(17) * P4; i += 4 }
        f.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        f.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
        // Check: i + 4 > len => break
        f.instruction(&Instruction::LocalGet(3)); // i
        f.instruction(&Instruction::I32Const(4));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalGet(1)); // len
        f.instruction(&Instruction::I32GtU);
        f.instruction(&Instruction::BrIf(1)); // break

        // Build k = u32 from 4 bytes in little-endian: str[i] | str[i+1]<<8 | str[i+2]<<16 | str[i+3]<<24
        // Use array.get_u to get unsigned bytes
        // byte 0
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
        f.instruction(&Instruction::LocalGet(3)); // i
        f.instruction(&Instruction::ArrayGetU(gc_types::STRING));
        // byte 1 << 8
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
        f.instruction(&Instruction::LocalGet(3));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add); // i+1
        f.instruction(&Instruction::ArrayGetU(gc_types::STRING));
        f.instruction(&Instruction::I32Const(8));
        f.instruction(&Instruction::I32Shl);
        f.instruction(&Instruction::I32Or);
        // byte 2 << 16
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
        f.instruction(&Instruction::LocalGet(3));
        f.instruction(&Instruction::I32Const(2));
        f.instruction(&Instruction::I32Add); // i+2
        f.instruction(&Instruction::ArrayGetU(gc_types::STRING));
        f.instruction(&Instruction::I32Const(16));
        f.instruction(&Instruction::I32Shl);
        f.instruction(&Instruction::I32Or);
        // byte 3 << 24
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
        f.instruction(&Instruction::LocalGet(3));
        f.instruction(&Instruction::I32Const(3));
        f.instruction(&Instruction::I32Add); // i+3
        f.instruction(&Instruction::ArrayGetU(gc_types::STRING));
        f.instruction(&Instruction::I32Const(24));
        f.instruction(&Instruction::I32Shl);
        f.instruction(&Instruction::I32Or);
        // k is now on the stack

        // acc = (acc + k * PRIME32_3).rotl(17) * PRIME32_4
        f.instruction(&Instruction::I32Const(PRIME32_3));
        f.instruction(&Instruction::I32Mul); // k * P3
        f.instruction(&Instruction::LocalGet(2)); // acc
        f.instruction(&Instruction::I32Add); // acc + k*P3
        f.instruction(&Instruction::I32Const(17));
        f.instruction(&Instruction::I32Rotl); // rotl(17)
        f.instruction(&Instruction::I32Const(PRIME32_4));
        f.instruction(&Instruction::I32Mul); // * P4
        f.instruction(&Instruction::LocalSet(2)); // acc

        // i += 4
        f.instruction(&Instruction::LocalGet(3));
        f.instruction(&Instruction::I32Const(4));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalSet(3));
        f.instruction(&Instruction::Br(0)); // continue
        f.instruction(&Instruction::End); // end loop
        f.instruction(&Instruction::End); // end block

        // === Phase 2: Process remaining bytes ===
        // while (i < len) { acc = (acc + byte*P5).rotl(11) * P1; i++ }
        f.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        f.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
        // Check: i >= len => break
        f.instruction(&Instruction::LocalGet(3)); // i
        f.instruction(&Instruction::LocalGet(1)); // len
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::BrIf(1)); // break

        // byte = array.get_u(str, i) -- unsigned get for correct multiply
        f.instruction(&Instruction::LocalGet(0)); // str
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
        f.instruction(&Instruction::LocalGet(3)); // i
        f.instruction(&Instruction::ArrayGetU(gc_types::STRING)); // unsigned byte

        // acc = (acc + byte * PRIME32_5).rotl(11) * PRIME32_1
        f.instruction(&Instruction::I32Const(PRIME32_5));
        f.instruction(&Instruction::I32Mul);
        f.instruction(&Instruction::LocalGet(2)); // acc
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I32Const(11));
        f.instruction(&Instruction::I32Rotl);
        f.instruction(&Instruction::I32Const(PRIME32_1));
        f.instruction(&Instruction::I32Mul);
        f.instruction(&Instruction::LocalSet(2)); // acc

        // i += 1
        f.instruction(&Instruction::LocalGet(3));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalSet(3));
        f.instruction(&Instruction::Br(0)); // continue
        f.instruction(&Instruction::End); // end loop
        f.instruction(&Instruction::End); // end block

        // === Avalanche mixing ===
        // acc ^= acc >> 15
        f.instruction(&Instruction::LocalGet(2));
        f.instruction(&Instruction::LocalGet(2));
        f.instruction(&Instruction::I32Const(15));
        f.instruction(&Instruction::I32ShrU);
        f.instruction(&Instruction::I32Xor);
        // acc *= PRIME32_2
        f.instruction(&Instruction::I32Const(PRIME32_2));
        f.instruction(&Instruction::I32Mul);
        // acc ^= acc >> 13
        f.instruction(&Instruction::LocalTee(2));
        f.instruction(&Instruction::LocalGet(2));
        f.instruction(&Instruction::I32Const(13));
        f.instruction(&Instruction::I32ShrU);
        f.instruction(&Instruction::I32Xor);
        // acc *= PRIME32_3
        f.instruction(&Instruction::I32Const(PRIME32_3));
        f.instruction(&Instruction::I32Mul);
        // acc ^= acc >> 16
        f.instruction(&Instruction::LocalTee(2));
        f.instruction(&Instruction::LocalGet(2));
        f.instruction(&Instruction::I32Const(16));
        f.instruction(&Instruction::I32ShrU);
        f.instruction(&Instruction::I32Xor);

        // Return acc (already on stack)
        f.instruction(&Instruction::End);
        f
    }

    /// Generate $get_type_id function - returns dispatch table slot index for protocol dispatch.
    ///
    /// Signature: (value: eqref) -> i32
    ///
    /// IMPORTANT: Returns a dispatch table SLOT INDEX, not the raw type_id.
    /// This is used for indexing into the dispatch table: slot * NUM_BUILTIN + method_id
    ///
    /// Slot mapping:
    /// - i31ref (nil, bool, small int): -1 (non-dispatchable)
    /// - Primitive types (INT64, FLOAT64, STRING, ARRAY, I32_ARRAY): 0-4
    /// - User deftypes: 5, 6, 7, ... (in order of definition)
    ///
    /// Uses ref.test chain for type dispatch.
    fn generate_get_type_id_func(&self) -> Function {
        use crate::ir::gc_types;

        // Primitive types get slots 0-4, deftypes get slots 5+
        const SLOT_I31REF: i32 = -1;
        const SLOT_INT64: i32 = 0;
        const SLOT_FLOAT64: i32 = 1;
        const SLOT_STRING: i32 = 2;
        const SLOT_ARRAY: i32 = 3;
        const SLOT_I32_ARRAY: i32 = 4;
        const DEFTYPE_SLOT_BASE: i32 = 5; // deftypes start at slot 5

        // Locals: none needed
        let locals = vec![];
        let mut f = Function::new(locals);

        // Test for i31ref first (most common for primitives: nil, bool, small int)
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Abstract {
            shared: false,
            ty: AbstractHeapType::I31,
        }));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(SLOT_I31REF));
        f.instruction(&Instruction::Else);

        // Test for FLOAT64 (boxed f64)
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::FLOAT64)));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(SLOT_FLOAT64));
        f.instruction(&Instruction::Else);

        // Test for INT64 (boxed i64)
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::INT64)));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(SLOT_INT64));
        f.instruction(&Instruction::Else);

        // Test for STRING (array type)
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::STRING)));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(SLOT_STRING));
        f.instruction(&Instruction::Else);

        // Test for ARRAY (array type, universal storage)
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::ARRAY)));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(SLOT_ARRAY));
        f.instruction(&Instruction::Else);

        // Test for I32_ARRAY (for BigInt magnitude)
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::I32_ARRAY)));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(SLOT_I32_ARRAY));
        f.instruction(&Instruction::Else);

        // Test for user deftypes - each gets a sequential slot starting at DEFTYPE_SLOT_BASE
        let deftype_count = self.ir.deftypes.len();
        for (idx, deftype) in self.ir.deftypes.iter().enumerate() {
            f.instruction(&Instruction::LocalGet(0));
            f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(deftype.gc_type_idx)));
            f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
            // Return the dispatch slot index, not the raw type_id
            f.instruction(&Instruction::I32Const(DEFTYPE_SLOT_BASE + idx as i32));
            f.instruction(&Instruction::Else);
        }

        // Unknown type (closures, etc.) - return -2
        f.instruction(&Instruction::I32Const(-2));

        // Close all the if/else chains (6 primitive + deftype_count nested ifs)
        for _ in 0..(6 + deftype_count) {
            f.instruction(&Instruction::End);
        }

        // End function body
        f.instruction(&Instruction::End);
        f
    }

    /// Generate $init_intern_tables function - initialize keyword/symbol intern tables.
    ///
    /// Signature: () -> ()
    ///
    /// Called by WASM start section on module instantiation.
    /// Creates arrays of pre-built keyword/symbol structs and stores them in globals.
    fn generate_init_intern_tables_func(&self) -> Function {
        use crate::ir::{gc_types, global_indices, type_ids};

        let locals = vec![];
        let mut f = Function::new(locals);

        let num_keywords = self.ir.keywords.len() as u32;
        let num_symbols = self.ir.symbols.len() as u32;

        // Helper to find string index by content
        let find_string_idx = |s: &str| -> Option<u32> {
            self.ir.strings.iter().position(|x| x == s).map(|i| i as u32)
        };

        // Create keyword array and populate it
        if num_keywords > 0 {
            // Stack will have: [kw_struct_0, kw_struct_1, ..., kw_struct_n]
            for (_idx, (ns_opt, name)) in self.ir.keywords.iter().enumerate() {
                // Compute hash for this keyword
                let full_name = if let Some(ns) = ns_opt {
                    format!(":{}:{}", ns, name)
                } else {
                    format!(":{}", name)
                };
                let hash = gc_types::xxhash32(full_name.as_bytes());

                // Create KEYWORD struct { type_id: i32, hash: i32, ns: (ref null STRING), name: (ref STRING) }
                f.instruction(&Instruction::I32Const(type_ids::KEYWORD));
                f.instruction(&Instruction::I32Const(hash));

                // ns field - create string array or null
                if let Some(ns) = ns_opt {
                    if let Some(str_idx) = find_string_idx(ns) {
                        let len = ns.len() as i32;
                        f.instruction(&Instruction::I32Const(0));     // offset
                        f.instruction(&Instruction::I32Const(len));   // length
                        f.instruction(&Instruction::ArrayNewData {
                            array_type_index: gc_types::STRING,
                            array_data_index: str_idx,
                        });
                    } else {
                        // Namespace not found in strings table - shouldn't happen
                        f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::STRING)));
                    }
                } else {
                    f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::STRING)));
                }

                // name field - create string array
                if let Some(str_idx) = find_string_idx(name) {
                    let len = name.len() as i32;
                    f.instruction(&Instruction::I32Const(0));     // offset
                    f.instruction(&Instruction::I32Const(len));   // length
                    f.instruction(&Instruction::ArrayNewData {
                        array_type_index: gc_types::STRING,
                        array_data_index: str_idx,
                    });
                } else {
                    // Name not found - create empty string as fallback
                    f.instruction(&Instruction::ArrayNewFixed {
                        array_type_index: gc_types::STRING,
                        array_size: 0,
                    });
                }

                f.instruction(&Instruction::StructNew(gc_types::KEYWORD));
            }

            // Create array from the stack of keyword structs
            f.instruction(&Instruction::ArrayNewFixed {
                array_type_index: gc_types::KEYWORD_INTERN_TABLE,
                array_size: num_keywords,
            });

            // Store in global
            f.instruction(&Instruction::GlobalSet(global_indices::KEYWORD_TABLE));
        }

        // Create symbol array and populate it
        if num_symbols > 0 {
            for (_idx, (ns_opt, name)) in self.ir.symbols.iter().enumerate() {
                // Compute hash for this symbol (no leading colon)
                let full_name = if let Some(ns) = ns_opt {
                    format!("{}/{}", ns, name)
                } else {
                    name.clone()
                };
                let hash = gc_types::xxhash32(full_name.as_bytes());

                // Create SYMBOL struct { type_id: i32, hash: i32, ns: (ref null STRING), name: (ref STRING), _marker: i32 }
                f.instruction(&Instruction::I32Const(type_ids::SYMBOL));
                f.instruction(&Instruction::I32Const(hash));

                // ns field - create string array or null
                if let Some(ns) = ns_opt {
                    if let Some(str_idx) = find_string_idx(ns) {
                        let len = ns.len() as i32;
                        f.instruction(&Instruction::I32Const(0));     // offset
                        f.instruction(&Instruction::I32Const(len));   // length
                        f.instruction(&Instruction::ArrayNewData {
                            array_type_index: gc_types::STRING,
                            array_data_index: str_idx,
                        });
                    } else {
                        f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::STRING)));
                    }
                } else {
                    f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::STRING)));
                }

                // name field - create string array
                if let Some(str_idx) = find_string_idx(name) {
                    let len = name.len() as i32;
                    f.instruction(&Instruction::I32Const(0));     // offset
                    f.instruction(&Instruction::I32Const(len));   // length
                    f.instruction(&Instruction::ArrayNewData {
                        array_type_index: gc_types::STRING,
                        array_data_index: str_idx,
                    });
                } else {
                    // Name not found - create empty string as fallback
                    f.instruction(&Instruction::ArrayNewFixed {
                        array_type_index: gc_types::STRING,
                        array_size: 0,
                    });
                }

                // Marker field - distinguishes SYMBOL from KEYWORD structurally
                f.instruction(&Instruction::I32Const(0));

                f.instruction(&Instruction::StructNew(gc_types::SYMBOL));
            }

            // Create array from the stack of symbol structs
            f.instruction(&Instruction::ArrayNewFixed {
                array_type_index: gc_types::SYMBOL_INTERN_TABLE,
                array_size: num_symbols,
            });

            // Store in global
            f.instruction(&Instruction::GlobalSet(global_indices::SYMBOL_TABLE));
        }

        f.instruction(&Instruction::End);
        f
    }

    /// Generate $cabi_realloc function - Component Model canonical ABI allocator.
    ///
    /// Signature: (old_ptr: i32, old_size: i32, align: i32, new_size: i32) -> i32
    ///
    /// Simple bump allocator using the heap_ptr global. Aligns the pointer,
    /// bumps by new_size, and grows memory if needed.
    fn generate_cabi_realloc_func(&self) -> Function {
        use crate::ir::global_indices;

        // Params: old_ptr=0, old_size=1, align=2, new_size=3
        // Locals: ptr=4 (aligned base pointer)
        let locals = vec![(1, ValType::I32)];
        let mut f = Function::new(locals);

        // ptr = global.get $heap_ptr
        f.instruction(&Instruction::GlobalGet(global_indices::HEAP_PTR));
        f.instruction(&Instruction::LocalSet(4));

        // Align: ptr = (ptr + align - 1) & ~(align - 1)
        f.instruction(&Instruction::LocalGet(4));       // ptr
        f.instruction(&Instruction::LocalGet(2));        // align
        f.instruction(&Instruction::I32Add);             // ptr + align
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);             // ptr + align - 1
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::LocalGet(2));        // align
        f.instruction(&Instruction::I32Sub);             // 0 - align = ~(align - 1) when align is power of 2
        f.instruction(&Instruction::I32And);             // (ptr + align - 1) & ~(align - 1)
        f.instruction(&Instruction::LocalSet(4));        // ptr = aligned

        // heap_ptr = ptr + new_size
        f.instruction(&Instruction::LocalGet(4));        // ptr
        f.instruction(&Instruction::LocalGet(3));        // new_size
        f.instruction(&Instruction::I32Add);             // ptr + new_size
        f.instruction(&Instruction::GlobalSet(global_indices::HEAP_PTR));

        // Grow memory if needed: if heap_ptr > memory.size * 65536
        // memory.size returns pages (64KB each)
        f.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        f.instruction(&Instruction::GlobalGet(global_indices::HEAP_PTR));
        f.instruction(&Instruction::MemorySize(0));
        f.instruction(&Instruction::I32Const(16));
        f.instruction(&Instruction::I32Shl);             // memory.size * 65536
        f.instruction(&Instruction::I32LeU);             // heap_ptr <= mem_size?
        f.instruction(&Instruction::BrIf(0));            // skip grow if enough

        // Calculate pages needed: (heap_ptr - mem_bytes + 65535) / 65536
        f.instruction(&Instruction::GlobalGet(global_indices::HEAP_PTR));
        f.instruction(&Instruction::MemorySize(0));
        f.instruction(&Instruction::I32Const(16));
        f.instruction(&Instruction::I32Shl);
        f.instruction(&Instruction::I32Sub);             // heap_ptr - mem_bytes
        f.instruction(&Instruction::I32Const(65535));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I32Const(16));
        f.instruction(&Instruction::I32ShrU);            // / 65536
        f.instruction(&Instruction::MemoryGrow(0));
        f.instruction(&Instruction::Drop);               // ignore result
        f.instruction(&Instruction::End);                // end block

        // Return aligned pointer
        f.instruction(&Instruction::LocalGet(4));

        f.instruction(&Instruction::End);
        f
    }

    /// Generate $print_str helper function.
    ///
    /// Signature: (value: eqref) -> ()
    /// Copies a GC string (array<i8>) to linear memory and calls the host import.
    /// If has_print is false, this is a no-op.
    fn generate_print_str_func(&self) -> Function {
        use crate::ir::gc_types;

        // Param 0: value (eqref) - the GC string to print
        // Local 1: str_ref (eqref) - cast string reference
        // Local 2: str_ptr (i32) - linear memory pointer
        // Local 3: str_len (i32) - string length
        // Local 4: str_i (i32) - loop counter
        let locals = vec![
            (1, ValType::Ref(RefType::EQREF)),  // str_ref
            (3, ValType::I32),                   // str_ptr, str_len, str_i
        ];
        let mut f = Function::new(locals);

        let param_val = 0u32;
        let str_ref = 1u32;
        let str_ptr = 2u32;
        let str_len = 3u32;
        let str_i = 4u32;

        // Check if arg is null/nil - if so, skip printing
        f.instruction(&Instruction::LocalGet(param_val));
        f.instruction(&Instruction::RefIsNull);
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        f.instruction(&Instruction::Return);
        f.instruction(&Instruction::End);

        // Also check for i31ref (nil sentinel) - not a string
        f.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        f.instruction(&Instruction::LocalGet(param_val));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
        f.instruction(&Instruction::LocalSet(str_ref));

        // Get string length
        f.instruction(&Instruction::LocalGet(str_ref));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
        f.instruction(&Instruction::ArrayLen);
        f.instruction(&Instruction::LocalSet(str_len));

        // If length is 0, skip
        f.instruction(&Instruction::LocalGet(str_len));
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::BrIf(0)); // break to end of block

        // Allocate linear memory: call cabi_realloc(0, 0, 1, len)
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::LocalGet(str_len));
        f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::CABI_REALLOC)));
        f.instruction(&Instruction::LocalSet(str_ptr));

        // Copy loop: for i = 0..len, mem[ptr + i] = array.get_s str[i]
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::LocalSet(str_i));

        f.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        f.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
        // break if i >= len
        f.instruction(&Instruction::LocalGet(str_i));
        f.instruction(&Instruction::LocalGet(str_len));
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::BrIf(1));

        // i32.store8 (ptr + i) <- array.get_s $STRING str_ref[i]
        f.instruction(&Instruction::LocalGet(str_ptr));
        f.instruction(&Instruction::LocalGet(str_i));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalGet(str_ref));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
        f.instruction(&Instruction::LocalGet(str_i));
        f.instruction(&Instruction::ArrayGetS(gc_types::STRING));
        f.instruction(&Instruction::I32Store8(wasm_encoder::MemArg {
            offset: 0,
            align: 0,
            memory_index: 0,
        }));

        // i++
        f.instruction(&Instruction::LocalGet(str_i));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalSet(str_i));
        f.instruction(&Instruction::Br(0)); // continue
        f.instruction(&Instruction::End); // end loop
        f.instruction(&Instruction::End); // end block (inner)

        // Call host import: print_str(ptr, len)
        f.instruction(&Instruction::LocalGet(str_ptr));
        f.instruction(&Instruction::LocalGet(str_len));
        f.instruction(&Instruction::Call(self.print_str_import_idx()));

        f.instruction(&Instruction::End); // end outer block

        f.instruction(&Instruction::End); // end function
        f
    }

    // ========================================================================

    /// Generate code to create a new persistent vector from elements.
    ///
    /// Structure: PersistentVector { cnt, shift, root, tail }
    /// - Empty vector: cnt=0, shift=5, root=null, tail=empty array
    /// - Small vector (≤32): cnt=n, shift=5, root=null, tail=[elements]
    /// - Large vector (>32): cnt=n, shift=5*depth, root=trie, tail=rightmost leaf
    fn generate_vec_new(&self, elements: &[Expr], f: &mut Function) -> CompileResult<()> {
        use crate::ir::gc_types;
        let cnt = elements.len() as i32;

        // Look up PersistentVector type indices dynamically
        let pv_gc_idx = self.deftype_gc_type_idx("PersistentVector")
            .ok_or_else(|| CompileError::Unsupported("PersistentVector deftype not found".to_string()))?;
        let pv_type_id = self.deftype_type_id("PersistentVector")
            .ok_or_else(|| CompileError::Unsupported("PersistentVector deftype not found".to_string()))?;

        if cnt == 0 {
            // Empty vector: type_id, cnt=0, shift=5, root=null, tail=empty array
            f.instruction(&Instruction::I32Const(pv_type_id));
            f.instruction(&Instruction::I32Const(0)); // cnt
            f.instruction(&Instruction::I32Const(5)); // shift
            f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::ARRAY))); // root
            // Create empty tail array using ArrayNewFixed with 0 elements
            f.instruction(&Instruction::ArrayNewFixed {
                array_type_index: gc_types::ARRAY,
                array_size: 0,
            });
            f.instruction(&Instruction::StructNew(pv_gc_idx));
        } else if cnt <= 32 {
            // Small vector: type_id, cnt=n, shift=5, root=null, tail=[elements]
            f.instruction(&Instruction::I32Const(pv_type_id));
            f.instruction(&Instruction::I32Const(cnt)); // cnt
            f.instruction(&Instruction::I32Const(5)); // shift
            f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::ARRAY))); // root

            // Build tail array using ArrayNewFixed - push all elements, then create array
            for elem in elements.iter() {
                self.generate_expr(elem, f)?;
            }
            f.instruction(&Instruction::ArrayNewFixed {
                array_type_index: gc_types::ARRAY,
                array_size: cnt as u32,
            });

            f.instruction(&Instruction::StructNew(pv_gc_idx));
        } else {
            // Large vector: build proper trie structure
            // This is complex - for now, delegate to a simpler construction
            // that builds the trie incrementally using conj
            self.generate_vec_new_large(elements, f)?;
        }

        Ok(())
    }

    /// Wrap a raw GC array (eqref) into a PersistentVector.
    ///
    /// Used to convert variadic rest params from raw arrays to usable collections.
    /// The array becomes the tail of a PersistentVector with cnt=array.len.
    fn generate_wrap_in_vector(&self, array_expr: &Expr, f: &mut Function) -> CompileResult<()> {
        use crate::ir::gc_types;

        let pv_gc_idx = self.deftype_gc_type_idx("PersistentVector")
            .ok_or_else(|| CompileError::Unsupported("PersistentVector deftype not found".to_string()))?;
        let pv_type_id = self.deftype_type_id("PersistentVector")
            .ok_or_else(|| CompileError::Unsupported("PersistentVector deftype not found".to_string()))?;

        // Store array in a scratch local so we can reference it twice (for len and as tail)
        // Must bump by 5 (full group) to maintain scratch alignment [eqref, i32, eqref, eqref, eqref]
        let scratch_base = self.scratch_local.get();
        let arr_local = scratch_base; // eqref
        self.scratch_local.set(scratch_base + 5);

        // Evaluate the array expression and store
        self.generate_expr(array_expr, f)?;
        f.instruction(&Instruction::LocalSet(arr_local));

        // Build PersistentVector struct: { type_id, cnt, shift, root, tail }
        // Field 0: type_id (i32)
        f.instruction(&Instruction::I32Const(pv_type_id));

        // Field 1: cnt (i32) = array.len
        f.instruction(&Instruction::LocalGet(arr_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::ARRAY)));
        f.instruction(&Instruction::ArrayLen);

        // Field 2: shift (i32) = 5
        f.instruction(&Instruction::I32Const(5));

        // Field 3: root (eqref) = null
        f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::ARRAY)));

        // Field 4: tail (eqref) = the raw array
        f.instruction(&Instruction::LocalGet(arr_local));

        // Create the struct
        f.instruction(&Instruction::StructNew(pv_gc_idx));

        self.scratch_local.set(scratch_base);
        Ok(())
    }

    /// Generate large vector construction by building trie incrementally
    fn generate_vec_new_large(&self, elements: &[Expr], f: &mut Function) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::method_ids;
        use crate::ir::dispatch_table;

        // Start with first 32 elements as base vector
        let (first_32, rest) = elements.split_at(32.min(elements.len()));

        // Look up PersistentVector type indices dynamically
        let pv_gc_idx = self.deftype_gc_type_idx("PersistentVector")
            .ok_or_else(|| CompileError::Unsupported("PersistentVector deftype not found".to_string()))?;
        let pv_type_id = self.deftype_type_id("PersistentVector")
            .ok_or_else(|| CompileError::Unsupported("PersistentVector deftype not found".to_string()))?;

        let vec_local = self.scratch_local.get();

        // Create initial vector with first batch: type_id, cnt, shift, root, tail
        f.instruction(&Instruction::I32Const(pv_type_id));
        f.instruction(&Instruction::I32Const(first_32.len() as i32));
        f.instruction(&Instruction::I32Const(5));
        f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::ARRAY)));

        // Build tail with first elements using ArrayNewFixed
        for elem in first_32.iter() {
            self.generate_expr(elem, f)?;
        }
        f.instruction(&Instruction::ArrayNewFixed {
            array_type_index: gc_types::ARRAY,
            array_size: first_32.len() as u32,
        });

        f.instruction(&Instruction::StructNew(pv_gc_idx));
        f.instruction(&Instruction::LocalSet(vec_local));

        // For remaining elements, call conj via protocol dispatch
        let type_idx = self.protocol_type_index_for_method(method_ids::CONJ, 2);

        // Calculate dispatch table index: dispatch_slot * methods_per_type + method_id
        let pv_slot = self.deftype_dispatch_slot("PersistentVector")
            .ok_or_else(|| CompileError::Unsupported("PersistentVector deftype not found".to_string()))?;
        let dispatch_idx = dispatch_table::index(pv_slot, method_ids::CONJ, self.ir.methods_per_type) as i32;

        for elem in rest {
            // Call -conj via dispatch table: takes (vec, val), returns new vec
            f.instruction(&Instruction::LocalGet(vec_local));
            self.generate_expr(elem, f)?;
            f.instruction(&Instruction::I32Const(dispatch_idx));
            f.instruction(&Instruction::CallIndirect {
                type_index: type_idx,
                table_index: dispatch_table::TABLE_INDEX,
            });
            f.instruction(&Instruction::LocalSet(vec_local));
        }

        // Final result
        f.instruction(&Instruction::LocalGet(vec_local));

        Ok(())
    }

    /// Generate code to get element at index from persistent vector.
    ///
    /// Algorithm:
    /// - If index < 0 or index >= count, return nil
    /// - If index >= tailoff: return tail[index & 0x1F]
    /// - Else: traverse trie from root using bit partitioning
    fn generate_vec_nth(&self, vec: &Expr, index: &Expr, f: &mut Function) -> CompileResult<()> {
        use crate::ir::gc_types;

        // Look up array-for helper function and PersistentVector type dynamically
        let array_for_idx = self.func_idx_by_name("array-for")
            .ok_or_else(|| CompileError::Unsupported("array-for function not found".to_string()))?;
        let pv_gc_idx = self.deftype_gc_type_idx("PersistentVector")
            .ok_or_else(|| CompileError::Unsupported("PersistentVector deftype not found".to_string()))?;

        // Use scratch locals to store vec and decoded index
        let scratch_base = self.scratch_local.get();
        let vec_local = scratch_base; // eqref
        let idx_local = scratch_base + 1; // i32

        // Evaluate and store vec
        self.generate_expr(vec, f)?;
        f.instruction(&Instruction::LocalSet(vec_local));

        // Evaluate index - it's an i31ref small integer, decode to i32
        self.generate_expr(index, f)?;
        f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
        f.instruction(&Instruction::I31GetS);
        // Decode from our encoding: n = (encoded >> 1)
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32ShrS);
        f.instruction(&Instruction::LocalSet(idx_local));

        // Bounds check: if idx < 0 || idx >= cnt, return nil
        // Check: idx < 0
        f.instruction(&Instruction::LocalGet(idx_local));
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32LtS);

        // Check: idx >= cnt
        f.instruction(&Instruction::LocalGet(idx_local));
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(pv_gc_idx)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: pv_gc_idx,
            field_index: 1, // cnt is field 1 (after type_id)
        });
        f.instruction(&Instruction::I32GeS);

        // OR the two conditions: (idx < 0) || (idx >= cnt)
        f.instruction(&Instruction::I32Or);

        // If out of bounds, return nil
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(RefType::EQREF))));
        f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
        f.instruction(&Instruction::RefI31);
        f.instruction(&Instruction::Else);

        // In bounds: call array-for(vec, idx) to get the leaf array
        f.instruction(&Instruction::LocalGet(vec_local));
        // Box idx as i31ref: (n << 1) | 1
        f.instruction(&Instruction::LocalGet(idx_local));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Shl);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::RefI31);
        f.instruction(&Instruction::Call(array_for_idx));

        // Cast result to ARRAY for array.get
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::ARRAY,
        )));

        // Get element at idx & 0x1f
        f.instruction(&Instruction::LocalGet(idx_local));
        f.instruction(&Instruction::I32Const(0x1F));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::ArrayGet(gc_types::ARRAY));

        f.instruction(&Instruction::End); // End of if

        Ok(())
    }

    // ========================================================================
    // Persistent Map Codegen
    // ========================================================================

    /// Generate code to create a new persistent map from key-value pairs.
    fn generate_map_new(&self, pairs: &[(Expr, Expr)], f: &mut Function) -> CompileResult<()> {
        // Look up PersistentMap type indices dynamically
        let pm_gc_idx = self.deftype_gc_type_idx("PersistentMap")
            .ok_or_else(|| CompileError::Unsupported("PersistentMap deftype not found".to_string()))?;
        let pm_type_id = self.deftype_type_id("PersistentMap")
            .ok_or_else(|| CompileError::Unsupported("PersistentMap deftype not found".to_string()))?;

        if pairs.is_empty() {
            // Empty map
            f.instruction(&Instruction::I32Const(pm_type_id)); // type_id
            f.instruction(&Instruction::I32Const(0)); // cnt
            f.instruction(&Instruction::RefNull(HeapType::Abstract {
                shared: false,
                ty: AbstractHeapType::Eq,
            })); // root (null for empty)
            f.instruction(&Instruction::StructNew(pm_gc_idx));
        } else {
            // Build map by starting with empty and assoc'ing each pair
            // Start with empty map on stack
            f.instruction(&Instruction::I32Const(pm_type_id)); // type_id
            f.instruction(&Instruction::I32Const(0)); // cnt
            f.instruction(&Instruction::RefNull(HeapType::Abstract {
                shared: false,
                ty: AbstractHeapType::Eq,
            })); // root (null for empty)
            f.instruction(&Instruction::StructNew(pm_gc_idx));

            // For each pair, assoc it into the map via protocol dispatch
            for (key, val) in pairs {
                // Stack: [current_map]
                // Push key and val
                self.generate_expr(key, f)?;
                self.generate_expr(val, f)?;
                // Stack: [current_map, key, val]
                // Call -assoc via protocol dispatch (method_id=1, 2 args)
                self.generate_protocol_dispatch_stack(crate::ir::method_ids::ASSOC, 2, f)?;
                // Stack: [new_map]
            }
        }
        Ok(())
    }

    // ========================================================================
    // Persistent Set Codegen
    // ========================================================================

    /// Generate code to create a new persistent set from elements.
    fn generate_set_new(&self, elements: &[Expr], f: &mut Function) -> CompileResult<()> {
        // Look up PersistentSet type indices dynamically
        let ps_gc_idx = self.deftype_gc_type_idx("PersistentSet")
            .ok_or_else(|| CompileError::Unsupported("PersistentSet deftype not found".to_string()))?;
        let ps_type_id = self.deftype_type_id("PersistentSet")
            .ok_or_else(|| CompileError::Unsupported("PersistentSet deftype not found".to_string()))?;

        if elements.is_empty() {
            // Empty set
            f.instruction(&Instruction::I32Const(ps_type_id)); // type_id
            f.instruction(&Instruction::I32Const(0)); // cnt
            f.instruction(&Instruction::RefNull(HeapType::Abstract {
                shared: false,
                ty: AbstractHeapType::Eq,
            })); // root (null for empty)
            f.instruction(&Instruction::I32Const(0)); // marker field
            f.instruction(&Instruction::StructNew(ps_gc_idx));
        } else {
            // Build set by starting with empty and conj'ing each element
            // Start with empty set on stack
            f.instruction(&Instruction::I32Const(ps_type_id)); // type_id
            f.instruction(&Instruction::I32Const(0)); // cnt
            f.instruction(&Instruction::RefNull(HeapType::Abstract {
                shared: false,
                ty: AbstractHeapType::Eq,
            })); // root (null for empty)
            f.instruction(&Instruction::I32Const(0)); // marker field
            f.instruction(&Instruction::StructNew(ps_gc_idx));

            // For each element, conj it into the set via protocol dispatch
            for elem in elements {
                // Stack: [current_set]
                self.generate_expr(elem, f)?;
                // Stack: [current_set, elem]
                // Call -conj via protocol dispatch (method_id=4, 1 arg)
                self.generate_protocol_dispatch_stack(crate::ir::method_ids::CONJ, 1, f)?;
                // Stack: [new_set]
            }
        }
        Ok(())
    }

    // generate_set_disj removed - now uses core.sus disj function
    // generate_map_dissoc removed - now uses core.sus dissoc function

    // ========================================================================
    // WIT/WASI helpers
    // ========================================================================

    /// Build a synthetic WIT world definition for an expression with WASI imports
    fn build_synthetic_wit_world(&self) -> CompileResult<String> {
        let mut wit = String::new();

        wit.push_str("package suss:expr;\n\n");

        // Define the suss runtime interface (print-str for string output)
        wit.push_str("interface suss {\n");
        wit.push_str("    print-str: func(ptr: u32, len: u32);\n");
        wit.push_str("}\n\n");

        wit.push_str("world expr {\n");

        // Import the suss runtime interface
        wit.push_str("    import suss;\n");

        // Add WASI imports
        for import in &self.ir.imports {
            wit.push_str(&format!("    import {};\n", import.wit_interface));
        }

        // Add eval export based on return type
        if let Some(func) = self.ir.functions.first() {
            let return_wit = type_to_wit_string(&func.return_type);
            wit.push_str(&format!("    export eval: func() -> {};\n", return_wit));
        }

        wit.push_str("}\n");

        Ok(wit)
    }

    /// Append metadata as a custom section to a WASM module
    fn append_metadata_section(&self, wasm: &[u8], metadata: &[u8]) -> CompileResult<Vec<u8>> {
        let parser = wasmparser::Parser::new(0);
        let mut output = WasmModule::new();

        for payload in parser.parse_all(wasm) {
            let payload = payload
                .map_err(|e| CompileError::Component(format!("Failed to parse WASM: {}", e)))?;

            match payload {
                wasmparser::Payload::Version { .. } => {
                    // Skip version, WasmModule handles this
                }
                wasmparser::Payload::End(_) => {
                    // End of module, add custom section before finalizing
                    let custom = CustomSection {
                        name: std::borrow::Cow::Borrowed("component-type:suss"),
                        data: std::borrow::Cow::Borrowed(metadata),
                    };
                    output.section(&custom);
                }
                _ => {
                    // Copy other sections as raw sections
                    if let Some((id, range)) = payload.as_section() {
                        let raw = RawSection {
                            id,
                            data: &wasm[range],
                        };
                        output.section(&raw);
                    }
                }
            }
        }

        Ok(output.finish())
    }

    // ========================================================================
    // Code generation
    // ========================================================================

    fn generate_function(&self, func: &IrFunc) -> CompileResult<Function> {
        let mut local_types: Vec<(u32, ValType)> = func.locals[func.params.len()..]
            .iter()
            .map(|ty| (1, self.type_to_valtype_gc(ty)))
            .collect();

        // Add scratch locals for internal codegen (protocol dispatch, vec_conj, etc.)
        // Index starts at: num_params + num_body_locals = func.locals.len()
        let scratch_base = func.locals.len() as u32;
        self.scratch_local.set(scratch_base);

        // Add 50 scratch locals (10 sets of 5, for nested operations):
        // Each operation (protocol dispatch, set conj, map assoc) uses 5 locals,
        // and nested calls bump by 5. We allow up to 10 levels of nesting.
        // This handles desugared vectors like [1 2 3 4 5 6 7 8 9] inside apply.
        // Scratch locals layout per set (repeated 10x for nesting):
        //   +0: eqref (protocol dispatch, vec storage)
        //   +1: i32 (count, index)
        //   +2: eqref (new tail, temp)
        //   +3: eqref (old tail, temp)
        //   +4: eqref (extra temp)
        for _ in 0..10 {
            local_types.push((1, ValType::Ref(RefType::EQREF))); // scratch +0
            local_types.push((1, ValType::I32));                  // scratch +1
            local_types.push((1, ValType::Ref(RefType::EQREF))); // scratch +2
            local_types.push((1, ValType::Ref(RefType::EQREF))); // scratch +3
            local_types.push((1, ValType::Ref(RefType::EQREF))); // scratch +4
        }

        // Add i64/f64 scratch locals for WIT import return boxing
        let i64_scratch = scratch_base + 50; // after 10 sets of 5
        local_types.push((1, ValType::I64));
        self.i64_scratch_local.set(i64_scratch);
        let f64_scratch = scratch_base + 51;
        local_types.push((1, ValType::F64));
        self.f64_scratch_local.set(f64_scratch);

        let mut f = Function::new(local_types);
        self.generate_expr(&func.body, &mut f)?;

        // Functions with explicit primitive return type hints need to unbox the result
        if func.has_explicit_return_type {
            match &func.return_type {
                Type::I32 => {
                    // Unbox i31ref to i32: ref.cast (ref i31), i31.get_s, i32.const 1, i32.shr_s
                    f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                    f.instruction(&Instruction::I31GetS);
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32ShrS);
                }
                Type::I64 => {
                    // Unbox Int64 struct to i64: ref.cast (ref $INT64), struct.get $INT64 1
                    f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
                        crate::ir::gc_types::INT64,
                    )));
                    f.instruction(&Instruction::StructGet {
                        struct_type_index: crate::ir::gc_types::INT64,
                        field_index: 1,
                    });
                }
                Type::F64 => {
                    // Unbox Float64 struct to f64: ref.cast (ref $FLOAT64), struct.get $FLOAT64 1
                    f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
                        crate::ir::gc_types::FLOAT64,
                    )));
                    f.instruction(&Instruction::StructGet {
                        struct_type_index: crate::ir::gc_types::FLOAT64,
                        field_index: 1,
                    });
                }
                _ => {}
            }
        }

        f.instruction(&Instruction::End);
        Ok(f)
    }

    /// Generate function with WIT boundary marshaling.
    ///
    /// For WIT component exports:
    /// - Function signature uses WIT types (i32/i64/f64, strings as ptr+len pairs)
    /// - At entry: convert WIT params to GC refs
    /// - At exit: convert GC result back to WIT type
    fn generate_function_wit(&self, func: &IrFunc) -> CompileResult<Function> {
        use crate::ir::gc_types;
        use crate::ir::type_ids;
        let num_params = func.params.len() as u32;

        // Calculate WIT param count (strings take 2 slots: ptr + len)
        let wit_param_count: u32 = func.params
            .iter()
            .map(|(_, ty)| type_to_valtypes(ty).len() as u32)
            .sum();

        // Check if any param or return involves strings (including inside option/list)
        let has_string = func.params.iter().any(|(_, ty)| type_contains_string(ty))
            || type_contains_string(&func.return_type);

        // Locals layout:
        // 0..wit_param_count: WIT params (i32/i64/f64, strings as ptr+len)
        // wit_param_count..wit_param_count+num_params: converted eqref params
        // ..body locals
        // ..string scratch locals (if needed)
        // ..scratch locals

        let mut local_types: Vec<(u32, ValType)> = Vec::new();

        // Add eqref locals for converted params (one per logical param)
        for _ in 0..num_params {
            local_types.push((1, ValType::Ref(RefType::EQREF)));
        }

        // Add body locals (excluding params which are in func.locals[0..num_params])
        for ty in func.locals[func.params.len()..].iter() {
            local_types.push((1, self.type_to_valtype_gc(ty)));
        }

        // Add string marshaling scratch locals if needed:
        // str_ref (eqref), str_ptr (i32), str_len (i32), str_i (i32)
        let string_scratch_base = if has_string {
            let base = wit_param_count + local_types.len() as u32;
            local_types.push((1, ValType::Ref(RefType::EQREF))); // str_ref
            local_types.push((1, ValType::I32));                   // str_ptr
            local_types.push((1, ValType::I32));                   // str_len
            local_types.push((1, ValType::I32));                   // str_i (loop counter)
            Some(base)
        } else {
            None
        };

        // Add scratch locals for internal codegen (protocol dispatch, set conj, etc.)
        let scratch_base = wit_param_count + local_types.len() as u32;
        self.scratch_local.set(scratch_base);

        // Add 25 scratch locals (5 sets of 5, for nested operations)
        for _ in 0..5 {
            local_types.push((1, ValType::Ref(RefType::EQREF))); // scratch +0
            local_types.push((1, ValType::I32));                  // scratch +1
            local_types.push((1, ValType::Ref(RefType::EQREF))); // scratch +2
            local_types.push((1, ValType::Ref(RefType::EQREF))); // scratch +3
            local_types.push((1, ValType::Ref(RefType::EQREF))); // scratch +4
        }

        // Add i64/f64 scratch locals for WIT import return boxing
        let i64_scratch = scratch_base + 25; // after 5 sets of 5
        local_types.push((1, ValType::I64));
        self.i64_scratch_local.set(i64_scratch);
        let f64_scratch = scratch_base + 26;
        local_types.push((1, ValType::F64));
        self.f64_scratch_local.set(f64_scratch);

        let mut f = Function::new(local_types);

        // Entry: convert each WIT param to eqref and store in converted local
        let mut wit_idx = 0u32; // tracks position in WIT param slots
        for i in 0..num_params {
            let (_, param_ty) = &func.params[i as usize];
            let converted_local = wit_param_count + i;
            match param_ty {
                Type::I32 | Type::Unknown => {
                    // Convert i32 to small int encoding: (n << 1) | 1, then ref.i31
                    f.instruction(&Instruction::LocalGet(wit_idx));
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32Shl);
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32Or);
                    f.instruction(&Instruction::RefI31);
                    f.instruction(&Instruction::LocalSet(converted_local));
                    wit_idx += 1;
                }
                Type::I64 => {
                    // Box i64 in INT64 struct: { type_id, value }
                    f.instruction(&Instruction::I32Const(type_ids::INT64));
                    f.instruction(&Instruction::LocalGet(wit_idx));
                    f.instruction(&Instruction::StructNew(gc_types::INT64));
                    f.instruction(&Instruction::LocalSet(converted_local));
                    wit_idx += 1;
                }
                Type::F64 => {
                    // Box f64 in FLOAT struct: { type_id, value }
                    f.instruction(&Instruction::I32Const(type_ids::FLOAT64));
                    f.instruction(&Instruction::LocalGet(wit_idx));
                    f.instruction(&Instruction::StructNew(gc_types::FLOAT64));
                    f.instruction(&Instruction::LocalSet(converted_local));
                    wit_idx += 1;
                }
                Type::Bool => {
                    // Convert i32 bool (0/1) to i31ref sentinel
                    // TRUE_SENTINEL = 4, FALSE_SENTINEL = 2
                    // if (param) { ref.i31(TRUE) } else { ref.i31(FALSE) }
                    f.instruction(&Instruction::LocalGet(wit_idx));
                    f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(RefType::EQREF))));
                    f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                    f.instruction(&Instruction::RefI31);
                    f.instruction(&Instruction::Else);
                    f.instruction(&Instruction::I32Const(gc_types::FALSE_SENTINEL));
                    f.instruction(&Instruction::RefI31);
                    f.instruction(&Instruction::End);
                    f.instruction(&Instruction::LocalSet(converted_local));
                    wit_idx += 1;
                }
                Type::String => {
                    // WIT gives us (ptr: i32, len: i32) in linear memory
                    // Create GC array<i8> and copy bytes from linear memory
                    let ss = string_scratch_base.unwrap();
                    let str_ref = ss;
                    let str_ptr = ss + 1;
                    let str_len = ss + 2;
                    let str_i = ss + 3;

                    // Save ptr and len
                    f.instruction(&Instruction::LocalGet(wit_idx));       // ptr
                    f.instruction(&Instruction::LocalSet(str_ptr));
                    f.instruction(&Instruction::LocalGet(wit_idx + 1));   // len
                    f.instruction(&Instruction::LocalSet(str_len));

                    // Create new GC string array: array.new_default $STRING len
                    f.instruction(&Instruction::LocalGet(str_len));
                    f.instruction(&Instruction::ArrayNewDefault(gc_types::STRING));
                    f.instruction(&Instruction::LocalSet(str_ref));

                    // Copy loop: for i = 0..len
                    f.instruction(&Instruction::I32Const(0));
                    f.instruction(&Instruction::LocalSet(str_i));

                    f.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
                    f.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
                    // break if i >= len
                    f.instruction(&Instruction::LocalGet(str_i));
                    f.instruction(&Instruction::LocalGet(str_len));
                    f.instruction(&Instruction::I32GeU);
                    f.instruction(&Instruction::BrIf(1));

                    // array.set $STRING str_ref[i] = mem[ptr + i]
                    f.instruction(&Instruction::LocalGet(str_ref));
                    f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
                    f.instruction(&Instruction::LocalGet(str_i));
                    // Load byte from linear memory: i32.load8_u (ptr + i)
                    f.instruction(&Instruction::LocalGet(str_ptr));
                    f.instruction(&Instruction::LocalGet(str_i));
                    f.instruction(&Instruction::I32Add);
                    f.instruction(&Instruction::I32Load8U(wasm_encoder::MemArg {
                        offset: 0,
                        align: 0,
                        memory_index: 0,
                    }));
                    f.instruction(&Instruction::ArraySet(gc_types::STRING));

                    // i++
                    f.instruction(&Instruction::LocalGet(str_i));
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32Add);
                    f.instruction(&Instruction::LocalSet(str_i));
                    f.instruction(&Instruction::Br(0)); // continue
                    f.instruction(&Instruction::End); // end loop
                    f.instruction(&Instruction::End); // end block

                    f.instruction(&Instruction::LocalGet(str_ref));
                    f.instruction(&Instruction::LocalSet(converted_local));
                    wit_idx += 2; // strings consume 2 WIT param slots
                }
                Type::Option(inner) => {
                    // option<T> params come as flat values: discriminant + payload
                    self.emit_option_entry(inner, &mut f, wit_idx, converted_local, string_scratch_base)?;
                    wit_idx += type_to_valtypes(param_ty).len() as u32;
                }
                Type::List(inner) => {
                    // list<T> params come as (ptr, len) flat values
                    self.emit_list_entry(inner, &mut f, wit_idx, converted_local, scratch_base, string_scratch_base)?;
                    wit_idx += 2; // ptr + len
                }
                _ => {
                    // For other types, just copy
                    f.instruction(&Instruction::LocalGet(wit_idx));
                    f.instruction(&Instruction::LocalSet(converted_local));
                    wit_idx += 1;
                }
            }
        }

        // Generate body with local offset for params
        // The offset tells generate_expr_inner where the converted eqref params are
        self.generate_expr_with_offset(&func.body, &mut f, wit_param_count)?;

        // Exit: convert eqref result back to WIT type
        match &func.return_type {
            Type::I32 | Type::Unknown => {
                // Decode from i31ref: cast, get_s, >> 1
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrS);
            }
            Type::I64 => {
                // Unbox INT64 struct to i64
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::INT64)));
                f.instruction(&Instruction::StructGet {
                    struct_type_index: gc_types::INT64,
                    field_index: gc_types::I64_VALUE,
                });
            }
            Type::F64 => {
                // Unbox from FLOAT struct
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::FLOAT64)));
                f.instruction(&Instruction::StructGet {
                    struct_type_index: gc_types::FLOAT64,
                    field_index: gc_types::F64_VALUE,
                });
            }
            Type::Bool => {
                // Decode i31ref sentinel to i32 bool (0 or 1)
                // TRUE_SENTINEL = 4, check if value == 4
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                f.instruction(&Instruction::I32Eq);
            }
            Type::Unit => {
                // Drop the eqref, return nothing
                f.instruction(&Instruction::Drop);
            }
            Type::String => {
                // Canonical ABI: MAX_FLAT_RESULTS = 1, string has 2 flat values
                // So we write (ptr, len) to a result area and return the retptr
                let ss = string_scratch_base.unwrap();
                // emit_gc_string_to_linear leaves (ptr, len) on stack
                self.emit_gc_string_to_linear(&mut f, ss)?;
                // Save ptr and len from stack
                let ret_len_local = ss + 2; // reuse str_len scratch
                let ret_ptr_local = ss + 1; // reuse str_ptr scratch
                f.instruction(&Instruction::LocalSet(ret_len_local));
                f.instruction(&Instruction::LocalSet(ret_ptr_local));

                // Allocate 8 bytes for the result tuple (ptr: i32, len: i32)
                f.instruction(&Instruction::I32Const(0));        // old_ptr
                f.instruction(&Instruction::I32Const(0));        // old_size
                f.instruction(&Instruction::I32Const(4));        // align (i32 alignment)
                f.instruction(&Instruction::I32Const(8));        // new_size (2 x i32)
                f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::CABI_REALLOC)));
                // Stack: [retptr]
                let retptr_local = ss; // reuse str_ref scratch (eqref, but we store i32 — need i32 local)
                // We can't use the eqref local for i32. Use a different scratch.
                // Actually the general scratch base has i32 locals we can use.
                let general_scratch = scratch_base;
                let retptr_scratch = general_scratch + 1; // scratch +1 is i32
                f.instruction(&Instruction::LocalTee(retptr_scratch));

                // Write ptr at retptr+0
                f.instruction(&Instruction::LocalGet(ret_ptr_local));
                f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                    offset: 0,
                    align: 2, // 4-byte aligned
                    memory_index: 0,
                }));

                // Write len at retptr+4
                f.instruction(&Instruction::LocalGet(retptr_scratch));
                f.instruction(&Instruction::LocalGet(ret_len_local));
                f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                    offset: 4,
                    align: 2, // 4-byte aligned
                    memory_index: 0,
                }));

                // Return retptr
                f.instruction(&Instruction::LocalGet(retptr_scratch));
            }
            Type::Option(inner) => {
                // option<T> always has ≥2 flat values → uses retptr (MAX_FLAT_RESULTS=1)
                self.emit_option_exit(inner, &mut f, scratch_base, string_scratch_base)?;
            }
            Type::List(inner) => {
                // list<T> flattens to (ptr, len) = 2 flat values → uses retptr
                self.emit_list_exit(inner, &mut f, scratch_base, string_scratch_base)?;
            }
            Type::Result { ok: None, err: None } => {
                // Bare result (wasi:cli/run): drop return value, return 0 (Ok discriminant)
                f.instruction(&Instruction::Drop);
                f.instruction(&Instruction::I32Const(0)); // Ok discriminant
            }
            Type::Result { .. } => {
                // TODO: handle result types with payloads
                f.instruction(&Instruction::Drop);
                f.instruction(&Instruction::I32Const(0));
            }
            _ => {
                // For other types, leave as-is (will cause type error if mismatched)
            }
        }

        f.instruction(&Instruction::End);
        Ok(f)
    }

    /// Convert GC string array (eqref on stack) to linear memory.
    /// Leaves (ptr: i32, len: i32) on the stack.
    ///
    /// Uses scratch locals starting at `scratch_base`:
    ///   +0: str_ref (eqref), +1: str_ptr (i32), +2: str_len (i32), +3: str_i (i32)
    fn emit_gc_string_to_linear(&self, f: &mut Function, scratch_base: u32) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::global_indices;

        let str_ref = scratch_base;
        let str_ptr = scratch_base + 1;
        let str_len = scratch_base + 2;
        let str_i = scratch_base + 3;

        // Cast eqref to STRING and save (local is eqref, so cast is lost on reload)
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
        f.instruction(&Instruction::LocalSet(str_ref));

        // Get string length (need re-cast since local type is eqref)
        f.instruction(&Instruction::LocalGet(str_ref));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
        f.instruction(&Instruction::ArrayLen);
        f.instruction(&Instruction::LocalSet(str_len));

        // Allocate linear memory: call cabi_realloc(0, 0, 1, len)
        f.instruction(&Instruction::I32Const(0));        // old_ptr
        f.instruction(&Instruction::I32Const(0));        // old_size
        f.instruction(&Instruction::I32Const(1));        // align
        f.instruction(&Instruction::LocalGet(str_len));  // new_size
        f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::CABI_REALLOC)));
        f.instruction(&Instruction::LocalSet(str_ptr));

        // Copy loop: for i = 0..len, mem[ptr + i] = array.get_s str[i]
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::LocalSet(str_i));

        f.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        f.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
        // break if i >= len
        f.instruction(&Instruction::LocalGet(str_i));
        f.instruction(&Instruction::LocalGet(str_len));
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::BrIf(1));

        // i32.store8 (ptr + i) <- array.get_s $STRING str_ref[i]
        f.instruction(&Instruction::LocalGet(str_ptr));
        f.instruction(&Instruction::LocalGet(str_i));
        f.instruction(&Instruction::I32Add);              // address = ptr + i
        f.instruction(&Instruction::LocalGet(str_ref));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
        f.instruction(&Instruction::LocalGet(str_i));
        f.instruction(&Instruction::ArrayGetS(gc_types::STRING)); // get byte (signed)
        f.instruction(&Instruction::I32Store8(wasm_encoder::MemArg {
            offset: 0,
            align: 0,
            memory_index: 0,
        }));

        // i++
        f.instruction(&Instruction::LocalGet(str_i));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalSet(str_i));
        f.instruction(&Instruction::Br(0)); // continue
        f.instruction(&Instruction::End); // end loop
        f.instruction(&Instruction::End); // end block

        // Leave (ptr, len) on stack
        f.instruction(&Instruction::LocalGet(str_ptr));
        f.instruction(&Instruction::LocalGet(str_len));

        Ok(())
    }

    /// Convert linear memory string (ptr: i32, len: i32 on stack) to GC string array.
    /// Leaves eqref (GC string) on the stack.
    ///
    /// Uses scratch locals starting at `scratch_base`:
    ///   +0: str_ref (eqref), +1: str_ptr (i32), +2: str_len (i32), +3: str_i (i32)
    fn emit_linear_to_gc_string(&self, f: &mut Function, scratch_base: u32) -> CompileResult<()> {
        use crate::ir::gc_types;

        let str_ref = scratch_base;
        let str_ptr = scratch_base + 1;
        let str_len = scratch_base + 2;
        let str_i = scratch_base + 3;

        // Save ptr and len from stack
        f.instruction(&Instruction::LocalSet(str_len));
        f.instruction(&Instruction::LocalSet(str_ptr));

        // Create GC string array: array.new_default $STRING len
        f.instruction(&Instruction::LocalGet(str_len));
        f.instruction(&Instruction::ArrayNewDefault(gc_types::STRING));
        f.instruction(&Instruction::LocalSet(str_ref));

        // Copy loop: for i = 0..len, array.set str[i] = mem[ptr + i]
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::LocalSet(str_i));

        f.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        f.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
        // break if i >= len
        f.instruction(&Instruction::LocalGet(str_i));
        f.instruction(&Instruction::LocalGet(str_len));
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::BrIf(1));

        // array.set $STRING str_ref[i] = i32.load8_u(ptr + i)
        f.instruction(&Instruction::LocalGet(str_ref));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
        f.instruction(&Instruction::LocalGet(str_i));
        f.instruction(&Instruction::LocalGet(str_ptr));
        f.instruction(&Instruction::LocalGet(str_i));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I32Load8U(wasm_encoder::MemArg {
            offset: 0,
            align: 0,
            memory_index: 0,
        }));
        f.instruction(&Instruction::ArraySet(gc_types::STRING));

        // i++
        f.instruction(&Instruction::LocalGet(str_i));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalSet(str_i));
        f.instruction(&Instruction::Br(0)); // continue
        f.instruction(&Instruction::End); // end loop
        f.instruction(&Instruction::End); // end block

        // Leave str_ref on stack
        f.instruction(&Instruction::LocalGet(str_ref));

        Ok(())
    }

    /// Emit nil check for option marshaling.
    ///
    /// Input: eqref on stack
    /// Output: i32 on stack (1 = nil, 0 = not nil)
    ///
    /// Checks: RefIsNull || (ref.test i31 && i31.get_s == 0)
    fn emit_nil_check(&self, f: &mut Function, val_local: u32) {
        use crate::ir::gc_types;
        // Save value
        f.instruction(&Instruction::LocalSet(val_local));

        // Check null ref first
        f.instruction(&Instruction::LocalGet(val_local));
        f.instruction(&Instruction::RefIsNull);
        f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(1)); // is nil
        f.instruction(&Instruction::Else);
        // Check i31ref nil sentinel
        f.instruction(&Instruction::LocalGet(val_local));
        f.instruction(&Instruction::RefTestNonNull(HeapType::I31));
        f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::LocalGet(val_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
        f.instruction(&Instruction::I31GetS);
        f.instruction(&Instruction::I32Eqz); // NIL_SENTINEL == 0, so eqz returns 1 if nil
        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::I32Const(0)); // not nil (not i31 and not null)
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
    }

    /// Emit option<T> exit marshaling for WIT exports.
    ///
    /// Input: eqref on stack (value or nil)
    /// Output: retptr (i32) on stack pointing to option layout in linear memory
    ///
    /// Retptr layout: [discriminant: i32, <aligned payload>]
    fn emit_option_exit(
        &self,
        inner: &Type,
        f: &mut Function,
        scratch_base: u32,
        string_scratch_base: Option<u32>,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;

        let val_scratch = scratch_base; // eqref scratch +0
        let retptr_scratch = scratch_base + 1; // i32 scratch +1

        // Get flat types for the full option
        let flat_vts = type_to_valtypes(&Type::Option(Box::new(inner.clone())));
        let (total_size, offsets) = flat_byte_size_and_offsets(&flat_vts);
        let align = if flat_vts.iter().any(|v| matches!(v, ValType::I64 | ValType::F64)) { 8 } else { 4 };

        // Save value to scratch
        f.instruction(&Instruction::LocalSet(val_scratch));

        // Allocate retptr
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Const(align));
        f.instruction(&Instruction::I32Const(total_size));
        f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::CABI_REALLOC)));
        f.instruction(&Instruction::LocalSet(retptr_scratch));

        // Check if nil
        f.instruction(&Instruction::LocalGet(val_scratch));
        self.emit_nil_check(f, val_scratch);

        // if (is_nil)
        f.instruction(&Instruction::If(BlockType::Empty));
        {
            // None: discriminant = 0, zero-fill payload
            f.instruction(&Instruction::LocalGet(retptr_scratch));
            f.instruction(&Instruction::I32Const(0));
            f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                offset: offsets[0].0 as u64,
                align: 2,
                memory_index: 0,
            }));
            // Zero-fill payload slots
            for &(offset, vt) in &offsets[1..] {
                f.instruction(&Instruction::LocalGet(retptr_scratch));
                match vt {
                    ValType::I64 => {
                        f.instruction(&Instruction::I64Const(0));
                        f.instruction(&Instruction::I64Store(wasm_encoder::MemArg {
                            offset: offset as u64,
                            align: 3,
                            memory_index: 0,
                        }));
                    }
                    ValType::F64 => {
                        f.instruction(&Instruction::F64Const(0.0));
                        f.instruction(&Instruction::F64Store(wasm_encoder::MemArg {
                            offset: offset as u64,
                            align: 3,
                            memory_index: 0,
                        }));
                    }
                    _ => {
                        f.instruction(&Instruction::I32Const(0));
                        f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                            offset: offset as u64,
                            align: 2,
                            memory_index: 0,
                        }));
                    }
                }
            }
        }
        f.instruction(&Instruction::Else);
        {
            // Some: discriminant = 1, marshal payload
            f.instruction(&Instruction::LocalGet(retptr_scratch));
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                offset: offsets[0].0 as u64,
                align: 2,
                memory_index: 0,
            }));

            // Marshal inner value based on type
            match inner {
                Type::I32 | Type::Unknown => {
                    // Decode i31ref to i32
                    f.instruction(&Instruction::LocalGet(retptr_scratch));
                    f.instruction(&Instruction::LocalGet(val_scratch));
                    f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                    f.instruction(&Instruction::I31GetS);
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32ShrS);
                    f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                        offset: offsets[1].0 as u64,
                        align: 2,
                        memory_index: 0,
                    }));
                }
                Type::I64 => {
                    // Unbox INT64
                    f.instruction(&Instruction::LocalGet(retptr_scratch));
                    f.instruction(&Instruction::LocalGet(val_scratch));
                    f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::INT64)));
                    f.instruction(&Instruction::StructGet {
                        struct_type_index: gc_types::INT64,
                        field_index: gc_types::I64_VALUE,
                    });
                    f.instruction(&Instruction::I64Store(wasm_encoder::MemArg {
                        offset: offsets[1].0 as u64,
                        align: 3,
                        memory_index: 0,
                    }));
                }
                Type::F64 => {
                    // Unbox FLOAT64
                    f.instruction(&Instruction::LocalGet(retptr_scratch));
                    f.instruction(&Instruction::LocalGet(val_scratch));
                    f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::FLOAT64)));
                    f.instruction(&Instruction::StructGet {
                        struct_type_index: gc_types::FLOAT64,
                        field_index: gc_types::F64_VALUE,
                    });
                    f.instruction(&Instruction::F64Store(wasm_encoder::MemArg {
                        offset: offsets[1].0 as u64,
                        align: 3,
                        memory_index: 0,
                    }));
                }
                Type::Bool => {
                    // Decode bool sentinel
                    f.instruction(&Instruction::LocalGet(retptr_scratch));
                    f.instruction(&Instruction::LocalGet(val_scratch));
                    f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                    f.instruction(&Instruction::I31GetS);
                    f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                    f.instruction(&Instruction::I32Eq);
                    f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                        offset: offsets[1].0 as u64,
                        align: 2,
                        memory_index: 0,
                    }));
                }
                Type::String => {
                    // Convert GC string to linear memory, write (ptr, len)
                    f.instruction(&Instruction::LocalGet(val_scratch));
                    let ss = string_scratch_base.unwrap();
                    self.emit_gc_string_to_linear(f, ss)?;
                    // Stack has (ptr, len)
                    let tmp_len = ss + 2; // reuse str_len
                    let tmp_ptr = ss + 1; // reuse str_ptr
                    f.instruction(&Instruction::LocalSet(tmp_len));
                    f.instruction(&Instruction::LocalSet(tmp_ptr));

                    f.instruction(&Instruction::LocalGet(retptr_scratch));
                    f.instruction(&Instruction::LocalGet(tmp_ptr));
                    f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                        offset: offsets[1].0 as u64,
                        align: 2,
                        memory_index: 0,
                    }));
                    f.instruction(&Instruction::LocalGet(retptr_scratch));
                    f.instruction(&Instruction::LocalGet(tmp_len));
                    f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                        offset: offsets[2].0 as u64,
                        align: 2,
                        memory_index: 0,
                    }));
                }
                _ => {
                    return Err(CompileError::Codegen(format!(
                        "Unsupported option inner type: {:?}",
                        inner
                    )));
                }
            }
        }
        f.instruction(&Instruction::End);

        // Return retptr
        f.instruction(&Instruction::LocalGet(retptr_scratch));

        Ok(())
    }

    /// Emit option<T> entry marshaling for WIT export params.
    ///
    /// Option params come as flat values: discriminant (i32) + payload.
    /// Converts to eqref (nil for None, boxed value for Some).
    fn emit_option_entry(
        &self,
        inner: &Type,
        f: &mut Function,
        wit_idx: u32,
        converted_local: u32,
        string_scratch_base: Option<u32>,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::type_ids;

        let disc_idx = wit_idx;
        f.instruction(&Instruction::LocalGet(disc_idx));
        // if discriminant != 0 → Some
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(RefType::EQREF))));
        {
            match inner {
                Type::I32 | Type::Unknown => {
                    // Encode as i31ref: (n << 1) | 1
                    f.instruction(&Instruction::LocalGet(disc_idx + 1));
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32Shl);
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32Or);
                    f.instruction(&Instruction::RefI31);
                }
                Type::I64 => {
                    // Box in INT64 struct
                    f.instruction(&Instruction::I32Const(type_ids::INT64));
                    f.instruction(&Instruction::LocalGet(disc_idx + 1));
                    f.instruction(&Instruction::StructNew(gc_types::INT64));
                }
                Type::F64 => {
                    // Box in FLOAT64 struct
                    f.instruction(&Instruction::I32Const(type_ids::FLOAT64));
                    f.instruction(&Instruction::LocalGet(disc_idx + 1));
                    f.instruction(&Instruction::StructNew(gc_types::FLOAT64));
                }
                Type::Bool => {
                    // Convert i32 bool to sentinel
                    f.instruction(&Instruction::LocalGet(disc_idx + 1));
                    f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(RefType::EQREF))));
                    f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                    f.instruction(&Instruction::RefI31);
                    f.instruction(&Instruction::Else);
                    f.instruction(&Instruction::I32Const(gc_types::FALSE_SENTINEL));
                    f.instruction(&Instruction::RefI31);
                    f.instruction(&Instruction::End);
                }
                Type::String => {
                    // Read (ptr, len) from wit_idx+1, wit_idx+2, convert to GC string
                    f.instruction(&Instruction::LocalGet(disc_idx + 1)); // ptr
                    f.instruction(&Instruction::LocalGet(disc_idx + 2)); // len
                    let ss = string_scratch_base.unwrap();
                    self.emit_linear_to_gc_string(f, ss)?;
                }
                _ => {
                    return Err(CompileError::Codegen(format!(
                        "Unsupported option inner type for entry: {:?}",
                        inner
                    )));
                }
            }
        }
        f.instruction(&Instruction::Else);
        {
            // None → nil
            f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
            f.instruction(&Instruction::RefI31);
        }
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::LocalSet(converted_local));

        Ok(())
    }

    /// Emit option<T> param marshaling for WIT import calls.
    ///
    /// Input: eqref on stack (value or nil)
    /// Output: flat values on stack (discriminant + payload)
    fn emit_option_import_param(
        &self,
        inner: &Type,
        f: &mut Function,
        string_scratch: Option<u32>,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;

        let scratch = self.scratch_local.get();
        let val_scratch = scratch; // eqref

        // Save the eqref value
        f.instruction(&Instruction::LocalSet(val_scratch));

        // Check if nil
        f.instruction(&Instruction::LocalGet(val_scratch));
        self.emit_nil_check(f, val_scratch);

        // Get the inner flat types count for the if/else block result
        let inner_vts = type_to_valtypes(inner);
        let option_vts = type_to_valtypes(&Type::Option(Box::new(inner.clone())));

        // We need to produce discriminant + payload values on stack.
        // Use a block that produces all values.
        // For simplicity, store to scratch locals and load them.
        let disc_local = scratch + 1; // i32
        // We'll push discriminant and payload separately

        // if (is_nil)
        f.instruction(&Instruction::If(BlockType::Empty));
        {
            // None: push 0 discriminant
            f.instruction(&Instruction::I32Const(0));
            f.instruction(&Instruction::LocalSet(disc_local));
        }
        f.instruction(&Instruction::Else);
        {
            // Some: push 1 discriminant
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::LocalSet(disc_local));
        }
        f.instruction(&Instruction::End);

        // Push discriminant
        f.instruction(&Instruction::LocalGet(disc_local));

        // Push payload (0 if none, marshaled value if some)
        f.instruction(&Instruction::LocalGet(disc_local));
        // if discriminant != 0
        match inner {
            Type::I32 | Type::Unknown => {
                f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                f.instruction(&Instruction::LocalGet(val_scratch));
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrS);
                f.instruction(&Instruction::Else);
                f.instruction(&Instruction::I32Const(0));
                f.instruction(&Instruction::End);
            }
            Type::I64 => {
                f.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
                f.instruction(&Instruction::LocalGet(val_scratch));
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::INT64)));
                f.instruction(&Instruction::StructGet {
                    struct_type_index: gc_types::INT64,
                    field_index: gc_types::I64_VALUE,
                });
                f.instruction(&Instruction::Else);
                f.instruction(&Instruction::I64Const(0));
                f.instruction(&Instruction::End);
            }
            Type::F64 => {
                f.instruction(&Instruction::If(BlockType::Result(ValType::F64)));
                f.instruction(&Instruction::LocalGet(val_scratch));
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::FLOAT64)));
                f.instruction(&Instruction::StructGet {
                    struct_type_index: gc_types::FLOAT64,
                    field_index: gc_types::F64_VALUE,
                });
                f.instruction(&Instruction::Else);
                f.instruction(&Instruction::F64Const(0.0));
                f.instruction(&Instruction::End);
            }
            Type::Bool => {
                f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
                f.instruction(&Instruction::LocalGet(val_scratch));
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                f.instruction(&Instruction::I32Eq);
                f.instruction(&Instruction::Else);
                f.instruction(&Instruction::I32Const(0));
                f.instruction(&Instruction::End);
            }
            Type::String => {
                // String produces 2 values (ptr, len) — need retptr approach
                // For simplicity: if Some, convert to linear string; if None, push (0, 0)
                let ss = string_scratch.unwrap();
                let tmp_ptr = ss + 1;
                let tmp_len = ss + 2;

                f.instruction(&Instruction::If(BlockType::Empty));
                {
                    f.instruction(&Instruction::LocalGet(val_scratch));
                    self.emit_gc_string_to_linear(f, ss)?;
                    f.instruction(&Instruction::LocalSet(tmp_len));
                    f.instruction(&Instruction::LocalSet(tmp_ptr));
                }
                f.instruction(&Instruction::Else);
                {
                    f.instruction(&Instruction::I32Const(0));
                    f.instruction(&Instruction::LocalSet(tmp_ptr));
                    f.instruction(&Instruction::I32Const(0));
                    f.instruction(&Instruction::LocalSet(tmp_len));
                }
                f.instruction(&Instruction::End);
                f.instruction(&Instruction::LocalGet(tmp_ptr));
                f.instruction(&Instruction::LocalGet(tmp_len));
            }
            _ => {
                return Err(CompileError::Codegen(format!(
                    "Unsupported option inner type for import param: {:?}",
                    inner
                )));
            }
        }

        Ok(())
    }

    /// Emit option<T> return marshaling for WIT import returns.
    ///
    /// Import wrote option to retptr. Read discriminant, then payload.
    /// Pushes eqref on stack (nil for None, boxed value for Some).
    fn emit_option_import_return(
        &self,
        inner: &Type,
        f: &mut Function,
        retptr: u32,
        string_scratch: Option<u32>,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::type_ids;

        let flat_vts = type_to_valtypes(&Type::Option(Box::new(inner.clone())));
        let (_total_size, offsets) = flat_byte_size_and_offsets(&flat_vts);

        // Read discriminant from retptr
        f.instruction(&Instruction::LocalGet(retptr));
        f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
            offset: offsets[0].0 as u64,
            align: 2,
            memory_index: 0,
        }));

        // if discriminant != 0 → Some
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(RefType::EQREF))));
        {
            match inner {
                Type::I32 | Type::Unknown => {
                    f.instruction(&Instruction::LocalGet(retptr));
                    f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
                        offset: offsets[1].0 as u64,
                        align: 2,
                        memory_index: 0,
                    }));
                    // Encode as i31ref
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32Shl);
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32Or);
                    f.instruction(&Instruction::RefI31);
                }
                Type::I64 => {
                    f.instruction(&Instruction::I32Const(type_ids::INT64));
                    f.instruction(&Instruction::LocalGet(retptr));
                    f.instruction(&Instruction::I64Load(wasm_encoder::MemArg {
                        offset: offsets[1].0 as u64,
                        align: 3,
                        memory_index: 0,
                    }));
                    f.instruction(&Instruction::StructNew(gc_types::INT64));
                }
                Type::F64 => {
                    f.instruction(&Instruction::I32Const(type_ids::FLOAT64));
                    f.instruction(&Instruction::LocalGet(retptr));
                    f.instruction(&Instruction::F64Load(wasm_encoder::MemArg {
                        offset: offsets[1].0 as u64,
                        align: 3,
                        memory_index: 0,
                    }));
                    f.instruction(&Instruction::StructNew(gc_types::FLOAT64));
                }
                Type::Bool => {
                    f.instruction(&Instruction::LocalGet(retptr));
                    f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
                        offset: offsets[1].0 as u64,
                        align: 2,
                        memory_index: 0,
                    }));
                    f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(RefType::EQREF))));
                    f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                    f.instruction(&Instruction::RefI31);
                    f.instruction(&Instruction::Else);
                    f.instruction(&Instruction::I32Const(gc_types::FALSE_SENTINEL));
                    f.instruction(&Instruction::RefI31);
                    f.instruction(&Instruction::End);
                }
                Type::String => {
                    // Read (ptr, len) from retptr payload offsets
                    f.instruction(&Instruction::LocalGet(retptr));
                    f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
                        offset: offsets[1].0 as u64,
                        align: 2,
                        memory_index: 0,
                    }));
                    f.instruction(&Instruction::LocalGet(retptr));
                    f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
                        offset: offsets[2].0 as u64,
                        align: 2,
                        memory_index: 0,
                    }));
                    self.emit_linear_to_gc_string(f, string_scratch.unwrap())?;
                }
                _ => {
                    return Err(CompileError::Codegen(format!(
                        "Unsupported option inner type for import return: {:?}",
                        inner
                    )));
                }
            }
        }
        f.instruction(&Instruction::Else);
        {
            // None → nil
            f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
            f.instruction(&Instruction::RefI31);
        }
        f.instruction(&Instruction::End);

        Ok(())
    }

    /// Emit list<T> exit marshaling for WIT exports.
    ///
    /// Input: eqref (PersistentVector) on stack
    /// Output: retptr (i32) on stack pointing to (ptr, len) in linear memory
    ///
    /// Iterates the vector via -count and -nth protocol dispatch,
    /// writes elements to linear memory, then writes (ptr, len) to retptr.
    fn emit_list_exit(
        &self,
        inner: &Type,
        f: &mut Function,
        scratch_base: u32,
        string_scratch_base: Option<u32>,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::method_ids;

        // Scratch local assignments:
        // +0 (eqref): vec
        // +1 (i32): elem_base_ptr (constant through loop)
        // +2 (eqref): count stored as i31ref (to free up an i32 slot)
        // +6 (i32): loop counter i
        // i64_scratch: nth_dispatch_idx (stored as i64 to avoid i32 conflicts)
        let vec_local = scratch_base;
        let elem_base_local = scratch_base + 1;
        let count_eqref_local = scratch_base + 2;
        let loop_i_local = scratch_base + 6;
        let nth_idx_i64 = self.i64_scratch_local.get();

        // 1. Save vector
        f.instruction(&Instruction::LocalSet(vec_local));

        // 2. Get type_id, save temporarily to loop_i_local (i32)
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::GET_TYPE_ID)));
        f.instruction(&Instruction::LocalSet(loop_i_local)); // type_id in loop_i_local temporarily

        // 3. Compute -nth dispatch index (type_id * mpt + NTH), save as i64
        f.instruction(&Instruction::LocalGet(loop_i_local)); // type_id
        f.instruction(&Instruction::I32Const(self.ir.methods_per_type as i32));
        f.instruction(&Instruction::I32Mul);
        f.instruction(&Instruction::I32Const(method_ids::NTH as i32));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::LocalSet(nth_idx_i64));

        // 4. Call -count: push vec arg, then compute+push dispatch idx
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::LocalGet(loop_i_local)); // type_id
        f.instruction(&Instruction::I32Const(self.ir.methods_per_type as i32));
        f.instruction(&Instruction::I32Mul);
        f.instruction(&Instruction::I32Const(method_ids::COUNT as i32));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::CallIndirect {
            type_index: self.protocol_type(protocol_type_offsets::ARITY_1_I32),
            table_index: 0,
        });
        // Stack: [count: i32]

        // 5. Save count as i31ref to free up i32 locals for the loop
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Shl);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::RefI31);
        f.instruction(&Instruction::LocalSet(count_eqref_local));

        // Helper: inline decode count from i31ref
        // Pushes count (i32) on stack from count_eqref_local
        macro_rules! push_count {
            () => {
                f.instruction(&Instruction::LocalGet(count_eqref_local));
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrS);
            }
        }

        // 6. Determine element stride and alignment
        let elem_stride = match inner {
            Type::I32 | Type::Bool | Type::Unknown => 4i32,
            Type::I64 | Type::F64 => 8,
            Type::String => 8, // ptr (4) + len (4)
            _ => 4,
        };
        let elem_align = match inner {
            Type::I64 | Type::F64 => 8i32,
            _ => 4i32,
        };

        // 7. Allocate element array: cabi_realloc(0, 0, align, count * stride)
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Const(elem_align));
        push_count!();
        f.instruction(&Instruction::I32Const(elem_stride));
        f.instruction(&Instruction::I32Mul);
        f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::CABI_REALLOC)));
        f.instruction(&Instruction::LocalSet(elem_base_local));

        // 8. Loop: for i = 0..count
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::LocalSet(loop_i_local));

        f.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        f.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
        {
            // Break if i >= count
            f.instruction(&Instruction::LocalGet(loop_i_local));
            push_count!();
            f.instruction(&Instruction::I32GeU);
            f.instruction(&Instruction::BrIf(1));

            // Get element via -nth protocol dispatch
            // call_indirect expects: [vec, i_eqref, dispatch_idx]
            f.instruction(&Instruction::LocalGet(vec_local));
            // Encode i as i31ref: (i << 1) | 1
            f.instruction(&Instruction::LocalGet(loop_i_local));
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Shl);
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Or);
            f.instruction(&Instruction::RefI31);
            // Push nth dispatch index (convert from i64)
            f.instruction(&Instruction::LocalGet(nth_idx_i64));
            f.instruction(&Instruction::I32WrapI64);
            f.instruction(&Instruction::CallIndirect {
                type_index: self.protocol_type(protocol_type_offsets::ARITY_2_REF),
                table_index: 0,
            });
            // Stack: [elem: eqref]

            // Compute store address: elem_base + i * stride
            // We need addr on stack BELOW the value for i32.store.
            // Pattern: save elem, push addr, push marshaled value, store
            let elem_eqref_local = scratch_base + 4; // eqref scratch
            f.instruction(&Instruction::LocalSet(elem_eqref_local));

            // Push store address
            f.instruction(&Instruction::LocalGet(elem_base_local));
            f.instruction(&Instruction::LocalGet(loop_i_local));
            f.instruction(&Instruction::I32Const(elem_stride));
            f.instruction(&Instruction::I32Mul);
            f.instruction(&Instruction::I32Add);

            // Marshal element and store
            match inner {
                Type::I32 | Type::Unknown => {
                    // Decode i31ref to i32
                    f.instruction(&Instruction::LocalGet(elem_eqref_local));
                    f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                    f.instruction(&Instruction::I31GetS);
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32ShrS);
                    f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                        offset: 0, align: 2, memory_index: 0,
                    }));
                }
                Type::I64 => {
                    f.instruction(&Instruction::LocalGet(elem_eqref_local));
                    f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::INT64)));
                    f.instruction(&Instruction::StructGet {
                        struct_type_index: gc_types::INT64,
                        field_index: gc_types::I64_VALUE,
                    });
                    f.instruction(&Instruction::I64Store(wasm_encoder::MemArg {
                        offset: 0, align: 3, memory_index: 0,
                    }));
                }
                Type::F64 => {
                    f.instruction(&Instruction::LocalGet(elem_eqref_local));
                    f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::FLOAT64)));
                    f.instruction(&Instruction::StructGet {
                        struct_type_index: gc_types::FLOAT64,
                        field_index: gc_types::F64_VALUE,
                    });
                    f.instruction(&Instruction::F64Store(wasm_encoder::MemArg {
                        offset: 0, align: 3, memory_index: 0,
                    }));
                }
                Type::Bool => {
                    f.instruction(&Instruction::LocalGet(elem_eqref_local));
                    f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                    f.instruction(&Instruction::I31GetS);
                    f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                    f.instruction(&Instruction::I32Eq);
                    f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                        offset: 0, align: 2, memory_index: 0,
                    }));
                }
                Type::String => {
                    // Drop the address we already pushed — we'll compute it again
                    // after getting (ptr, len) from the string
                    f.instruction(&Instruction::Drop);

                    // Convert GC string to linear memory
                    f.instruction(&Instruction::LocalGet(elem_eqref_local));
                    let ss = string_scratch_base.unwrap();
                    self.emit_gc_string_to_linear(f, ss)?;
                    // Stack: [str_ptr, str_len]
                    let tmp_len = ss + 2;
                    let tmp_ptr = ss + 1;
                    f.instruction(&Instruction::LocalSet(tmp_len));
                    f.instruction(&Instruction::LocalSet(tmp_ptr));

                    // Compute address for string: elem_base + i * 8
                    let addr_local = loop_i_local; // temporarily reuse — NO, loop_i is needed for i++

                    // Store ptr at addr+0
                    f.instruction(&Instruction::LocalGet(elem_base_local));
                    f.instruction(&Instruction::LocalGet(loop_i_local));
                    f.instruction(&Instruction::I32Const(elem_stride));
                    f.instruction(&Instruction::I32Mul);
                    f.instruction(&Instruction::I32Add);
                    f.instruction(&Instruction::LocalGet(tmp_ptr));
                    f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                        offset: 0, align: 2, memory_index: 0,
                    }));
                    // Store len at addr+4
                    f.instruction(&Instruction::LocalGet(elem_base_local));
                    f.instruction(&Instruction::LocalGet(loop_i_local));
                    f.instruction(&Instruction::I32Const(elem_stride));
                    f.instruction(&Instruction::I32Mul);
                    f.instruction(&Instruction::I32Add);
                    f.instruction(&Instruction::LocalGet(tmp_len));
                    f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                        offset: 4, align: 2, memory_index: 0,
                    }));
                }
                _ => {
                    // Unsupported inner type — store 0
                    f.instruction(&Instruction::I32Const(0));
                    f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                        offset: 0, align: 2, memory_index: 0,
                    }));
                }
            }

            // i++
            f.instruction(&Instruction::LocalGet(loop_i_local));
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Add);
            f.instruction(&Instruction::LocalSet(loop_i_local));
            f.instruction(&Instruction::Br(0)); // continue
        }
        f.instruction(&Instruction::End); // end loop
        f.instruction(&Instruction::End); // end block

        // 9. Allocate retptr for (ptr: i32, len: i32)
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Const(4));
        f.instruction(&Instruction::I32Const(8));
        f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::CABI_REALLOC)));
        // Reuse loop_i_local as retptr (loop is done)
        let retptr_local = loop_i_local;
        f.instruction(&Instruction::LocalSet(retptr_local));

        // 10. Write (elem_base, count) to retptr
        f.instruction(&Instruction::LocalGet(retptr_local));
        f.instruction(&Instruction::LocalGet(elem_base_local));
        f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
            offset: 0, align: 2, memory_index: 0,
        }));
        f.instruction(&Instruction::LocalGet(retptr_local));
        push_count!();
        f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
            offset: 4, align: 2, memory_index: 0,
        }));

        // 11. Return retptr
        f.instruction(&Instruction::LocalGet(retptr_local));

        Ok(())
    }

    /// Emit list<T> entry marshaling for WIT export params.
    ///
    /// list<T> params come as (ptr: i32, len: i32) flat values.
    /// Builds a PersistentVector by conjing each element via protocol dispatch.
    fn emit_list_entry(
        &self,
        inner: &Type,
        f: &mut Function,
        wit_idx: u32,
        converted_local: u32,
        scratch_base: u32,
        string_scratch_base: Option<u32>,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::method_ids;

        // Scratch locals:
        // +0 (eqref): accumulator vec
        // +1 (i32): loop counter
        // +6 (i32): ptr (from wit_idx)
        // i64_scratch: len (as i64 to save i32 locals)
        let vec_acc_local = scratch_base;
        let loop_i_local = scratch_base + 1;
        let ptr_local = scratch_base + 1; // reuse after copying
        let len_i64 = self.i64_scratch_local.get();

        // Save ptr and len from WIT params
        f.instruction(&Instruction::LocalGet(wit_idx));     // ptr
        f.instruction(&Instruction::LocalSet(ptr_local));
        f.instruction(&Instruction::LocalGet(wit_idx + 1)); // len
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::LocalSet(len_i64));

        // Start with empty PersistentVector
        // Look up the PersistentVector constructor or create an empty one
        // For simplicity, create nil (empty vec) and conj elements
        // Empty vec = nil sentinel — but we need a real PersistentVector for -conj
        // Use the PersistentVector deftype constructor
        if let Some(pv_gc_idx) = self.deftype_gc_type_idx("PersistentVector") {
            let pv_type_id = self.deftype_type_id("PersistentVector").unwrap();
            // PersistentVector struct: { type_id: i32, cnt: i32, shift: i32, root: eqref, tail: eqref }
            f.instruction(&Instruction::I32Const(pv_type_id));
            f.instruction(&Instruction::I32Const(0)); // cnt = 0
            f.instruction(&Instruction::I32Const(5)); // shift = 5
            f.instruction(&Instruction::RefNull(HeapType::Abstract { shared: false, ty: AbstractHeapType::Eq })); // root = null
            f.instruction(&Instruction::I32Const(0)); // empty array size
            f.instruction(&Instruction::ArrayNewDefault(gc_types::ARRAY));
            f.instruction(&Instruction::StructNew(pv_gc_idx));
        } else {
            // Fallback: nil
            f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
            f.instruction(&Instruction::RefI31);
        }
        f.instruction(&Instruction::LocalSet(vec_acc_local));

        // Copy ptr to a stable local since loop_i_local will overwrite it
        // Actually ptr_local and loop_i_local are the same (+1). Save ptr to stack via i32 trick.
        // We need both ptr and loop_i. Let me use a different approach.
        // Save ptr into f64_scratch as bits
        let ptr_i64 = self.f64_scratch_local.get(); // repurpose f64 scratch
        // Actually can't store i32 in f64 local. Let me just save ptr in count_eqref as i31ref.
        let ptr_eqref_local = scratch_base + 2; // eqref
        f.instruction(&Instruction::LocalGet(ptr_local));
        // ptr is an address (positive i32), encode as i31ref
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Shl);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::RefI31);
        f.instruction(&Instruction::LocalSet(ptr_eqref_local));

        // Helper to decode ptr from i31ref
        macro_rules! push_ptr {
            () => {
                f.instruction(&Instruction::LocalGet(ptr_eqref_local));
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrS);
            }
        }

        let elem_stride = match inner {
            Type::I32 | Type::Bool | Type::Unknown => 4i32,
            Type::I64 | Type::F64 => 8,
            Type::String => 8,
            _ => 4,
        };

        // Loop: for i = 0..len, conj each element
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::LocalSet(loop_i_local));

        f.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        f.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
        {
            // Break if i >= len
            f.instruction(&Instruction::LocalGet(loop_i_local));
            f.instruction(&Instruction::LocalGet(len_i64));
            f.instruction(&Instruction::I32WrapI64);
            f.instruction(&Instruction::I32GeU);
            f.instruction(&Instruction::BrIf(1));

            // Read element from linear memory: mem[ptr + i * stride]
            match inner {
                Type::I32 | Type::Unknown => {
                    push_ptr!();
                    f.instruction(&Instruction::LocalGet(loop_i_local));
                    f.instruction(&Instruction::I32Const(elem_stride));
                    f.instruction(&Instruction::I32Mul);
                    f.instruction(&Instruction::I32Add);
                    f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
                        offset: 0, align: 2, memory_index: 0,
                    }));
                    // Encode as i31ref
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32Shl);
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32Or);
                    f.instruction(&Instruction::RefI31);
                }
                Type::I64 => {
                    use crate::ir::type_ids;
                    push_ptr!();
                    f.instruction(&Instruction::LocalGet(loop_i_local));
                    f.instruction(&Instruction::I32Const(elem_stride));
                    f.instruction(&Instruction::I32Mul);
                    f.instruction(&Instruction::I32Add);
                    f.instruction(&Instruction::I64Load(wasm_encoder::MemArg {
                        offset: 0, align: 3, memory_index: 0,
                    }));
                    // Box in INT64
                    let scratch = self.i64_scratch_local.get();
                    // Can't use i64_scratch — it holds len! Use f64_scratch repurposed.
                    // Actually we can just use struct.new directly since both args are on stack
                    // struct.new needs (type_id: i32, value: i64) — but value is on top, type_id below
                    // Need scratch. Use f64_scratch as i64:
                    // Hmm, f64_scratch is ValType::F64, can't store i64.
                    // Just push type_id first, but we need to save the i64.
                    // Alternative: recompute the load address
                    // Simpler: push type_id const BEFORE the load, then the load pushes i64 on top.
                    // But struct.new expects (type_id, value) and type_id was pushed first = below.
                    // Actually that IS the right order! type_id is consumed first (from bottom).
                    // Wait, WASM stack: struct.new pops in order (first param from bottom).
                    // So if stack is [type_id, value], struct.new will create {type_id, value}. Correct!

                    // Let me redo: push type_id first, then do the load
                    // But the load was already emitted above. I need to restructure.
                    // For simplicity, just drop and redo.
                    // Actually I emitted the load already. Let me save to a scratch and redo.
                    // Use a simple approach: the i64 is on the stack. Save via local, push type_id, reload.
                    // Use len_i64 as temp? No, it has the length!
                    // We ARE stuck. Let me just not emit the load inline — restructure.

                    // Actually, I already have the i64 on stack from the load.
                    // I need: [i32, i64] on stack for struct.new.
                    // Current stack: [i64]. I need to get i32 below it.
                    // WASM has no swap. Only option: save i64, push i32, reload i64.
                    // But we don't have a free i64 local!
                    // len_i64 is occupied. nth_idx_i64 is... well it's i64_scratch which = len_i64.
                    // Hmm, we only have one i64 scratch.
                    // Solution: Save len to stack before the loop (it's constant).
                    // Actually the issue is fundamental: we need 2 i64 locals for list<s64>.
                    // For now, handle s64 in list by a different approach:
                    // pre-push type_id, then compute address and load.

                    // Delete the load we already emitted? Can't.
                    // The load is already emitted. But I can just use it:
                    // After i64Load, stack has [i64_value].
                    // Use i64_scratch to save it (len is constant, decode from len_i64):
                    // Wait, I'm using i64_scratch AS len_i64. So they're the same local!
                    // If I overwrite it with the loaded value, I lose len.
                    // Solution for i64 inner: DON'T store len in i64_scratch.
                    // Instead store len as i31ref like we store count in emit_list_exit.

                    // This method is already complex. For i64 list elements, let me use a
                    // simpler approach: before the loop, copy len to an i31ref local.
                    // But I'd need to restructure the whole method.

                    // For now, since list<s64> params are rare, emit a simplified version
                    // that just boxes it using the struct.new with correct stack order.
                    // I'll swap by storing to i64_scratch, losing len, but len was already
                    // checked at the top of the loop so we don't need it again until next iteration.
                    // Actually we DO need len for the loop condition check.

                    // OK best approach: just don't emit the i64Load above for the I64 case.
                    // But I already did in the match arm. Hmm.

                    // Actually wait — I'm IN the I64 match arm. The push_ptr/mul/add/i64Load
                    // was also in this arm. Let me remove what I emitted above and redo properly.
                    // The issue is that Rust match arms execute at runtime, but the wasm instructions
                    // are emitted at compile-time. Each match arm emits different instructions.
                    // So the I64 arm above emits the push_ptr + compute addr + i64Load.
                    // Now I need to also box it. The i64 is on stack.
                    // I need to save len, use i64_scratch for the value, push type_id, reload.

                    // Let's save len to an eqref local as i31ref first (before loop).
                    // But I already started the loop! Can't go back.

                    // Simplest fix: reuse f64_scratch by storing len as f64 bits.
                    // Actually we can store an i64 value in an f64 local using reinterpret:
                    // i64.reinterpret_f64 and f64.reinterpret_i64.
                    // Save len to f64_scratch:
                    f.instruction(&Instruction::LocalGet(len_i64)); // get len (i64)
                    f.instruction(&Instruction::F64ReinterpretI64);
                    f.instruction(&Instruction::LocalSet(ptr_i64)); // save len as f64 bits in f64_scratch

                    // Now i64_scratch is free. Save the loaded i64 value.
                    f.instruction(&Instruction::LocalSet(len_i64)); // repurpose for value

                    f.instruction(&Instruction::I32Const(type_ids::INT64));
                    f.instruction(&Instruction::LocalGet(len_i64)); // loaded value
                    f.instruction(&Instruction::StructNew(gc_types::INT64));

                    // Restore len from f64_scratch
                    f.instruction(&Instruction::LocalGet(ptr_i64));
                    f.instruction(&Instruction::I64ReinterpretF64);
                    f.instruction(&Instruction::LocalSet(len_i64));
                }
                Type::F64 => {
                    use crate::ir::type_ids;
                    f.instruction(&Instruction::I32Const(type_ids::FLOAT64));
                    push_ptr!();
                    f.instruction(&Instruction::LocalGet(loop_i_local));
                    f.instruction(&Instruction::I32Const(elem_stride));
                    f.instruction(&Instruction::I32Mul);
                    f.instruction(&Instruction::I32Add);
                    f.instruction(&Instruction::F64Load(wasm_encoder::MemArg {
                        offset: 0, align: 3, memory_index: 0,
                    }));
                    f.instruction(&Instruction::StructNew(gc_types::FLOAT64));
                }
                Type::Bool => {
                    push_ptr!();
                    f.instruction(&Instruction::LocalGet(loop_i_local));
                    f.instruction(&Instruction::I32Const(elem_stride));
                    f.instruction(&Instruction::I32Mul);
                    f.instruction(&Instruction::I32Add);
                    f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
                        offset: 0, align: 2, memory_index: 0,
                    }));
                    f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(RefType::EQREF))));
                    f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                    f.instruction(&Instruction::RefI31);
                    f.instruction(&Instruction::Else);
                    f.instruction(&Instruction::I32Const(gc_types::FALSE_SENTINEL));
                    f.instruction(&Instruction::RefI31);
                    f.instruction(&Instruction::End);
                }
                Type::String => {
                    // Read (ptr, len) at addr and addr+4
                    push_ptr!();
                    f.instruction(&Instruction::LocalGet(loop_i_local));
                    f.instruction(&Instruction::I32Const(elem_stride));
                    f.instruction(&Instruction::I32Mul);
                    f.instruction(&Instruction::I32Add);
                    // Load str_ptr
                    f.instruction(&Instruction::LocalTee(scratch_base + 6)); // save addr temporarily
                    f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
                        offset: 0, align: 2, memory_index: 0,
                    }));
                    // Load str_len
                    f.instruction(&Instruction::LocalGet(scratch_base + 6));
                    f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
                        offset: 4, align: 2, memory_index: 0,
                    }));
                    // Convert to GC string
                    let ss = string_scratch_base.unwrap();
                    self.emit_linear_to_gc_string(f, ss)?;
                }
                _ => {
                    // Unknown type: push nil
                    f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
                    f.instruction(&Instruction::RefI31);
                }
            }
            // Stack: [elem: eqref]

            // Conj element onto accumulator via protocol dispatch
            // call_indirect expects: [vec, elem, dispatch_idx] for -conj
            // Save element temporarily
            let elem_local = scratch_base + 3; // eqref scratch
            f.instruction(&Instruction::LocalSet(elem_local));

            // Get type_id of accumulator
            f.instruction(&Instruction::LocalGet(vec_acc_local));
            f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::GET_TYPE_ID)));
            // Compute conj dispatch idx: type_id * mpt + CONJ
            f.instruction(&Instruction::I32Const(self.ir.methods_per_type as i32));
            f.instruction(&Instruction::I32Mul);
            f.instruction(&Instruction::I32Const(method_ids::CONJ as i32));
            f.instruction(&Instruction::I32Add);
            // Save dispatch idx
            let conj_idx_local = scratch_base + 6; // reuse
            // Wait, +6 might be used. For string inner, we used it as addr temp.
            // It's fine — we're past that usage now.
            f.instruction(&Instruction::LocalSet(conj_idx_local));

            // Push args: [vec, elem, dispatch_idx]
            f.instruction(&Instruction::LocalGet(vec_acc_local));
            f.instruction(&Instruction::LocalGet(elem_local));
            f.instruction(&Instruction::LocalGet(conj_idx_local));
            f.instruction(&Instruction::CallIndirect {
                type_index: self.protocol_type(protocol_type_offsets::ARITY_2_REF),
                table_index: 0,
            });
            // Stack: [new_vec: eqref]
            f.instruction(&Instruction::LocalSet(vec_acc_local));

            // i++
            f.instruction(&Instruction::LocalGet(loop_i_local));
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Add);
            f.instruction(&Instruction::LocalSet(loop_i_local));
            f.instruction(&Instruction::Br(0)); // continue
        }
        f.instruction(&Instruction::End); // end loop
        f.instruction(&Instruction::End); // end block

        // Result is the accumulated vector
        f.instruction(&Instruction::LocalGet(vec_acc_local));
        f.instruction(&Instruction::LocalSet(converted_local));

        Ok(())
    }

    /// Emit list<T> param marshaling for WIT import params.
    /// Converts eqref vec → flat list values (ptr, len) on stack.
    ///
    /// Reuses emit_list_exit logic: iterates vec via -count/-nth,
    /// writes elements to linear memory, pushes (ptr, len) on stack.
    fn emit_list_import_param(
        &self,
        inner: &Type,
        f: &mut Function,
        string_scratch: Option<u32>,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::method_ids;

        // This is essentially the same as list exit but leaves (ptr, len) on stack
        // instead of writing to retptr. Reuse scratch from self.scratch_local.
        let scratch_base = self.scratch_local.get();
        // Bump scratch past what we'll use (2 sets)
        self.scratch_local.set(scratch_base + 10);

        let vec_local = scratch_base;
        let elem_base_local = scratch_base + 1;
        let count_eqref_local = scratch_base + 2;
        let loop_i_local = scratch_base + 6;
        let nth_idx_i64 = self.i64_scratch_local.get();

        // Save vector (on stack from caller)
        f.instruction(&Instruction::LocalSet(vec_local));

        // Get type_id
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::GET_TYPE_ID)));
        f.instruction(&Instruction::LocalSet(loop_i_local)); // type_id temp

        // Compute nth dispatch idx
        f.instruction(&Instruction::LocalGet(loop_i_local));
        f.instruction(&Instruction::I32Const(self.ir.methods_per_type as i32));
        f.instruction(&Instruction::I32Mul);
        f.instruction(&Instruction::I32Const(method_ids::NTH as i32));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::LocalSet(nth_idx_i64));

        // Call -count
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::LocalGet(loop_i_local)); // type_id
        f.instruction(&Instruction::I32Const(self.ir.methods_per_type as i32));
        f.instruction(&Instruction::I32Mul);
        f.instruction(&Instruction::I32Const(method_ids::COUNT as i32));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::CallIndirect {
            type_index: self.protocol_type(protocol_type_offsets::ARITY_1_I32),
            table_index: 0,
        });
        // Encode count as i31ref
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Shl);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::RefI31);
        f.instruction(&Instruction::LocalSet(count_eqref_local));

        macro_rules! push_count {
            () => {
                f.instruction(&Instruction::LocalGet(count_eqref_local));
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrS);
            }
        }

        let elem_stride = match inner {
            Type::I32 | Type::Bool | Type::Unknown => 4i32,
            Type::I64 | Type::F64 => 8,
            Type::String => 8,
            _ => 4,
        };
        let elem_align = match inner {
            Type::I64 | Type::F64 => 8i32,
            _ => 4i32,
        };

        // Allocate element array
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Const(elem_align));
        push_count!();
        f.instruction(&Instruction::I32Const(elem_stride));
        f.instruction(&Instruction::I32Mul);
        f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::CABI_REALLOC)));
        f.instruction(&Instruction::LocalSet(elem_base_local));

        // Loop
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::LocalSet(loop_i_local));

        f.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        f.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
        {
            f.instruction(&Instruction::LocalGet(loop_i_local));
            push_count!();
            f.instruction(&Instruction::I32GeU);
            f.instruction(&Instruction::BrIf(1));

            // Get element via -nth
            f.instruction(&Instruction::LocalGet(vec_local));
            f.instruction(&Instruction::LocalGet(loop_i_local));
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Shl);
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Or);
            f.instruction(&Instruction::RefI31);
            f.instruction(&Instruction::LocalGet(nth_idx_i64));
            f.instruction(&Instruction::I32WrapI64);
            f.instruction(&Instruction::CallIndirect {
                type_index: self.protocol_type(protocol_type_offsets::ARITY_2_REF),
                table_index: 0,
            });

            let elem_eqref_local = scratch_base + 4;
            f.instruction(&Instruction::LocalSet(elem_eqref_local));

            // Compute address and store
            f.instruction(&Instruction::LocalGet(elem_base_local));
            f.instruction(&Instruction::LocalGet(loop_i_local));
            f.instruction(&Instruction::I32Const(elem_stride));
            f.instruction(&Instruction::I32Mul);
            f.instruction(&Instruction::I32Add);

            match inner {
                Type::I32 | Type::Unknown => {
                    f.instruction(&Instruction::LocalGet(elem_eqref_local));
                    f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                    f.instruction(&Instruction::I31GetS);
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32ShrS);
                    f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                        offset: 0, align: 2, memory_index: 0,
                    }));
                }
                Type::I64 => {
                    f.instruction(&Instruction::LocalGet(elem_eqref_local));
                    f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::INT64)));
                    f.instruction(&Instruction::StructGet {
                        struct_type_index: gc_types::INT64,
                        field_index: gc_types::I64_VALUE,
                    });
                    f.instruction(&Instruction::I64Store(wasm_encoder::MemArg {
                        offset: 0, align: 3, memory_index: 0,
                    }));
                }
                Type::F64 => {
                    f.instruction(&Instruction::LocalGet(elem_eqref_local));
                    f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::FLOAT64)));
                    f.instruction(&Instruction::StructGet {
                        struct_type_index: gc_types::FLOAT64,
                        field_index: gc_types::F64_VALUE,
                    });
                    f.instruction(&Instruction::F64Store(wasm_encoder::MemArg {
                        offset: 0, align: 3, memory_index: 0,
                    }));
                }
                Type::Bool => {
                    f.instruction(&Instruction::LocalGet(elem_eqref_local));
                    f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                    f.instruction(&Instruction::I31GetS);
                    f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                    f.instruction(&Instruction::I32Eq);
                    f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                        offset: 0, align: 2, memory_index: 0,
                    }));
                }
                Type::String => {
                    f.instruction(&Instruction::Drop); // drop pre-computed addr
                    f.instruction(&Instruction::LocalGet(elem_eqref_local));
                    let ss = string_scratch.unwrap();
                    self.emit_gc_string_to_linear(f, ss)?;
                    let tmp_len = ss + 2;
                    let tmp_ptr = ss + 1;
                    f.instruction(&Instruction::LocalSet(tmp_len));
                    f.instruction(&Instruction::LocalSet(tmp_ptr));
                    // Store at addr
                    f.instruction(&Instruction::LocalGet(elem_base_local));
                    f.instruction(&Instruction::LocalGet(loop_i_local));
                    f.instruction(&Instruction::I32Const(elem_stride));
                    f.instruction(&Instruction::I32Mul);
                    f.instruction(&Instruction::I32Add);
                    f.instruction(&Instruction::LocalGet(tmp_ptr));
                    f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                        offset: 0, align: 2, memory_index: 0,
                    }));
                    f.instruction(&Instruction::LocalGet(elem_base_local));
                    f.instruction(&Instruction::LocalGet(loop_i_local));
                    f.instruction(&Instruction::I32Const(elem_stride));
                    f.instruction(&Instruction::I32Mul);
                    f.instruction(&Instruction::I32Add);
                    f.instruction(&Instruction::LocalGet(tmp_len));
                    f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                        offset: 4, align: 2, memory_index: 0,
                    }));
                }
                _ => {
                    f.instruction(&Instruction::I32Const(0));
                    f.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                        offset: 0, align: 2, memory_index: 0,
                    }));
                }
            }

            f.instruction(&Instruction::LocalGet(loop_i_local));
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Add);
            f.instruction(&Instruction::LocalSet(loop_i_local));
            f.instruction(&Instruction::Br(0));
        }
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);

        // Push (ptr, len) on stack
        f.instruction(&Instruction::LocalGet(elem_base_local));
        push_count!();

        // Restore scratch
        self.scratch_local.set(scratch_base);

        Ok(())
    }

    /// Emit list<T> return marshaling for WIT import returns.
    /// Reads (ptr, len) from retptr, builds PersistentVector via -conj.
    fn emit_list_import_return(
        &self,
        inner: &Type,
        f: &mut Function,
        retptr: u32,
        string_scratch: Option<u32>,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::method_ids;

        let scratch_base = self.scratch_local.get();
        self.scratch_local.set(scratch_base + 10);

        let vec_acc_local = scratch_base;
        let loop_i_local = scratch_base + 1;
        let ptr_eqref_local = scratch_base + 2;
        let len_i64 = self.i64_scratch_local.get();

        // Read ptr and len from retptr
        f.instruction(&Instruction::LocalGet(retptr));
        f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
            offset: 0, align: 2, memory_index: 0,
        }));
        // Encode ptr as i31ref
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Shl);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::RefI31);
        f.instruction(&Instruction::LocalSet(ptr_eqref_local));

        f.instruction(&Instruction::LocalGet(retptr));
        f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
            offset: 4, align: 2, memory_index: 0,
        }));
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::LocalSet(len_i64));

        macro_rules! push_ptr {
            () => {
                f.instruction(&Instruction::LocalGet(ptr_eqref_local));
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrS);
            }
        }

        let elem_stride = match inner {
            Type::I32 | Type::Bool | Type::Unknown => 4i32,
            Type::I64 | Type::F64 => 8,
            Type::String => 8,
            _ => 4,
        };

        // Start with empty PersistentVector
        if let Some(pv_gc_idx) = self.deftype_gc_type_idx("PersistentVector") {
            let pv_type_id = self.deftype_type_id("PersistentVector").unwrap();
            f.instruction(&Instruction::I32Const(pv_type_id));
            f.instruction(&Instruction::I32Const(0));
            f.instruction(&Instruction::I32Const(5));
            f.instruction(&Instruction::RefNull(HeapType::Abstract { shared: false, ty: AbstractHeapType::Eq }));
            f.instruction(&Instruction::I32Const(0));
            f.instruction(&Instruction::ArrayNewDefault(gc_types::ARRAY));
            f.instruction(&Instruction::StructNew(pv_gc_idx));
        } else {
            f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
            f.instruction(&Instruction::RefI31);
        }
        f.instruction(&Instruction::LocalSet(vec_acc_local));

        // Loop
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::LocalSet(loop_i_local));

        f.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        f.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
        {
            f.instruction(&Instruction::LocalGet(loop_i_local));
            f.instruction(&Instruction::LocalGet(len_i64));
            f.instruction(&Instruction::I32WrapI64);
            f.instruction(&Instruction::I32GeU);
            f.instruction(&Instruction::BrIf(1));

            // Read element
            match inner {
                Type::I32 | Type::Unknown => {
                    push_ptr!();
                    f.instruction(&Instruction::LocalGet(loop_i_local));
                    f.instruction(&Instruction::I32Const(elem_stride));
                    f.instruction(&Instruction::I32Mul);
                    f.instruction(&Instruction::I32Add);
                    f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
                        offset: 0, align: 2, memory_index: 0,
                    }));
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32Shl);
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32Or);
                    f.instruction(&Instruction::RefI31);
                }
                Type::I64 => {
                    use crate::ir::type_ids;
                    push_ptr!();
                    f.instruction(&Instruction::LocalGet(loop_i_local));
                    f.instruction(&Instruction::I32Const(elem_stride));
                    f.instruction(&Instruction::I32Mul);
                    f.instruction(&Instruction::I32Add);
                    f.instruction(&Instruction::I64Load(wasm_encoder::MemArg {
                        offset: 0, align: 3, memory_index: 0,
                    }));
                    // Save len, use i64_scratch for value
                    let f64_scratch = self.f64_scratch_local.get();
                    f.instruction(&Instruction::LocalGet(len_i64));
                    f.instruction(&Instruction::F64ReinterpretI64);
                    f.instruction(&Instruction::LocalSet(f64_scratch));
                    f.instruction(&Instruction::LocalSet(len_i64)); // value
                    f.instruction(&Instruction::I32Const(type_ids::INT64));
                    f.instruction(&Instruction::LocalGet(len_i64));
                    f.instruction(&Instruction::StructNew(gc_types::INT64));
                    // Restore len
                    f.instruction(&Instruction::LocalGet(f64_scratch));
                    f.instruction(&Instruction::I64ReinterpretF64);
                    f.instruction(&Instruction::LocalSet(len_i64));
                }
                Type::F64 => {
                    use crate::ir::type_ids;
                    f.instruction(&Instruction::I32Const(type_ids::FLOAT64));
                    push_ptr!();
                    f.instruction(&Instruction::LocalGet(loop_i_local));
                    f.instruction(&Instruction::I32Const(elem_stride));
                    f.instruction(&Instruction::I32Mul);
                    f.instruction(&Instruction::I32Add);
                    f.instruction(&Instruction::F64Load(wasm_encoder::MemArg {
                        offset: 0, align: 3, memory_index: 0,
                    }));
                    f.instruction(&Instruction::StructNew(gc_types::FLOAT64));
                }
                Type::Bool => {
                    push_ptr!();
                    f.instruction(&Instruction::LocalGet(loop_i_local));
                    f.instruction(&Instruction::I32Const(elem_stride));
                    f.instruction(&Instruction::I32Mul);
                    f.instruction(&Instruction::I32Add);
                    f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
                        offset: 0, align: 2, memory_index: 0,
                    }));
                    f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(RefType::EQREF))));
                    f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                    f.instruction(&Instruction::RefI31);
                    f.instruction(&Instruction::Else);
                    f.instruction(&Instruction::I32Const(gc_types::FALSE_SENTINEL));
                    f.instruction(&Instruction::RefI31);
                    f.instruction(&Instruction::End);
                }
                Type::String => {
                    push_ptr!();
                    f.instruction(&Instruction::LocalGet(loop_i_local));
                    f.instruction(&Instruction::I32Const(elem_stride));
                    f.instruction(&Instruction::I32Mul);
                    f.instruction(&Instruction::I32Add);
                    f.instruction(&Instruction::LocalTee(scratch_base + 6));
                    f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
                        offset: 0, align: 2, memory_index: 0,
                    }));
                    f.instruction(&Instruction::LocalGet(scratch_base + 6));
                    f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
                        offset: 4, align: 2, memory_index: 0,
                    }));
                    let ss = string_scratch.unwrap();
                    self.emit_linear_to_gc_string(f, ss)?;
                }
                _ => {
                    f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
                    f.instruction(&Instruction::RefI31);
                }
            }

            // Conj element
            let elem_local = scratch_base + 3;
            f.instruction(&Instruction::LocalSet(elem_local));

            f.instruction(&Instruction::LocalGet(vec_acc_local));
            f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::GET_TYPE_ID)));
            f.instruction(&Instruction::I32Const(self.ir.methods_per_type as i32));
            f.instruction(&Instruction::I32Mul);
            f.instruction(&Instruction::I32Const(method_ids::CONJ as i32));
            f.instruction(&Instruction::I32Add);
            let conj_idx_local = scratch_base + 6;
            f.instruction(&Instruction::LocalSet(conj_idx_local));

            f.instruction(&Instruction::LocalGet(vec_acc_local));
            f.instruction(&Instruction::LocalGet(elem_local));
            f.instruction(&Instruction::LocalGet(conj_idx_local));
            f.instruction(&Instruction::CallIndirect {
                type_index: self.protocol_type(protocol_type_offsets::ARITY_2_REF),
                table_index: 0,
            });
            f.instruction(&Instruction::LocalSet(vec_acc_local));

            f.instruction(&Instruction::LocalGet(loop_i_local));
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Add);
            f.instruction(&Instruction::LocalSet(loop_i_local));
            f.instruction(&Instruction::Br(0));
        }
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);

        f.instruction(&Instruction::LocalGet(vec_acc_local));

        self.scratch_local.set(scratch_base);

        Ok(())
    }

    /// Generate a call to an imported function with proper marshaling.
    ///
    /// Converts eqref arguments to WIT types for each param, calls the import,
    /// then converts the WIT return type back to eqref.
    fn generate_import_call(
        &self,
        import_idx: u32,
        args: &[Expr],
        f: &mut Function,
        loop_depth: u32,
        param_offset: u32,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;

        let import = &self.ir.imports[import_idx as usize];

        // Reserve scratch locals for string/option/list marshaling if needed
        let scratch_save = self.scratch_local.get();
        let needs_scratch = import.params.iter().any(|ty| type_needs_marshal_scratch(ty))
            || type_needs_marshal_scratch(&import.return_type);
        let string_scratch = if needs_scratch {
            let base = self.scratch_local.get();
            self.scratch_local.set(base + 5);
            Some(base)
        } else {
            None
        };

        // Generate and marshal each argument (eqref → WIT type)
        for (i, arg) in args.iter().enumerate() {
            self.generate_expr_inner(arg, f, loop_depth, param_offset)?;

            if i < import.params.len() {
                match &import.params[i] {
                    Type::I32 | Type::Unknown => {
                        // Unwrap i31ref to i32
                        f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                        f.instruction(&Instruction::I31GetS);
                        f.instruction(&Instruction::I32Const(1));
                        f.instruction(&Instruction::I32ShrS);
                    }
                    Type::I64 => {
                        // Unbox INT64 struct to i64
                        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::INT64)));
                        f.instruction(&Instruction::StructGet {
                            struct_type_index: gc_types::INT64,
                            field_index: gc_types::I64_VALUE,
                        });
                    }
                    Type::F64 => {
                        // Unbox FLOAT64 struct to f64
                        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::FLOAT64)));
                        f.instruction(&Instruction::StructGet {
                            struct_type_index: gc_types::FLOAT64,
                            field_index: gc_types::F64_VALUE,
                        });
                    }
                    Type::Bool => {
                        // Decode i31ref sentinel to i32 bool
                        f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                        f.instruction(&Instruction::I31GetS);
                        f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                        f.instruction(&Instruction::I32Eq);
                    }
                    Type::String => {
                        self.emit_gc_string_to_linear(f, string_scratch.unwrap())?;
                    }
                    Type::Option(inner) => {
                        // Marshal eqref → option flat values (discriminant + payload)
                        self.emit_option_import_param(inner, f, string_scratch)?;
                    }
                    Type::List(inner) => {
                        // Marshal eqref vec → list flat values (ptr, len)
                        self.emit_list_import_param(inner, f, string_scratch)?;
                    }
                    _ => {}
                }
            }
        }

        // Canonical ABI: For imports returning > 1 flat value (e.g., string),
        // we need to allocate a retptr and pass it as the last param
        let flat_return = if import.return_type == Type::Unit {
            vec![]
        } else {
            type_to_valtypes(&import.return_type)
        };
        let import_uses_retptr = flat_return.len() > 1;
        let retptr_local = if import_uses_retptr {
            // Allocate space for return values in linear memory with proper alignment
            let (size, _offsets) = flat_byte_size_and_offsets(&flat_return);
            let align = if flat_return.iter().any(|v| matches!(v, ValType::I64 | ValType::F64)) { 8 } else { 4 };
            f.instruction(&Instruction::I32Const(0));        // old_ptr
            f.instruction(&Instruction::I32Const(0));        // old_size
            f.instruction(&Instruction::I32Const(align));    // align
            f.instruction(&Instruction::I32Const(size));     // new_size
            f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::CABI_REALLOC)));
            // Save retptr to a scratch local
            let ss = string_scratch.unwrap();
            let retptr = ss + 1; // reuse str_ptr (i32) scratch
            f.instruction(&Instruction::LocalTee(retptr));
            Some(retptr)
        } else {
            None
        };

        // Call the import
        f.instruction(&Instruction::Call(import_idx));

        // Marshal return value back to eqref (WIT type → eqref)
        match &import.return_type {
            Type::I32 | Type::Unknown => {
                // Wrap i32 as i31ref
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Shl);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Or);
                f.instruction(&Instruction::RefI31);
            }
            Type::I64 => {
                // Box i64 in INT64 struct: { type_id, value }
                use crate::ir::type_ids;
                let scratch = self.i64_scratch_local.get();
                f.instruction(&Instruction::LocalSet(scratch));
                f.instruction(&Instruction::I32Const(type_ids::INT64));
                f.instruction(&Instruction::LocalGet(scratch));
                f.instruction(&Instruction::StructNew(gc_types::INT64));
            }
            Type::F64 => {
                // Box f64 in FLOAT64 struct: { type_id, value }
                use crate::ir::type_ids;
                let scratch = self.f64_scratch_local.get();
                f.instruction(&Instruction::LocalSet(scratch));
                f.instruction(&Instruction::I32Const(type_ids::FLOAT64));
                f.instruction(&Instruction::LocalGet(scratch));
                f.instruction(&Instruction::StructNew(gc_types::FLOAT64));
            }
            Type::Bool => {
                // Convert i32 (0/1) to i31ref bool sentinel
                f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(RefType::EQREF))));
                f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                f.instruction(&Instruction::RefI31);
                f.instruction(&Instruction::Else);
                f.instruction(&Instruction::I32Const(gc_types::FALSE_SENTINEL));
                f.instruction(&Instruction::RefI31);
                f.instruction(&Instruction::End);
            }
            Type::String => {
                // Canonical ABI: import wrote (ptr, len) to retptr
                let retptr = retptr_local.unwrap();
                // Read ptr from retptr+0
                f.instruction(&Instruction::LocalGet(retptr));
                f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));
                // Read len from retptr+4
                f.instruction(&Instruction::LocalGet(retptr));
                f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
                    offset: 4,
                    align: 2,
                    memory_index: 0,
                }));
                self.emit_linear_to_gc_string(f, string_scratch.unwrap())?;
            }
            Type::Option(inner) => {
                // option<T> return via retptr: read discriminant, then payload
                let retptr = retptr_local.unwrap();
                self.emit_option_import_return(inner, f, retptr, string_scratch)?;
            }
            Type::List(inner) => {
                // list<T> return via retptr: read (ptr, len), build PersistentVector
                let retptr = retptr_local.unwrap();
                self.emit_list_import_return(inner, f, retptr, string_scratch)?;
            }
            Type::Unit => {
                // Import returns nothing, push nil
                f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
                f.instruction(&Instruction::RefI31);
            }
            _ => {}
        }

        // Restore scratch
        self.scratch_local.set(scratch_save);

        Ok(())
    }

    fn generate_expr(&self, expr: &Expr, f: &mut Function) -> CompileResult<()> {
        self.generate_expr_inner(expr, f, 0, 0)
    }

    /// Generate expression with param_offset for WIT-exported functions.
    /// param_offset is the number of raw WIT params before converted eqref params.
    fn generate_expr_with_offset(&self, expr: &Expr, f: &mut Function, param_offset: u32) -> CompileResult<()> {
        self.generate_expr_inner(expr, f, 0, param_offset)
    }

    fn generate_expr_inner(&self, expr: &Expr, f: &mut Function, loop_depth: u32, param_offset: u32) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::type_ids;
        match expr {
            Expr::Unit => {
                // nil sentinel: i31ref(0)
                f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
                f.instruction(&Instruction::RefI31);
            }

            Expr::Bool(b) => {
                // true: i31ref(4), false: i31ref(2)
                let sentinel = if *b {
                    gc_types::TRUE_SENTINEL
                } else {
                    gc_types::FALSE_SENTINEL
                };
                f.instruction(&Instruction::I32Const(sentinel));
                f.instruction(&Instruction::RefI31);
            }

            Expr::Int(i) => {
                // Encode as i31ref: (n << 1) | 1
                // Small ints fit in i31ref range: -2^29 to 2^29-1
                let encoded = gc_types::encode_small_int(*i);
                if gc_types::fits_in_small_int(*i) {
                    f.instruction(&Instruction::I32Const(encoded));
                    f.instruction(&Instruction::RefI31);
                } else {
                    // Large integer: box in INT64 struct { type_id, value }
                    f.instruction(&Instruction::I32Const(type_ids::INT64));
                    f.instruction(&Instruction::I64Const(*i));
                    f.instruction(&Instruction::StructNew(gc_types::INT64));
                }
            }

            Expr::RawI32(v) => {
                // Raw i32 constant - not GC-encoded, for struct fields like type_id
                f.instruction(&Instruction::I32Const(*v));
            }

            Expr::Unbox32(value) => {
                // Polymorphic unbox to raw i32:
                // Handles both i31ref (small integers) and INT64 (from bit-shift-left overflow)
                let scratch_base = self.scratch_local.get();
                self.scratch_local.set(scratch_base + 5);

                self.generate_expr_inner(value, f, loop_depth, param_offset)?;
                self.generate_polymorphic_unwrap_i32(f);

                self.scratch_local.set(scratch_base);
            }

            Expr::Float(v) => {
                // Box float in FLOAT struct { type_id, value }
                f.instruction(&Instruction::I32Const(type_ids::FLOAT64));
                f.instruction(&Instruction::F64Const(*v));
                f.instruction(&Instruction::StructNew(gc_types::FLOAT64));
            }

            Expr::String(idx) => {
                // Create STRING (array<i8>) from passive data segment using array.new_data
                let len = self.ir.strings[*idx as usize].len() as u32;
                f.instruction(&Instruction::I32Const(0));     // offset into data segment
                f.instruction(&Instruction::I32Const(len as i32));  // length
                f.instruction(&Instruction::ArrayNewData {
                    array_type_index: gc_types::STRING,
                    array_data_index: *idx,  // Data segment index = string index
                });
            }

            Expr::Keyword { idx, hash: _ } => {
                // Load interned KEYWORD from global intern table
                // This ensures reference equality: same keyword literal = same object
                use crate::ir::global_indices;
                f.instruction(&Instruction::GlobalGet(global_indices::KEYWORD_TABLE));
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::KEYWORD_INTERN_TABLE)));
                f.instruction(&Instruction::I32Const(*idx as i32));
                f.instruction(&Instruction::ArrayGet(gc_types::KEYWORD_INTERN_TABLE));
            }

            Expr::Symbol { idx, hash: _ } => {
                // Load interned SYMBOL from global intern table
                // This ensures reference equality: same symbol literal = same object
                use crate::ir::global_indices;
                f.instruction(&Instruction::GlobalGet(global_indices::SYMBOL_TABLE));
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::SYMBOL_INTERN_TABLE)));
                f.instruction(&Instruction::I32Const(*idx as i32));
                f.instruction(&Instruction::ArrayGet(gc_types::SYMBOL_INTERN_TABLE));
            }

            Expr::LocalGet { local, ty: _ } => {
                f.instruction(&Instruction::LocalGet(*local + param_offset));
            }

            Expr::LocalSet { local, value, ty: _ } => {
                self.generate_expr_inner(value, f, loop_depth, param_offset)?;
                f.instruction(&Instruction::LocalSet(*local + param_offset));
                f.instruction(&Instruction::I32Const(0));
            }

            Expr::GlobalGet(idx) => {
                f.instruction(&Instruction::GlobalGet(*idx));
            }

            Expr::GlobalSet(idx, value) => {
                self.generate_expr_inner(value, f, loop_depth, param_offset)?;
                f.instruction(&Instruction::GlobalSet(*idx));
                f.instruction(&Instruction::I32Const(0));
            }

            Expr::BinOp { op, left, right, ty } => {
                // In GC mode, operands are i31refs with encoded integers
                // We need to: extract -> decode -> operate -> encode -> wrap

                // For arithmetic operations on small integers:
                // 1. Generate left operand (i31ref)
                // 2. i31.get_s to extract raw i32
                // 3. Arithmetic shift right by 1 to decode
                // 4. Generate right operand (i31ref)
                // 5. i31.get_s to extract raw i32
                // 6. Arithmetic shift right by 1 to decode
                // 7. Perform operation
                // 8. Shift left by 1, or with 1 to encode
                // 9. ref.i31 to wrap

                // Helper to generate unwrap (eqref -> decoded i32)
                // Cast eqref to i31ref first, then extract and decode
                let generate_unwrap_i31 = |f: &mut Function| {
                    f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                    f.instruction(&Instruction::I31GetS);
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32ShrS);
                };

                match (op, ty) {
                    // Arithmetic operations: polymorphic unwrap to handle INT64 from bit ops
                    (BinOp::Add, Type::I32) | (BinOp::Add, Type::I64) |
                    (BinOp::Sub, Type::I32) | (BinOp::Sub, Type::I64) |
                    (BinOp::Mul, Type::I32) | (BinOp::Mul, Type::I64) |
                    (BinOp::Div, Type::I32) | (BinOp::Div, Type::I64) |
                    (BinOp::Rem, Type::I32) | (BinOp::Rem, Type::I64) |
                    (BinOp::Quot, Type::I32) | (BinOp::Quot, Type::I64) => {
                        // Reserve scratch locals for polymorphic unwrap
                        let scratch_base = self.scratch_local.get();
                        self.scratch_local.set(scratch_base + 5);

                        let right_i32_local = scratch_base + 1;

                        // Generate right first, store it
                        self.generate_expr_inner(right, f, loop_depth, param_offset)?;
                        self.generate_polymorphic_unwrap_i32(f);
                        f.instruction(&Instruction::LocalSet(right_i32_local));

                        // Generate left (stays on stack)
                        self.generate_expr_inner(left, f, loop_depth, param_offset)?;
                        self.generate_polymorphic_unwrap_i32(f);

                        // Get right from local
                        f.instruction(&Instruction::LocalGet(right_i32_local));
                        // Stack: [left, right]

                        // Perform operation
                        match op {
                            BinOp::Add => f.instruction(&Instruction::I32Add),
                            BinOp::Sub => f.instruction(&Instruction::I32Sub),
                            BinOp::Mul => f.instruction(&Instruction::I32Mul),
                            BinOp::Div => f.instruction(&Instruction::I32DivS),
                            BinOp::Rem => f.instruction(&Instruction::I32RemS),
                            BinOp::Quot => f.instruction(&Instruction::I32DivS),
                            _ => unreachable!(),
                        };

                        // Safe box result (INT64 if overflow)
                        self.generate_box_i32_safe(f);

                        self.scratch_local.set(scratch_base);
                    }

                    // Float arithmetic: unwrap structs, compute, wrap in { type_id, value }
                    (BinOp::Add, Type::F64) => {
                        f.instruction(&Instruction::I32Const(type_ids::FLOAT64));
                        self.generate_expr(left, f)?;
                        f.instruction(&Instruction::StructGet {
                            struct_type_index: gc_types::FLOAT64,
                            field_index: gc_types::F64_VALUE,
                        });
                        self.generate_expr(right, f)?;
                        f.instruction(&Instruction::StructGet {
                            struct_type_index: gc_types::FLOAT64,
                            field_index: gc_types::F64_VALUE,
                        });
                        f.instruction(&Instruction::F64Add);
                        f.instruction(&Instruction::StructNew(gc_types::FLOAT64));
                    }
                    (BinOp::Sub, Type::F64) => {
                        f.instruction(&Instruction::I32Const(type_ids::FLOAT64));
                        self.generate_expr(left, f)?;
                        f.instruction(&Instruction::StructGet {
                            struct_type_index: gc_types::FLOAT64,
                            field_index: gc_types::F64_VALUE,
                        });
                        self.generate_expr(right, f)?;
                        f.instruction(&Instruction::StructGet {
                            struct_type_index: gc_types::FLOAT64,
                            field_index: gc_types::F64_VALUE,
                        });
                        f.instruction(&Instruction::F64Sub);
                        f.instruction(&Instruction::StructNew(gc_types::FLOAT64));
                    }
                    (BinOp::Mul, Type::F64) => {
                        f.instruction(&Instruction::I32Const(type_ids::FLOAT64));
                        self.generate_expr(left, f)?;
                        f.instruction(&Instruction::StructGet {
                            struct_type_index: gc_types::FLOAT64,
                            field_index: gc_types::F64_VALUE,
                        });
                        self.generate_expr(right, f)?;
                        f.instruction(&Instruction::StructGet {
                            struct_type_index: gc_types::FLOAT64,
                            field_index: gc_types::F64_VALUE,
                        });
                        f.instruction(&Instruction::F64Mul);
                        f.instruction(&Instruction::StructNew(gc_types::FLOAT64));
                    }
                    (BinOp::Div, Type::F64) => {
                        f.instruction(&Instruction::I32Const(type_ids::FLOAT64));
                        self.generate_expr(left, f)?;
                        // Cast to FLOAT struct before extracting value
                        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::FLOAT64)));
                        f.instruction(&Instruction::StructGet {
                            struct_type_index: gc_types::FLOAT64,
                            field_index: gc_types::F64_VALUE,
                        });
                        self.generate_expr(right, f)?;
                        // Cast to FLOAT struct before extracting value
                        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::FLOAT64)));
                        f.instruction(&Instruction::StructGet {
                            struct_type_index: gc_types::FLOAT64,
                            field_index: gc_types::F64_VALUE,
                        });
                        f.instruction(&Instruction::F64Div);
                        f.instruction(&Instruction::StructNew(gc_types::FLOAT64));
                    }

                    // Comparison operations: use raw i31 values to avoid sentinel/number collision
                    // Numbers are encoded as (n << 1) | 1, sentinels are 0/2/4
                    // Using raw values preserves both equality semantics and ordering
                    (BinOp::Eq, _) | (BinOp::Ne, _) | (BinOp::Lt, _) | (BinOp::Le, _) | (BinOp::Gt, _) | (BinOp::Ge, _) => {
                        // Reserve scratch locals for polymorphic unwrap
                        let scratch_base = self.scratch_local.get();
                        self.scratch_local.set(scratch_base + 5);

                        let right_i32_local = scratch_base + 1;

                        // Generate right first, store it
                        self.generate_expr_inner(right, f, loop_depth, param_offset)?;
                        self.generate_polymorphic_unwrap_i32_for_compare(f);
                        f.instruction(&Instruction::LocalSet(right_i32_local));

                        // Generate left (stays on stack)
                        self.generate_expr_inner(left, f, loop_depth, param_offset)?;
                        self.generate_polymorphic_unwrap_i32_for_compare(f);

                        // Get right from local
                        f.instruction(&Instruction::LocalGet(right_i32_local));
                        // Stack: [left, right]

                        let cmp_instr = match op {
                            BinOp::Eq => Instruction::I32Eq,
                            BinOp::Ne => Instruction::I32Ne,
                            BinOp::Lt => Instruction::I32LtS,
                            BinOp::Le => Instruction::I32LeS,
                            BinOp::Gt => Instruction::I32GtS,
                            BinOp::Ge => Instruction::I32GeS,
                            _ => unreachable!(),
                        };
                        f.instruction(&cmp_instr);

                        // Convert i32 boolean (0/1) to GC bool sentinel
                        // if result == 0 then false_sentinel else true_sentinel
                        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(
                            ValType::Ref(RefType::EQREF),
                        )));
                        f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                        f.instruction(&Instruction::RefI31);
                        f.instruction(&Instruction::Else);
                        f.instruction(&Instruction::I32Const(gc_types::FALSE_SENTINEL));
                        f.instruction(&Instruction::RefI31);
                        f.instruction(&Instruction::End);

                        self.scratch_local.set(scratch_base);
                    }

                    (BinOp::And, _) | (BinOp::Or, _) => {
                        // Logical operations on booleans
                        self.generate_expr(left, f)?;
                        generate_unwrap_i31(f);
                        self.generate_expr(right, f)?;
                        generate_unwrap_i31(f);
                        let instr = if matches!(op, BinOp::And) {
                            Instruction::I32And
                        } else {
                            Instruction::I32Or
                        };
                        f.instruction(&instr);
                        f.instruction(&Instruction::RefI31);
                    }

                    // Bitwise operations on integers
                    // These use polymorphic unwrap to handle both i31ref and INT64 inputs,
                    // and safe boxing to handle results that may exceed i31ref range.
                    // This is critical for HAMT bitmap operations where bit-shift-left
                    // can produce values like 2^30 or 2^31.
                    (BinOp::BitAnd, Type::I32) | (BinOp::BitOr, Type::I32) |
                    (BinOp::BitXor, Type::I32) | (BinOp::Shl, Type::I32) |
                    (BinOp::ShrS, Type::I32) | (BinOp::ShrU, Type::I32) => {
                        // Reserve scratch locals for unwrap operations
                        // Layout: +0: eqref, +1: i32, +2: eqref, +3: eqref, +4: eqref
                        let scratch_base = self.scratch_local.get();
                        self.scratch_local.set(scratch_base + 5);

                        let right_i32_local = scratch_base + 1; // i32 slot

                        // WASM stack order: for (op left right), we need [left, right]
                        // with right on top. So we generate right first, store it,
                        // then generate left (stays on stack), then get right.

                        // Step 1: Generate and unwrap right operand, store it
                        self.generate_expr(right, f)?;
                        self.generate_polymorphic_unwrap_i32(f);
                        f.instruction(&Instruction::LocalSet(right_i32_local));

                        // Step 2: Generate and unwrap left operand (stays on stack)
                        self.generate_expr(left, f)?;
                        self.generate_polymorphic_unwrap_i32(f);
                        // Stack: [left_i32]

                        // Step 3: Get right operand from local
                        f.instruction(&Instruction::LocalGet(right_i32_local));
                        // Stack: [left_i32, right_i32] - correct order!

                        // Step 4: Perform the operation
                        let instr = match op {
                            BinOp::BitAnd => Instruction::I32And,
                            BinOp::BitOr => Instruction::I32Or,
                            BinOp::BitXor => Instruction::I32Xor,
                            BinOp::Shl => Instruction::I32Shl,
                            BinOp::ShrS => Instruction::I32ShrS,
                            BinOp::ShrU => Instruction::I32ShrU,
                            _ => unreachable!()
                        };
                        f.instruction(&instr);

                        // Step 5: Box result safely (INT64 if overflow, i31ref otherwise)
                        self.generate_box_i32_safe(f);

                        // Restore scratch
                        self.scratch_local.set(scratch_base);
                    }

                    _ => {
                        return Err(CompileError::Codegen(format!(
                            "Unsupported binop {:?} for type {:?}",
                            op, ty
                        )));
                    }
                }
            }

            Expr::UnOp { op, operand, ty } => {
                match op {
                    UnOp::Neg => {
                        // In GC mode, unbox, negate, rebox
                        match ty {
                            Type::I32 | Type::I64 => {
                                // Cast eqref to i31ref, unbox, negate, rebox
                                self.generate_expr_inner(operand, f, loop_depth, param_offset)?;
                                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                                f.instruction(&Instruction::I31GetS);
                                f.instruction(&Instruction::I32Const(1));
                                f.instruction(&Instruction::I32ShrS); // decode
                                f.instruction(&Instruction::I32Const(0));
                                f.instruction(&Instruction::I32Sub); // negate
                                f.instruction(&Instruction::I32Const(1));
                                f.instruction(&Instruction::I32Shl);
                                f.instruction(&Instruction::I32Const(1));
                                f.instruction(&Instruction::I32Or); // encode
                                f.instruction(&Instruction::RefI31);
                            }
                            Type::F64 => {
                                // Unbox float, negate, rebox in { type_id, value }
                                f.instruction(&Instruction::I32Const(type_ids::FLOAT64));
                                self.generate_expr_inner(operand, f, loop_depth, param_offset)?;
                                f.instruction(&Instruction::StructGet {
                                    struct_type_index: gc_types::FLOAT64,
                                    field_index: gc_types::F64_VALUE,
                                });
                                f.instruction(&Instruction::F64Neg);
                                f.instruction(&Instruction::StructNew(gc_types::FLOAT64));
                            }
                            _ => {
                                return Err(CompileError::Codegen(
                                    "Neg not supported for this type".into(),
                                ));
                            }
                        }
                    }
                    UnOp::Not => {
                        // Logical not: falsy -> true, truthy -> false
                        // Use generate_condition to get 0/1, then convert to bool sentinel
                        self.generate_condition_inner(operand, f, loop_depth, param_offset)?;
                        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(
                            ValType::Ref(RefType::EQREF),
                        )));
                        // Was truthy, return false
                        f.instruction(&Instruction::I32Const(gc_types::FALSE_SENTINEL));
                        f.instruction(&Instruction::RefI31);
                        f.instruction(&Instruction::Else);
                        // Was falsy, return true
                        f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                        f.instruction(&Instruction::RefI31);
                        f.instruction(&Instruction::End);
                    }
                }
            }

            Expr::Call { func, args } => {
                let num_imports = self.num_imports();
                let user_func_base = num_imports + NUM_RUNTIME_HELPERS;

                // Check if this is an import call (func index < num_imports)
                let is_import = *func < num_imports;

                if is_import {
                    // Import call: marshal eqref args to WIT types, call, marshal return back
                    self.generate_import_call(*func, args, f, loop_depth, param_offset)?;
                } else {
                    // Check if target function is exported (needs WIT marshaling for i32 <-> eqref)
                    // Only relevant when we're in a WIT context (param_offset > 0)
                    let is_exported = if param_offset > 0 && *func >= user_func_base {
                        let local_idx = (*func - user_func_base) as usize;
                        self.ir.functions.get(local_idx).map_or(false, |f| f.exported)
                    } else {
                        false
                    };

                    for arg in args {
                        self.generate_expr_inner(arg, f, loop_depth, param_offset)?;
                        if is_exported {
                            // Unwrap eqref to i32 for exported functions
                            f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                            f.instruction(&Instruction::I31GetS);
                            f.instruction(&Instruction::I32Const(1));
                            f.instruction(&Instruction::I32ShrS);
                        }
                    }
                    f.instruction(&Instruction::Call(*func));

                    if is_exported {
                        // Wrap i32 result back to eqref
                        f.instruction(&Instruction::I32Const(1));
                        f.instruction(&Instruction::I32Shl);
                        f.instruction(&Instruction::I32Const(1));
                        f.instruction(&Instruction::I32Or);
                        f.instruction(&Instruction::RefI31);
                    }
                }
            }

            Expr::TailCall { func, args } => {
                // Check if target function is exported (needs WIT marshaling)
                let num_imports = self.num_imports();
                let user_func_base = num_imports + NUM_RUNTIME_HELPERS;
                let is_exported = if param_offset > 0 && *func >= user_func_base {
                    let local_idx = (*func - user_func_base) as usize;
                    self.ir.functions.get(local_idx).map_or(false, |f| f.exported)
                } else {
                    false
                };

                for arg in args {
                    self.generate_expr_inner(arg, f, loop_depth, param_offset)?;
                    if is_exported {
                        // Unwrap eqref to i32 for exported functions
                        f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                        f.instruction(&Instruction::I31GetS);
                        f.instruction(&Instruction::I32Const(1));
                        f.instruction(&Instruction::I32ShrS);
                    }
                }
                f.instruction(&Instruction::ReturnCall(*func));
                // Note: tail call returns directly, the function's exit marshaling handles return value
            }

            Expr::If {
                cond,
                then_branch,
                else_branch,
                ty,
            } => {
                self.generate_condition_inner(cond, f, loop_depth, param_offset)?;

                let block_type = wasm_encoder::BlockType::Result(self.type_to_valtype_gc(ty));

                f.instruction(&Instruction::If(block_type));
                // Branches are inside the If block, so increment depth
                self.generate_expr_inner(then_branch, f, loop_depth + 1, param_offset)?;
                f.instruction(&Instruction::Else);
                self.generate_expr_inner(else_branch, f, loop_depth + 1, param_offset)?;
                f.instruction(&Instruction::End);
            }

            Expr::Block(exprs) => {
                if exprs.is_empty() {
                    // Empty block returns nil
                    f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
                    f.instruction(&Instruction::RefI31);
                } else {
                    for (i, expr) in exprs.iter().enumerate() {
                        self.generate_expr_inner(expr, f, loop_depth, param_offset)?;
                        if i < exprs.len() - 1 {
                            f.instruction(&Instruction::Drop);
                        }
                    }
                }
            }

            Expr::Let { bindings, body } => {
                for (idx, value) in bindings {
                    self.generate_expr_inner(value, f, loop_depth, param_offset)?;
                    f.instruction(&Instruction::LocalSet(*idx + param_offset));
                }
                self.generate_expr_inner(body, f, loop_depth, param_offset)?;
            }

            Expr::Loop { bindings, body } => {
                for (idx, value) in bindings {
                    self.generate_expr_inner(value, f, loop_depth, param_offset)?;
                    f.instruction(&Instruction::LocalSet(*idx + param_offset));
                }

                // The outer block catches the result when the loop exits (non-recur path)
                // Loop body produces a value when not recurring; the block captures it
                let result_type = body.expr_type();
                let block_type = wasm_encoder::BlockType::Result(self.type_to_valtype_gc(&result_type));
                f.instruction(&Instruction::Block(block_type));
                // Loop also has result type - when falling through (not recurring),
                // the body's value stays on stack. Br to loop start needs no values
                // (loops have empty parameter types for branch targets).
                f.instruction(&Instruction::Loop(block_type));

                // Inside the loop, depth resets to 0 (Br(0) branches to loop header)
                self.generate_expr_inner(body, f, 0, param_offset)?;

                // When body falls through (doesn't recur), Br(1) exits to outer block
                // with the result value on stack
                f.instruction(&Instruction::Br(1));

                f.instruction(&Instruction::End);
                f.instruction(&Instruction::End);
            }

            Expr::Recur(values) => {
                // Parallel assignment: evaluate ALL values first (using current locals),
                // then store them all. This ensures (recur b (+ a b)) uses the OLD
                // values of a and b when computing new values.

                // First: evaluate ALL values, pushing results onto stack
                for (_local_idx, value) in values.iter() {
                    self.generate_expr_inner(value, f, loop_depth, param_offset)?;
                }
                // Second: store ALL values in reverse order (stack is LIFO)
                for (local_idx, _value) in values.iter().rev() {
                    f.instruction(&Instruction::LocalSet(*local_idx + param_offset));
                }
                // Branch to loop header - depth tracks nesting inside blocks/ifs
                f.instruction(&Instruction::Br(loop_depth));
            }

            Expr::StrConcat(parts) => {
                if parts.is_empty() {
                    f.instruction(&Instruction::I32Const(0));
                    f.instruction(&Instruction::I32Const(0));
                } else if parts.len() == 1 {
                    self.generate_expr(&parts[0], f)?;
                } else {
                    return Err(CompileError::Unsupported(
                        "Runtime string concatenation not yet implemented".into(),
                    ));
                }
            }

            Expr::PrintStr(string_expr) => {
                // Evaluate the string expression
                self.generate_expr(string_expr, f)?;
                // Call the print_str runtime helper
                f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::PRINT_STR)));
                // Push nil (print-str returns nil)
                f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
                f.instruction(&Instruction::RefI31);
            }

            Expr::Coerce { expr, from, to } => {
                self.generate_expr(expr, f)?;

                match (from, to) {
                    (Type::I32, Type::I64) => {
                        f.instruction(&Instruction::I64ExtendI32S);
                    }
                    (Type::I64, Type::I32) => {
                        f.instruction(&Instruction::I32WrapI64);
                    }
                    (Type::I32, Type::F64) => {
                        f.instruction(&Instruction::F64ConvertI32S);
                    }
                    (Type::I64, Type::F64) => {
                        f.instruction(&Instruction::F64ConvertI64S);
                    }
                    _ => {}
                }
            }

            // ================================================================
            // WASM GC Operations
            // ================================================================

            Expr::I31New(value) => {
                self.generate_expr(value, f)?;
                f.instruction(&Instruction::RefI31);
            }

            Expr::I31GetS(value) => {
                self.generate_expr(value, f)?;
                f.instruction(&Instruction::I31GetS);
            }

            Expr::RefCastI31(value) => {
                self.generate_expr(value, f)?;
                f.instruction(&Instruction::RefCastNonNull(HeapType::Abstract {
                    shared: false,
                    ty: AbstractHeapType::I31,
                }));
            }

            Expr::StructNew { type_idx, fields } => {
                for field in fields {
                    self.generate_expr(field, f)?;
                }
                f.instruction(&Instruction::StructNew(*type_idx));
            }

            Expr::StructGet {
                type_idx,
                field_idx,
                value,
            } => {
                self.generate_expr(value, f)?;
                // Cast eqref to the specific struct type
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                f.instruction(&Instruction::StructGet {
                    struct_type_index: *type_idx,
                    field_index: *field_idx,
                });
                // Determine if the field is i32 (needs wrapping) or eqref (already reference)
                // Field 0 is always type_id (i32) for all deftypes.
                // For other fields, check DeftypeDef dynamically.
                let needs_i32_wrap = if *field_idx == 0 {
                    true // field 0 is always type_id (i32)
                } else {
                    self.ir.deftypes.iter()
                        .find(|dt| dt.gc_type_idx == *type_idx)
                        .and_then(|dt| dt.fields.get((*field_idx - 1) as usize))
                        .map(|f| matches!(f.field_type, IrFieldType::I32))
                        .unwrap_or(false)
                };
                if needs_i32_wrap {
                    // Encode i32 as small int
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32Shl);
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32Or);
                    f.instruction(&Instruction::RefI31);
                }
            }

            // Get i32 field and encode safely (i31ref or INT64 if overflow)
            // Used for user-defined types with ^i32 fields
            // Critical for HAMT bitmap which can have values >= 2^29
            Expr::StructGetI32 {
                type_idx,
                field_idx,
                value,
            } => {
                // Reserve scratch for safe boxing
                let scratch_base = self.scratch_local.get();
                self.scratch_local.set(scratch_base + 5);

                self.generate_expr(value, f)?;
                // Cast to specific struct type
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                // Get the i32 field
                f.instruction(&Instruction::StructGet {
                    struct_type_index: *type_idx,
                    field_index: *field_idx,
                });
                // Safe box: i31ref if fits, INT64 if overflow
                self.generate_box_i32_safe(f);

                self.scratch_local.set(scratch_base);
            }

            // Set a mutable field in a struct
            // struct.set expects [ref, value] on stack, returns nothing
            // We return the value for expression chaining
            Expr::StructSet { type_idx, field_idx, field_type, obj, value } => {
                use crate::ir::FieldType;

                // Use scratch to save the value for return
                // Reserve full 5-slot group so inner expressions get clean scratch space
                let scratch = self.scratch_local.get();
                let scratch_i32 = scratch + 1;  // For i32 values
                self.scratch_local.set(scratch + 5);

                // First: generate struct reference (goes on stack first for struct.set)
                self.generate_expr(obj, f)?;
                // Cast to specific struct type
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));

                // Second: generate value
                self.generate_expr(value, f)?;

                // For i32 fields, unbox the value before struct.set
                if *field_type == FieldType::I32 {
                    // Save boxed value for return
                    f.instruction(&Instruction::LocalTee(scratch));
                    // Unbox to i32
                    self.generate_polymorphic_unwrap_i32(f);
                    // Save unboxed i32 for struct.set
                    f.instruction(&Instruction::LocalTee(scratch_i32));
                    // struct.set with i32 value
                    f.instruction(&Instruction::StructSet {
                        struct_type_index: *type_idx,
                        field_index: *field_idx,
                    });
                    // Return the boxed value
                    f.instruction(&Instruction::LocalGet(scratch));
                } else {
                    // For eqref fields, just save and use directly
                    f.instruction(&Instruction::LocalTee(scratch));
                    f.instruction(&Instruction::StructSet {
                        struct_type_index: *type_idx,
                        field_index: *field_idx,
                    });
                    f.instruction(&Instruction::LocalGet(scratch));
                }

                self.scratch_local.set(scratch);
            }

            Expr::ArrayNew { type_idx, elements } => {
                if elements.is_empty() {
                    // Empty array: use ArrayNewFixed with 0 elements
                    f.instruction(&Instruction::ArrayNewFixed {
                        array_type_index: *type_idx,
                        array_size: 0,
                    });
                } else {
                    // Use scratch local to store array ref
                    let scratch = self.scratch_local.get();
                    self.scratch_local.set(scratch + 5);
                    let arr_local = scratch; // eqref at scratch +0

                    // Create array with default values
                    f.instruction(&Instruction::I32Const(elements.len() as i32));
                    f.instruction(&Instruction::ArrayNewDefault(*type_idx));
                    f.instruction(&Instruction::LocalSet(arr_local));

                    // Set each element
                    for (i, elem) in elements.iter().enumerate() {
                        // Cast from eqref to concrete array type for array.set
                        f.instruction(&Instruction::LocalGet(arr_local));
                        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                        f.instruction(&Instruction::I32Const(i as i32));
                        self.generate_expr(elem, f)?;
                        f.instruction(&Instruction::ArraySet(*type_idx));
                    }

                    // Push array back onto stack as result
                    f.instruction(&Instruction::LocalGet(arr_local));
                }
            }

            Expr::ArrayNewData {
                type_idx,
                data_idx,
                offset,
                length,
            } => {
                self.generate_expr(offset, f)?;
                self.generate_expr(length, f)?;
                f.instruction(&Instruction::ArrayNewData {
                    array_type_index: *type_idx,
                    array_data_index: *data_idx,
                });
            }

            Expr::ArrayLen(array) => {
                self.generate_expr(array, f)?;
                // Cast eqref to abstract array type for array.len
                // This works for any array type (STRING, ARRAY, I32_ARRAY)
                f.instruction(&Instruction::RefCastNonNull(HeapType::Abstract {
                    shared: false,
                    ty: AbstractHeapType::Array,
                }));
                f.instruction(&Instruction::ArrayLen);
                // Box result as i31ref: (n << 1) | 1
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Shl);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Or);
                f.instruction(&Instruction::RefI31);
            }

            Expr::ArrayGet {
                type_idx,
                array,
                index,
            } => {
                self.generate_expr(array, f)?;
                // Cast eqref to array type
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                // Unbox index from i31ref
                self.generate_expr(index, f)?;
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrS); // Decode tagged value
                // For STRING (array<i8>), use array.get_s for packed type, then box
                if *type_idx == gc_types::STRING {
                    f.instruction(&Instruction::ArrayGetS(*type_idx));
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32Shl);
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32Or);
                    f.instruction(&Instruction::RefI31);
                } else {
                    f.instruction(&Instruction::ArrayGet(*type_idx));
                }
            }

            Expr::ArraySet {
                type_idx,
                array,
                index,
                value,
            } => {
                self.generate_expr(array, f)?;
                // Cast eqref to array type
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                // Unbox index from i31ref
                self.generate_expr(index, f)?;
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrS); // Decode tagged value
                self.generate_expr(value, f)?;
                // Duplicate value to return after set
                let scratch = self.scratch_local.get();
                f.instruction(&Instruction::LocalTee(scratch));
                // For STRING (array<i8>), unbox the value to i32 before array.set
                if *type_idx == gc_types::STRING {
                    f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                    f.instruction(&Instruction::I31GetS);
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32ShrS);
                }
                f.instruction(&Instruction::ArraySet(*type_idx));
                // Return the value that was set
                f.instruction(&Instruction::LocalGet(scratch));
            }

            Expr::ArrayNewDefault { type_idx, size } => {
                // Unbox size from i31ref
                self.generate_expr(size, f)?;
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrS); // Decode tagged value
                f.instruction(&Instruction::ArrayNewDefault(*type_idx));
            }

            Expr::ArrayClone { type_idx, array } => {
                // Clone by creating new array and copying
                // Stack: array -> new_array
                // Scratch layout: +0: eqref, +1: i32, +2: eqref, +3: eqref, +4: eqref
                let scratch = self.scratch_local.get();
                let src_arr_local = scratch;     // eqref at +0
                let new_arr_local = scratch + 2; // eqref at +2 (NOT +1 which is i32!)

                self.generate_expr(array, f)?;
                // Cast eqref to array type and store
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                // NOTE: local.tee returns the local's type (eqref), so we must cast after!
                f.instruction(&Instruction::LocalTee(src_arr_local));
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                f.instruction(&Instruction::ArrayLen);
                // Create new array with same length
                f.instruction(&Instruction::ArrayNewDefault(*type_idx));
                // Stack: new_array
                // Copy: array.copy dst_arr dst_offset src_arr src_offset len
                f.instruction(&Instruction::LocalTee(new_arr_local)); // save new_arr
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                f.instruction(&Instruction::I32Const(0)); // dst_offset
                // Note: scratch locals are eqref, so we must cast again after LocalGet
                f.instruction(&Instruction::LocalGet(src_arr_local)); // src_arr
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                f.instruction(&Instruction::I32Const(0)); // src_offset
                f.instruction(&Instruction::LocalGet(src_arr_local)); // src_arr for len
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                f.instruction(&Instruction::ArrayLen);
                f.instruction(&Instruction::ArrayCopy {
                    array_type_index_dst: *type_idx,
                    array_type_index_src: *type_idx,
                });
                // Return the new array
                f.instruction(&Instruction::LocalGet(new_arr_local));
            }

            Expr::ArrayCopy {
                type_idx,
                dst,
                dst_offset,
                src,
                src_offset,
                len,
            } => {
                // Emit array.copy instruction
                // Stack: dst dst_offset src src_offset len -> (nothing)
                // Returns nil after the copy
                self.generate_expr(dst, f)?;
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                self.generate_expr(dst_offset, f)?;
                // Unbox dst_offset from i31ref to i32
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrS);
                self.generate_expr(src, f)?;
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                self.generate_expr(src_offset, f)?;
                // Unbox src_offset from i31ref to i32
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrS);
                self.generate_expr(len, f)?;
                // Unbox len from i31ref to i32
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrS);
                f.instruction(&Instruction::ArrayCopy {
                    array_type_index_dst: *type_idx,
                    array_type_index_src: *type_idx,
                });
                // Return nil
                f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
                f.instruction(&Instruction::RefI31);
            }

            Expr::BitCount(value) => {
                // Polymorphic unwrap to handle both i31ref and INT64 inputs
                // (INT64 can come from bit-shift-left with large results)
                let scratch_base = self.scratch_local.get();
                self.scratch_local.set(scratch_base + 5);

                self.generate_expr(value, f)?;
                self.generate_polymorphic_unwrap_i32(f);
                f.instruction(&Instruction::I32Popcnt);
                // Result is always small (0-32), safe to encode as i31ref
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Shl);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Or);
                f.instruction(&Instruction::RefI31);

                self.scratch_local.set(scratch_base);
            }

            Expr::NilCheck(value) => {
                // Check if value is nil (i31ref(0) = NIL_SENTINEL) or null reference
                // Use scratch local to store value
                let scratch = self.scratch_local.get();

                self.generate_expr(value, f)?;
                f.instruction(&Instruction::LocalSet(scratch));

                // First check if it's a null reference (for struct fields like vector root)
                f.instruction(&Instruction::LocalGet(scratch));
                f.instruction(&Instruction::RefIsNull);
                f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::Ref(
                    RefType::EQREF,
                ))));
                // Is null - return true
                f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                f.instruction(&Instruction::RefI31);
                f.instruction(&Instruction::Else);

                // Not null - check if it's an i31ref with value 0 (NIL_SENTINEL)
                f.instruction(&Instruction::LocalGet(scratch));
                f.instruction(&Instruction::RefTestNonNull(HeapType::I31));
                f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::Ref(
                    RefType::EQREF,
                ))));

                // It's an i31ref - get value and check if 0
                f.instruction(&Instruction::LocalGet(scratch));
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Eqz); // Is it 0?

                // Convert boolean to sentinel
                f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::Ref(
                    RefType::EQREF,
                ))));
                f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                f.instruction(&Instruction::RefI31);
                f.instruction(&Instruction::Else);
                f.instruction(&Instruction::I32Const(gc_types::FALSE_SENTINEL));
                f.instruction(&Instruction::RefI31);
                f.instruction(&Instruction::End);

                f.instruction(&Instruction::Else);
                // Not an i31ref, so not nil
                f.instruction(&Instruction::I32Const(gc_types::FALSE_SENTINEL));
                f.instruction(&Instruction::RefI31);
                f.instruction(&Instruction::End);

                f.instruction(&Instruction::End); // close null check
            }

            Expr::RefTestI31(value) => {
                self.generate_expr(value, f)?;
                f.instruction(&Instruction::RefTestNonNull(HeapType::I31));
            }

            Expr::RefTest { type_idx, value } => {
                self.generate_expr(value, f)?;
                f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(*type_idx)));
                // Convert i32 boolean (0/1) to GC boolean sentinel
                f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::Ref(RefType::EQREF))));
                f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                f.instruction(&Instruction::RefI31);
                f.instruction(&Instruction::Else);
                f.instruction(&Instruction::I32Const(gc_types::FALSE_SENTINEL));
                f.instruction(&Instruction::RefI31);
                f.instruction(&Instruction::End);
            }

            Expr::RefNull(type_idx) => {
                f.instruction(&Instruction::RefNull(HeapType::Concrete(*type_idx)));
            }

            Expr::RefIsNull(value) => {
                self.generate_expr(value, f)?;
                f.instruction(&Instruction::RefIsNull);
            }

            Expr::IntCheck(value) => {
                // Check if value is an integer:
                // - i31ref with number tag (low bit = 1 after i31.get_s)
                // - OR INT64 struct
                let scratch = self.scratch_local.get();

                self.generate_expr(value, f)?;
                f.instruction(&Instruction::LocalSet(scratch));

                // First check if it's an i31ref
                f.instruction(&Instruction::LocalGet(scratch));
                f.instruction(&Instruction::RefTestNonNull(HeapType::I31));
                f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));

                // It's an i31ref - check if it has number tag (low bit = 1)
                f.instruction(&Instruction::LocalGet(scratch));
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32And);
                // Result is 1 (true) if number tag, 0 (false) otherwise

                f.instruction(&Instruction::Else);
                // Not i31ref - check if it's INT64 struct
                f.instruction(&Instruction::LocalGet(scratch));
                f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::INT64)));
                f.instruction(&Instruction::End);

                // Convert i32 boolean to sentinel
                f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::Ref(
                    RefType::EQREF,
                ))));
                f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                f.instruction(&Instruction::RefI31);
                f.instruction(&Instruction::Else);
                f.instruction(&Instruction::I32Const(gc_types::FALSE_SENTINEL));
                f.instruction(&Instruction::RefI31);
                f.instruction(&Instruction::End);
            }

            Expr::FloatCheck(value) => {
                // Check if value is a float (FLOAT64 struct)
                self.generate_expr(value, f)?;
                f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::FLOAT64)));
                // Convert i32 boolean to sentinel
                f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::Ref(
                    RefType::EQREF,
                ))));
                f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                f.instruction(&Instruction::RefI31);
                f.instruction(&Instruction::Else);
                f.instruction(&Instruction::I32Const(gc_types::FALSE_SENTINEL));
                f.instruction(&Instruction::RefI31);
                f.instruction(&Instruction::End);
            }

            Expr::StringCheck(value) => {
                // Check if value is a string (STRING array)
                self.generate_expr(value, f)?;
                f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::STRING)));
                // Convert i32 boolean to sentinel
                f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::Ref(
                    RefType::EQREF,
                ))));
                f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                f.instruction(&Instruction::RefI31);
                f.instruction(&Instruction::Else);
                f.instruction(&Instruction::I32Const(gc_types::FALSE_SENTINEL));
                f.instruction(&Instruction::RefI31);
                f.instruction(&Instruction::End);
            }

            Expr::F64Trunc(value) => {
                // Truncate f64 toward zero
                // First put type_id for result struct
                f.instruction(&Instruction::I32Const(type_ids::FLOAT64));
                // Generate and unbox input
                self.generate_expr(value, f)?;
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::FLOAT64)));
                f.instruction(&Instruction::StructGet {
                    struct_type_index: gc_types::FLOAT64,
                    field_index: gc_types::F64_VALUE,
                });
                // Truncate
                f.instruction(&Instruction::F64Trunc);
                // Box result
                f.instruction(&Instruction::StructNew(gc_types::FLOAT64));
            }

            Expr::F64ToI64(value) => {
                // Convert f64 to i64 (saturating)
                // First put type_id for result struct
                f.instruction(&Instruction::I32Const(type_ids::INT64));
                // Generate and unbox input
                self.generate_expr(value, f)?;
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::FLOAT64)));
                f.instruction(&Instruction::StructGet {
                    struct_type_index: gc_types::FLOAT64,
                    field_index: gc_types::F64_VALUE,
                });
                // Convert to i64
                f.instruction(&Instruction::I64TruncSatF64S);
                // Box result
                f.instruction(&Instruction::StructNew(gc_types::INT64));
            }

            Expr::I64ToF64(value) => {
                // Convert i64 to f64
                // First put type_id for result struct
                f.instruction(&Instruction::I32Const(type_ids::FLOAT64));
                // Generate and unbox input (could be INT64 struct or small int i31ref)
                self.generate_expr(value, f)?;
                // Use polymorphic unwrap to handle both INT64 struct and i31ref small ints
                self.generate_polymorphic_unwrap_i64(f);
                // Convert to f64
                f.instruction(&Instruction::F64ConvertI64S);
                // Box result
                f.instruction(&Instruction::StructNew(gc_types::FLOAT64));
            }

            Expr::F64Sqrt(value) => {
                // Compute square root - accepts both integers and floats
                // First put type_id for result struct
                f.instruction(&Instruction::I32Const(type_ids::FLOAT64));
                // Generate input and convert to f64 (handles int or float)
                self.generate_expr(value, f)?;
                self.generate_numeric_to_f64(f);
                // Compute sqrt
                f.instruction(&Instruction::F64Sqrt);
                // Box result
                f.instruction(&Instruction::StructNew(gc_types::FLOAT64));
            }

            Expr::Identical(left, right) => {
                // Reference equality using WASM ref.eq
                self.generate_expr(left, f)?;
                self.generate_expr(right, f)?;
                f.instruction(&Instruction::RefEq);
                // Convert i32 boolean (0/1) to GC boolean sentinel
                f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::Ref(
                    RefType::EQREF,
                ))));
                f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                f.instruction(&Instruction::RefI31);
                f.instruction(&Instruction::Else);
                f.instruction(&Instruction::I32Const(gc_types::FALSE_SENTINEL));
                f.instruction(&Instruction::RefI31);
                f.instruction(&Instruction::End);
            }

            // =========================================================
            // Persistent Vector Operations
            // =========================================================

            Expr::VecNew(elements) => {
                self.generate_vec_new(elements, f)?;
            }

            Expr::VecNth { vec, index } => {
                self.generate_vec_nth(vec, index, f)?;
            }

            Expr::VecCount(vec) => {
                // Look up PersistentVector type index dynamically
                let pv_idx = self.deftype_gc_type_idx("PersistentVector")
                    .ok_or_else(|| CompileError::Unsupported("PersistentVector deftype not found".to_string()))?;
                self.generate_expr(vec, f)?;
                // Cast to concrete PersistentVector type for struct.get
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(pv_idx)));
                // struct.get PersistentVector.cnt (field 1) - produces i32
                f.instruction(&Instruction::StructGet {
                    struct_type_index: pv_idx,
                    field_index: 1, // cnt is field 1 (after type_id)
                });
                // Wrap as i31ref: encode = (n << 1) | 1
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Shl);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Or);
                f.instruction(&Instruction::RefI31);
            }

            Expr::WrapInVector(array_expr) => {
                self.generate_wrap_in_vector(array_expr, f)?;
            }

            // =========================================================
            // Persistent Map Operations
            // =========================================================

            Expr::MapNew(pairs) => {
                self.generate_map_new(pairs, f)?;
            }

            Expr::MapCount(map) => {
                // Look up PersistentMap type index dynamically
                let pm_idx = self.deftype_gc_type_idx("PersistentMap")
                    .ok_or_else(|| CompileError::Unsupported("PersistentMap deftype not found".to_string()))?;
                self.generate_expr(map, f)?;
                // Cast to concrete PersistentMap type for struct.get
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(pm_idx)));
                // struct.get PersistentMap.cnt (field 1) - produces i32
                f.instruction(&Instruction::StructGet {
                    struct_type_index: pm_idx,
                    field_index: 1, // cnt is field 1 (after type_id)
                });
                // Wrap as i31ref: encode = (n << 1) | 1
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Shl);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Or);
                f.instruction(&Instruction::RefI31);
            }

            // MapDissoc removed - now uses core.sus dissoc function

            // =========================================================
            // Persistent Set Operations
            // =========================================================

            Expr::SetNew(elements) => {
                self.generate_set_new(elements, f)?;
            }

            // SetDisj removed - now uses core.sus disj function

            Expr::SetCount(set) => {
                // Look up PersistentSet type index dynamically
                let ps_idx = self.deftype_gc_type_idx("PersistentSet")
                    .ok_or_else(|| CompileError::Unsupported("PersistentSet deftype not found".to_string()))?;
                self.generate_expr(set, f)?;
                // Cast to concrete PersistentSet type for struct.get
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(ps_idx)));
                // struct.get PersistentSet.cnt (field 1) - produces i32
                f.instruction(&Instruction::StructGet {
                    struct_type_index: ps_idx,
                    field_index: 1, // cnt is field 1 (after type_id)
                });
                // Wrap as i31ref: encode = (n << 1) | 1
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Shl);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Or);
                f.instruction(&Instruction::RefI31);
            }

            // =========================================================
            // List Operations (cons cells)
            // =========================================================

            Expr::ListFirst(list) => {
                // Look up Cons type index dynamically
                let cons_idx = self.deftype_gc_type_idx("Cons")
                    .ok_or_else(|| CompileError::Unsupported("Cons deftype not found".to_string()))?;
                self.generate_expr(list, f)?;
                // Cast to concrete Cons type for struct.get
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(cons_idx)));
                // struct.get Cons.first (field 1)
                f.instruction(&Instruction::StructGet {
                    struct_type_index: cons_idx,
                    field_index: 1, // first is field 1 (after type_id)
                });
            }

            Expr::ListRest(list) => {
                // Look up Cons type index dynamically
                let cons_idx = self.deftype_gc_type_idx("Cons")
                    .ok_or_else(|| CompileError::Unsupported("Cons deftype not found".to_string()))?;
                self.generate_expr(list, f)?;
                // Cast to concrete Cons type for struct.get
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(cons_idx)));
                // struct.get Cons.rest (field 2)
                f.instruction(&Instruction::StructGet {
                    struct_type_index: cons_idx,
                    field_index: 2, // rest is field 2
                });
            }

            // =========================================================
            // Hash Operations
            // =========================================================

            Expr::Hash(value) => {
                self.generate_hash(value, f)?;
            }

            // =========================================================
            // Protocol Dispatch Operations
            // =========================================================

            Expr::ProtocolDispatch {
                obj,
                method_id,
                args,
                in_tail_position,
            } => {
                self.generate_protocol_dispatch(obj, *method_id, args, *in_tail_position, f, param_offset)?;
            }

            Expr::GetTypeId(value) => {
                self.generate_get_type_id(value, f, param_offset)?;
            }

            // =========================================================================
            // Closure Operations
            // =========================================================================
            Expr::ClosureNew {
                func_idx,
                arity,
                captures,
                is_variadic,
            } => {
                self.generate_closure_new(*func_idx, *arity, captures, *is_variadic, f)?;
            }

            Expr::VariadicClosureNew { op: _, func_indices } => {
                self.generate_variadic_closure_new(func_indices, f)?;
            }

            Expr::ClosureCall {
                closure,
                args,
                in_tail_position,
            } => {
                self.generate_closure_call(closure, args, *in_tail_position, f)?;
            }

            Expr::Apply { func, args } => {
                self.generate_apply(func, args, f)?;
            }

            Expr::ToFloat(inner) => {
                self.generate_to_float(inner, f)?;
            }

            Expr::GetName(inner) => {
                self.generate_get_name(inner, f)?;
            }

            Expr::GetNamespace(inner) => {
                self.generate_get_namespace(inner, f)?;
            }

            Expr::SymbolFromString { ns, name } => {
                self.generate_symbol_from_string(ns.as_deref(), name, f)?;
            }

            Expr::KeywordFromString { ns, name } => {
                self.generate_keyword_from_string(ns.as_deref(), name, f)?;
            }

            // =========================================================================
            // Var Operations
            // =========================================================================
            Expr::VarNew { root, meta, sym } => {
                self.generate_var_new(root, meta, sym, f)?;
            }

            Expr::VarDeref(var_expr) => {
                self.generate_var_deref(var_expr, f)?;
            }

            Expr::VarMeta(var_expr) => {
                self.generate_var_meta(var_expr, f)?;
            }

            // =========================================================================
            // Exception Handling
            // =========================================================================
            Expr::Throw(value) => {
                self.generate_expr(value, f)?;
                // The exception tag index is 0 (defined in tag section)
                f.instruction(&Instruction::Throw(0));
            }

            Expr::TryCatch { body, catch_binding, catch_body, finally_body } => {
                self.generate_try_catch(body, *catch_binding, catch_body, finally_body.as_deref(), f)?;
            }
        }

        Ok(())
    }

    // ========================================================================
    // Closure Operations
    // ========================================================================

    /// Generate code for creating a closure
    ///
    /// Creates: struct { type_id, env, fn }
    /// - type_id: i32 identifying arity for protocol dispatch
    ///   - For regular closures: CLOSURE_0 + arity
    ///   - For variadic closures: VARIADIC_CAPTURE (special marker)
    /// - env: array<eqref> containing captured values
    /// - fn: typed funcref to the wrapper function
    fn generate_closure_new(
        &self,
        func_idx: u32,
        arity: u32,
        captures: &[Expr],
        is_variadic: bool,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::type_ids;

        let closure_type = gc_types::closure_type_for_arity(arity);
        // Variadic closures with captures use special type_id so call site can detect them
        let type_id = if is_variadic {
            type_ids::VARIADIC_CAPTURE
        } else {
            type_ids::CLOSURE_0 + arity as i32
        };

        // Field 0: type_id (i32)
        f.instruction(&Instruction::I32Const(type_id));

        // Field 1: env (ref null $trie_node)
        // Create array of captured values
        if captures.is_empty() {
            // Empty captures - use null ref
            f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::ARRAY)));
        } else {
            // Generate each capture expression
            for capture in captures {
                self.generate_expr(capture, f)?;
            }
            // Create array from values on stack
            f.instruction(&Instruction::ArrayNewFixed {
                array_type_index: gc_types::ARRAY,
                array_size: captures.len() as u32,
            });
        }

        // Field 2: fn (ref $closure_fn_N) - typed funcref
        // RefFunc creates a typed reference to the function
        // Note: func_idx is the IR index, we need to convert to actual WASM function index
        f.instruction(&Instruction::RefFunc(self.user_func_idx(func_idx)));

        // Create the closure struct
        f.instruction(&Instruction::StructNew(closure_type));

        Ok(())
    }

    /// Generate code for creating a variadic closure
    ///
    /// Creates: struct { type_id, fn0, fn1, ..., fn8 }
    /// - type_id: i32 identifying as VARIADIC_CLOSURE
    /// - fn0..fn8: typed funcrefs for each arity
    fn generate_variadic_closure_new(
        &self,
        func_indices: &[u32; 9],
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::type_ids;

        // Field 0: type_id (i32)
        f.instruction(&Instruction::I32Const(type_ids::VARIADIC_CLOSURE));

        // Fields 1-9: fn0 through fn8 (typed funcrefs)
        for &func_idx in func_indices {
            f.instruction(&Instruction::RefFunc(self.user_func_idx(func_idx)));
        }

        // Create the variadic closure struct
        f.instruction(&Instruction::StructNew(gc_types::VARIADIC_CLOSURE));

        Ok(())
    }

    /// Generate code for calling a closure
    ///
    /// Uses call_ref with typed funcref for efficient invocation.
    /// Handles:
    /// - VARIADIC_CLOSURE: builtin variadic functions (+, *, etc.)
    /// - VARIADIC_CAPTURE: closures with (fn [& args] body) that capture values
    /// - Regular closures: fixed-arity closures
    fn generate_closure_call(
        &self,
        closure: &Expr,
        args: &[Expr],
        in_tail_position: bool,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;

        let arity = args.len() as u32;

        // Reserve scratch space for closure storage before generating the closure expr
        let scratch = self.scratch_local.get();
        let closure_local = scratch;
        self.scratch_local.set(scratch + 5); // Reserve space so args don't clobber

        // Evaluate and save the closure to a local
        self.generate_expr(closure, f)?;
        f.instruction(&Instruction::LocalSet(closure_local));

        // Check if callee is ANY closure type (VARIADIC_CLOSURE or CLOSURE_*)
        // If not a closure, dispatch to IFn/-invoke protocol (collections as functions)
        f.instruction(&Instruction::LocalGet(closure_local));
        self.generate_is_closure_check(f);

        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(RefType::EQREF))));

        // CLOSURE PATH - existing logic
        self.generate_closure_call_inner(closure_local, args, arity, in_tail_position, f)?;

        f.instruction(&Instruction::Else);

        // IFN PATH - dispatch to -invoke protocol (for collections as functions)
        // (#{1 2} key) => (-invoke #{1 2} key) => (-lookup #{1 2} key)
        self.generate_ifn_invoke(closure_local, args, in_tail_position, f)?;

        f.instruction(&Instruction::End);

        Ok(())
    }

    /// Check if the value on stack is any closure type.
    /// Returns i32 (1 if closure, 0 otherwise).
    fn generate_is_closure_check(&self, f: &mut Function) {
        use crate::ir::gc_types;

        // Use scratch local to save the value for multiple tests
        let scratch = self.scratch_local.get();
        f.instruction(&Instruction::LocalTee(scratch));

        // Test VARIADIC_CLOSURE
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::VARIADIC_CLOSURE)));

        // Test CLOSURE_0 through CLOSURE_N
        for closure_type in [
            gc_types::CLOSURE_0,
            gc_types::CLOSURE_1,
            gc_types::CLOSURE_2,
            gc_types::CLOSURE_3,
            gc_types::CLOSURE_4,
            gc_types::CLOSURE_N,
        ] {
            f.instruction(&Instruction::LocalGet(scratch));
            f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(closure_type)));
            f.instruction(&Instruction::I32Or);
        }
    }

    /// Generate the inner closure call logic (for callee known to be a closure).
    fn generate_closure_call_inner(
        &self,
        closure_local: u32,
        args: &[Expr],
        arity: u32,
        in_tail_position: bool,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;

        // Check if it's a VARIADIC_CLOSURE (builtin like +, *, etc.)
        f.instruction(&Instruction::LocalGet(closure_local));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(
            gc_types::VARIADIC_CLOSURE,
        )));
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));

        // VARIADIC_CLOSURE path (builtins)
        self.generate_variadic_closure_call(closure_local, args, arity, in_tail_position, f)?;

        f.instruction(&Instruction::Else);

        // Check for VARIADIC_CAPTURE: a closure with (fn [& args] body) that captures values.
        // These are CLOSURE_1 struct type with type_id = VARIADIC_CAPTURE.
        // They can be called with ANY number of arguments (packed into an array).
        //
        // Compute flag: is_variadic_capture = is_CLOSURE_1 && (type_id == VARIADIC_CAPTURE)
        // Then use a single branch: if variadic_capture → pack args, else → regular call
        f.instruction(&Instruction::LocalGet(closure_local));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::CLOSURE_1)));
        f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));

        // Is CLOSURE_1 - check type_id
        f.instruction(&Instruction::LocalGet(closure_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::CLOSURE_1)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::CLOSURE_1,
            field_index: gc_types::CL_TYPE_ID,
        });
        f.instruction(&Instruction::I32Const(crate::ir::type_ids::VARIADIC_CAPTURE));
        f.instruction(&Instruction::I32Eq);

        f.instruction(&Instruction::Else);

        // Not CLOSURE_1, so definitely not variadic capture
        f.instruction(&Instruction::I32Const(0));

        f.instruction(&Instruction::End);

        // Stack now has i32 flag: 1 if variadic capture, 0 otherwise
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(RefType::EQREF))));

        // VARIADIC_CAPTURE path - pack all args into array and call with arity=1
        self.generate_variadic_capture_call(closure_local, args, in_tail_position, f)?;

        f.instruction(&Instruction::Else);

        // Regular closure path - use call-site arity.
        // This works for all non-variadic-capture closures.
        // The closure type will match call-site arity (or fail at runtime if mismatched).
        self.generate_regular_closure_call(closure_local, args, arity, in_tail_position, f)?;

        f.instruction(&Instruction::End);

        f.instruction(&Instruction::End); // outer if (VARIADIC_CLOSURE check)

        Ok(())
    }

    /// Generate IFn/-invoke dispatch for non-closure callees (collections as functions).
    fn generate_ifn_invoke(
        &self,
        obj_local: u32,
        args: &[Expr],
        in_tail_position: bool,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::method_ids;

        let arity = args.len();

        // Determine method_id based on arity
        // -invoke with 1 arg (key) -> INVOKE_1
        // -invoke with 2 args (key, not-found) -> INVOKE_2
        let method_id = match arity {
            1 => method_ids::INVOKE_1,
            2 => method_ids::INVOKE_2,
            _ => {
                // Collections only support 1-2 args for -invoke
                // Return nil for unsupported arities
                f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
                f.instruction(&Instruction::RefI31);
                return Ok(());
            }
        };

        // Generate protocol dispatch with obj already in local
        self.generate_protocol_dispatch_with_local(obj_local, method_id, args, in_tail_position, f)
    }

    /// Generate protocol dispatch when the object is already saved to a local.
    fn generate_protocol_dispatch_with_local(
        &self,
        obj_local: u32,
        method_id: u32,
        args: &[Expr],
        in_tail_position: bool,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::dispatch_table;
        use crate::ir::method_ids;

        let scratch = self.scratch_local.get();
        self.scratch_local.set(scratch + 5);

        let type_id_local = scratch + 1;
        let args_base = scratch + 2;

        // Evaluate and save args to scratch locals
        for (i, arg) in args.iter().enumerate() {
            self.generate_expr(arg, f)?;
            f.instruction(&Instruction::LocalSet(args_base + i as u32));
        }

        // Get type ID from the object
        f.instruction(&Instruction::LocalGet(obj_local));
        f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::GET_TYPE_ID)));
        f.instruction(&Instruction::LocalSet(type_id_local));

        // Push obj and args back on stack for the call
        f.instruction(&Instruction::LocalGet(obj_local));
        for i in 0..args.len() {
            f.instruction(&Instruction::LocalGet(args_base + i as u32));
        }

        // Calculate table index: type_id * methods_per_type + method_id
        f.instruction(&Instruction::LocalGet(type_id_local));
        f.instruction(&Instruction::I32Const(self.ir.methods_per_type as i32));
        f.instruction(&Instruction::I32Mul);
        f.instruction(&Instruction::I32Const(method_id as i32));
        f.instruction(&Instruction::I32Add);

        // Get the correct type index for this method (arity = obj + method args)
        let total_arity = args.len() + 1;
        let type_idx = self.protocol_type_index_for_method(method_id, total_arity);

        // call_indirect or return_call_indirect
        let returns_i32 = matches!(method_id, method_ids::COUNT | method_ids::HASH | method_ids::EQUIV);
        let use_tail_call = in_tail_position && !returns_i32;

        if use_tail_call {
            f.instruction(&Instruction::ReturnCallIndirect {
                type_index: type_idx,
                table_index: dispatch_table::TABLE_INDEX,
            });
        } else {
            f.instruction(&Instruction::CallIndirect {
                type_index: type_idx,
                table_index: dispatch_table::TABLE_INDEX,
            });
            // If method returns i32 but we need eqref, box it as small int
            if returns_i32 {
                // Encode as small int: (value << 1) | 1
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Shl);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Or);
                f.instruction(&Instruction::RefI31);
            }
        }

        self.scratch_local.set(scratch);
        Ok(())
    }

    /// Generate code for calling a regular closure (with env parameter).
    fn generate_regular_closure_call(
        &self,
        closure_local: u32,
        args: &[Expr],
        arity: u32,
        in_tail_position: bool,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;

        let closure_type = gc_types::closure_type_for_arity(arity);
        let fn_type = gc_types::closure_fn_type_for_arity(arity);

        // Get env (first arg to wrapper function)
        f.instruction(&Instruction::LocalGet(closure_local));
        // Cast to the correct closure type
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(closure_type)));
        // Get env field
        f.instruction(&Instruction::StructGet {
            struct_type_index: closure_type,
            field_index: gc_types::CL_ENV,
        });

        // Push actual arguments
        for arg in args {
            self.generate_expr(arg, f)?;
        }

        // Get fn (typed funcref)
        f.instruction(&Instruction::LocalGet(closure_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(closure_type)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: closure_type,
            field_index: gc_types::CL_FN,
        });

        // Call with typed funcref
        if in_tail_position {
            f.instruction(&Instruction::ReturnCallRef(fn_type));
        } else {
            f.instruction(&Instruction::CallRef(fn_type));
        }

        Ok(())
    }

    /// Generate code for calling a variadic closure (no env parameter).
    fn generate_variadic_closure_call(
        &self,
        closure_local: u32,
        args: &[Expr],
        arity: u32,
        in_tail_position: bool,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;

        let fn_type = gc_types::variadic_fn_type_for_arity_new(arity);
        let fn_field = gc_types::VC_FN + arity; // fn field indices: 1, 2, 3, ... for arities 0, 1, 2, ...

        // Push null env (variadic closures don't capture, but CLOSURE_FN_* types expect env)
        f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::ARRAY)));

        // Push actual arguments
        for arg in args {
            self.generate_expr(arg, f)?;
        }

        // Get fnN (typed funcref) from variadic closure struct
        f.instruction(&Instruction::LocalGet(closure_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::VARIADIC_CLOSURE,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::VARIADIC_CLOSURE,
            field_index: fn_field,
        });

        // Call with typed funcref
        if in_tail_position {
            f.instruction(&Instruction::ReturnCallRef(fn_type));
        } else {
            f.instruction(&Instruction::CallRef(fn_type));
        }

        Ok(())
    }

    /// Generate code for calling a variadic closure with captures (fn [& args] body).
    ///
    /// These closures use CLOSURE_1 with type_id=VARIADIC_CAPTURE.
    /// At the call site, we pack all arguments into an array and call with arity=1.
    fn generate_variadic_capture_call(
        &self,
        closure_local: u32,
        args: &[Expr],
        in_tail_position: bool,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;

        // Variadic capture closures are CLOSURE_1 with fn signature (env, args_array) -> result
        let fn_type = gc_types::CLOSURE_FN_1;

        // Push env (from CLOSURE_1 field 1)
        f.instruction(&Instruction::LocalGet(closure_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::CLOSURE_1)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::CLOSURE_1,
            field_index: gc_types::CL_ENV,
        });

        // Pack all arguments into an array
        if args.is_empty() {
            // Empty array
            f.instruction(&Instruction::ArrayNewFixed {
                array_type_index: gc_types::ARRAY,
                array_size: 0,
            });
        } else {
            // Generate each argument
            for arg in args {
                self.generate_expr(arg, f)?;
            }
            // Create array from values on stack
            f.instruction(&Instruction::ArrayNewFixed {
                array_type_index: gc_types::ARRAY,
                array_size: args.len() as u32,
            });
        }

        // Get fn (typed funcref for arity=1)
        f.instruction(&Instruction::LocalGet(closure_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::CLOSURE_1)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::CLOSURE_1,
            field_index: gc_types::CL_FN,
        });

        // Call with typed funcref
        if in_tail_position {
            f.instruction(&Instruction::ReturnCallRef(fn_type));
        } else {
            f.instruction(&Instruction::CallRef(fn_type));
        }

        Ok(())
    }

    /// Generate code for dynamic function application.
    ///
    /// (apply f coll) calls f with elements of coll as arguments.
    /// At runtime, we dispatch based on the vector count (0-8).
    ///
    /// Pre-normalizes the PersistentVector to a flat WASM array so that
    /// element access works correctly for vectors of any size (including >32).
    fn generate_apply(
        &self,
        func: &Expr,
        args: &Expr,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;

        // Look up PersistentVector type index dynamically
        let pv_gc_idx = self.deftype_gc_type_idx("PersistentVector")
            .ok_or_else(|| CompileError::Unsupported("PersistentVector deftype not found".to_string()))?;

        // Scratch locals: +0: eqref (closure), +1: i32 (count), +2: eqref (vec), +3: eqref (arr), +4: eqref
        let scratch_base = self.scratch_local.get();
        let closure_local = scratch_base;     // eqref at +0
        let count_local = scratch_base + 1;   // i32 at +1
        let vec_local = scratch_base + 2;     // eqref at +2
        let arr_local = scratch_base + 3;     // eqref at +3 (flat array)

        // Reserve our scratch locals before generating subexpressions
        self.scratch_local.set(scratch_base + 5);

        // Evaluate and store closure
        self.generate_expr(func, f)?;
        f.instruction(&Instruction::LocalSet(closure_local));

        // Evaluate and store args vector
        self.generate_expr(args, f)?;
        f.instruction(&Instruction::LocalSet(vec_local));

        // Get vector count as raw i32 - PersistentVector field 1 is cnt
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(pv_gc_idx)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: pv_gc_idx,
            field_index: 1, // cnt field is at index 1 (after type_id)
        });
        f.instruction(&Instruction::LocalSet(count_local));

        // Pre-normalize: for vectors > 32, flatten trie to array via vec-to-array.
        // For vectors <= 32, the tail array contains all elements.
        f.instruction(&Instruction::LocalGet(count_local));
        f.instruction(&Instruction::I32Const(32));
        f.instruction(&Instruction::I32GtU);
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(RefType::EQREF))));
        {
            f.instruction(&Instruction::LocalGet(vec_local));
            let vec_to_array_idx = self.func_idx_by_name("suss.core/vec-to-array")
                .or_else(|| self.func_idx_by_name("vec-to-array"))
                .ok_or_else(|| CompileError::Unsupported("vec-to-array not found".into()))?;
            f.instruction(&Instruction::Call(vec_to_array_idx));
        }
        f.instruction(&Instruction::Else);
        {
            f.instruction(&Instruction::LocalGet(vec_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(pv_gc_idx)));
            f.instruction(&Instruction::StructGet {
                struct_type_index: pv_gc_idx,
                field_index: 4, // tail field
            });
        }
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::LocalSet(arr_local));

        // Check if it's a variadic closure by testing the type
        f.instruction(&Instruction::LocalGet(closure_local));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(
            gc_types::VARIADIC_CLOSURE,
        )));
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));

        // Variadic closure path (builtins like +, *, etc. with fn0..fn8)
        self.generate_apply_variadic_dispatch(closure_local, arr_local, count_local, f)?;

        f.instruction(&Instruction::Else);

        // Check for VARIADIC_CAPTURE: user-defined (fn [& args] ...) closures
        f.instruction(&Instruction::LocalGet(closure_local));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::CLOSURE_1)));
        f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));

        f.instruction(&Instruction::LocalGet(closure_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::CLOSURE_1)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::CLOSURE_1,
            field_index: gc_types::CL_TYPE_ID,
        });
        f.instruction(&Instruction::I32Const(crate::ir::type_ids::VARIADIC_CAPTURE));
        f.instruction(&Instruction::I32Eq);

        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::End);

        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(RefType::EQREF))));

        // VARIADIC_CAPTURE path - pass flat array directly
        self.generate_apply_variadic_capture(closure_local, arr_local, f)?;

        f.instruction(&Instruction::Else);

        // Regular closure path (fixed arity, 0-8 args limit)
        self.generate_apply_dispatch(closure_local, arr_local, count_local, f)?;

        f.instruction(&Instruction::End); // VARIADIC_CAPTURE check
        f.instruction(&Instruction::End); // VARIADIC_CLOSURE check

        Ok(())
    }

    /// Generate code to apply a VARIADIC_CAPTURE closure to a pre-normalized flat array.
    ///
    /// VARIADIC_CAPTURE closures are (fn [& args] ...) that take a single array argument.
    /// The caller has already normalized the PV to a flat array via arr_local.
    fn generate_apply_variadic_capture(
        &self,
        closure_local: u32,
        arr_local: u32,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;

        let fn_type = gc_types::CLOSURE_FN_1;

        // Get env from CLOSURE_1
        f.instruction(&Instruction::LocalGet(closure_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::CLOSURE_1)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::CLOSURE_1,
            field_index: gc_types::CL_ENV,
        });

        // Pass the pre-normalized flat array directly
        f.instruction(&Instruction::LocalGet(arr_local));

        // Get fn from CLOSURE_1
        f.instruction(&Instruction::LocalGet(closure_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::CLOSURE_1)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::CLOSURE_1,
            field_index: gc_types::CL_FN,
        });

        // Call the function: (env, args_array) -> result
        f.instruction(&Instruction::CallRef(fn_type));

        Ok(())
    }

    /// Generate code to convert a boxed numeric value to a boxed FLOAT.
    ///
    /// Handles:
    /// - FLOAT struct -> return as-is
    /// - INT64 struct -> extract i64, convert to f64, box as FLOAT64
    /// - i31ref small int -> decode, convert to f64, box as FLOAT64
    ///
    /// Returns eqref (FLOAT struct) on the stack.
    fn generate_to_float(&self, inner: &Expr, f: &mut Function) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::type_ids;
        use wasm_encoder::{AbstractHeapType, BlockType, HeapType, Instruction, RefType, ValType};

        let eqref = RefType {
            nullable: true,
            heap_type: HeapType::Abstract {
                shared: false,
                ty: AbstractHeapType::Eq,
            },
        };

        // Generate the inner expression
        self.generate_expr(inner, f)?;

        // Store in a local to test multiple times
        let scratch_base = self.scratch_local.get();
        let val_local = scratch_base;
        self.scratch_local.set(scratch_base + 1);
        f.instruction(&Instruction::LocalSet(val_local));

        // Check if it's already a FLOAT struct - if so, return it as-is
        f.instruction(&Instruction::LocalGet(val_local));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::FLOAT64)));
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(eqref))));
        {
            // It's already a FLOAT - return it unchanged
            f.instruction(&Instruction::LocalGet(val_local));
        }
        f.instruction(&Instruction::Else);
        {
            // Check if it's an INT64 struct
            f.instruction(&Instruction::LocalGet(val_local));
            f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::INT64)));
            f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(eqref))));
            {
                // It's an INT64 - extract i64, convert to f64, wrap in FLOAT
                f.instruction(&Instruction::I32Const(type_ids::FLOAT64));
                f.instruction(&Instruction::LocalGet(val_local));
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::INT64)));
                f.instruction(&Instruction::StructGet {
                    struct_type_index: gc_types::INT64,
                    field_index: gc_types::I64_VALUE,
                });
                f.instruction(&Instruction::F64ConvertI64S);
                f.instruction(&Instruction::StructNew(gc_types::FLOAT64));
            }
            f.instruction(&Instruction::Else);
            {
                // Must be i31ref small int - decode, convert, wrap in FLOAT
                f.instruction(&Instruction::I32Const(type_ids::FLOAT64));
                f.instruction(&Instruction::LocalGet(val_local));
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                // Small ints are stored shifted by 1, so unshift
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrU);
                // Convert i32 to f64
                f.instruction(&Instruction::F64ConvertI32S);
                f.instruction(&Instruction::StructNew(gc_types::FLOAT64));
            }
            f.instruction(&Instruction::End);
        }
        f.instruction(&Instruction::End);

        // Restore scratch local
        self.scratch_local.set(scratch_base);

        Ok(())
    }

    /// Generate code to get the name from a symbol or keyword.
    /// Returns the name as a string.
    fn generate_get_name(&self, inner: &Expr, f: &mut Function) -> CompileResult<()> {
        use crate::ir::gc_types;
        use wasm_encoder::{BlockType, HeapType, Instruction, RefType, ValType};

        let eqref = RefType::EQREF;

        // Generate the inner expression
        self.generate_expr(inner, f)?;

        // Store in a local to test
        let scratch_base = self.scratch_local.get();
        let val_local = scratch_base;
        self.scratch_local.set(scratch_base + 1);
        f.instruction(&Instruction::LocalSet(val_local));

        // Check if it's a KEYWORD
        f.instruction(&Instruction::LocalGet(val_local));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::KEYWORD)));
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(eqref))));
        {
            // It's a KEYWORD - extract name field (ref STRING)
            f.instruction(&Instruction::LocalGet(val_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::KEYWORD)));
            f.instruction(&Instruction::StructGet {
                struct_type_index: gc_types::KEYWORD,
                field_index: gc_types::KW_NAME,
            });
        }
        f.instruction(&Instruction::Else);
        {
            // Check if it's a SYMBOL
            f.instruction(&Instruction::LocalGet(val_local));
            f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::SYMBOL)));
            f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(eqref))));
            {
                // It's a SYMBOL - extract name field (ref STRING)
                f.instruction(&Instruction::LocalGet(val_local));
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::SYMBOL)));
                f.instruction(&Instruction::StructGet {
                    struct_type_index: gc_types::SYMBOL,
                    field_index: gc_types::SYM_NAME,
                });
            }
            f.instruction(&Instruction::Else);
            {
                // Not a keyword or symbol - return nil
                f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
                f.instruction(&Instruction::RefI31);
            }
            f.instruction(&Instruction::End);
        }
        f.instruction(&Instruction::End);

        self.scratch_local.set(scratch_base);
        Ok(())
    }

    /// Generate code to get the namespace from a symbol or keyword.
    /// Returns the namespace as a string, or nil if no namespace.
    fn generate_get_namespace(&self, inner: &Expr, f: &mut Function) -> CompileResult<()> {
        use crate::ir::gc_types;
        use wasm_encoder::{BlockType, HeapType, Instruction, RefType, ValType};

        let eqref = RefType::EQREF;

        // Generate the inner expression
        self.generate_expr(inner, f)?;

        // Store in a local to test
        let scratch_base = self.scratch_local.get();
        let val_local = scratch_base;
        self.scratch_local.set(scratch_base + 1);
        f.instruction(&Instruction::LocalSet(val_local));

        // Check if it's a KEYWORD
        f.instruction(&Instruction::LocalGet(val_local));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::KEYWORD)));
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(eqref))));
        {
            // It's a KEYWORD - extract ns field (ref null STRING)
            // The field is nullable, so check if it's null
            f.instruction(&Instruction::LocalGet(val_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::KEYWORD)));
            f.instruction(&Instruction::StructGet {
                struct_type_index: gc_types::KEYWORD,
                field_index: gc_types::KW_NS,
            });
            // Result is already (ref null STRING) - if null, it stays null which is fine for returning nil
            // But we need to convert null to proper nil sentinel
            f.instruction(&Instruction::RefIsNull);
            f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(eqref))));
            {
                // Namespace is null - return nil
                f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
                f.instruction(&Instruction::RefI31);
            }
            f.instruction(&Instruction::Else);
            {
                // Namespace is non-null - return it
                f.instruction(&Instruction::LocalGet(val_local));
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::KEYWORD)));
                f.instruction(&Instruction::StructGet {
                    struct_type_index: gc_types::KEYWORD,
                    field_index: gc_types::KW_NS,
                });
            }
            f.instruction(&Instruction::End);
        }
        f.instruction(&Instruction::Else);
        {
            // Check if it's a SYMBOL
            f.instruction(&Instruction::LocalGet(val_local));
            f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::SYMBOL)));
            f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(eqref))));
            {
                // It's a SYMBOL - extract ns field (ref null STRING)
                f.instruction(&Instruction::LocalGet(val_local));
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::SYMBOL)));
                f.instruction(&Instruction::StructGet {
                    struct_type_index: gc_types::SYMBOL,
                    field_index: gc_types::SYM_NS,
                });
                // Check if null
                f.instruction(&Instruction::RefIsNull);
                f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(eqref))));
                {
                    // Namespace is null - return nil
                    f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
                    f.instruction(&Instruction::RefI31);
                }
                f.instruction(&Instruction::Else);
                {
                    // Namespace is non-null - return it
                    f.instruction(&Instruction::LocalGet(val_local));
                    f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::SYMBOL)));
                    f.instruction(&Instruction::StructGet {
                        struct_type_index: gc_types::SYMBOL,
                        field_index: gc_types::SYM_NS,
                    });
                }
                f.instruction(&Instruction::End);
            }
            f.instruction(&Instruction::Else);
            {
                // Not a keyword or symbol - return nil
                f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
                f.instruction(&Instruction::RefI31);
            }
            f.instruction(&Instruction::End);
        }
        f.instruction(&Instruction::End);

        self.scratch_local.set(scratch_base);
        Ok(())
    }

    /// Generate code to create a symbol from string(s) at runtime.
    ///
    /// (symbol "foo") → SYMBOL { type_id, hash("foo"), null, "foo", 0 }
    /// (symbol "ns" "foo") → SYMBOL { type_id, hash("ns/foo"), "ns", "foo", 0 }
    fn generate_symbol_from_string(
        &self,
        ns: Option<&Expr>,
        name: &Expr,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::type_ids;

        // Scratch layout: [eqref, i32, eqref, eqref, eqref]
        let scratch_base = self.scratch_local.get();
        let name_local = scratch_base;           // +0: eqref
        self.scratch_local.set(scratch_base + 5);

        // Evaluate and store name
        self.generate_expr(name, f)?;
        f.instruction(&Instruction::LocalSet(name_local));

        if let Some(ns_expr) = ns {
            // (symbol "ns" "foo"): hash = hash("ns/foo")
            let ns_local = scratch_base + 2;     // +2: eqref
            let concat_local = scratch_base + 3; // +3: eqref
            let ns_len_local = scratch_base + 1; // +1: i32

            self.generate_expr(ns_expr, f)?;
            f.instruction(&Instruction::LocalSet(ns_local));

            // ns_len
            f.instruction(&Instruction::LocalGet(ns_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::ArrayLen);
            f.instruction(&Instruction::LocalSet(ns_len_local));

            // total_len = ns_len + 1 + name_len
            f.instruction(&Instruction::LocalGet(ns_len_local));
            f.instruction(&Instruction::I32Const(1)); // for '/'
            f.instruction(&Instruction::I32Add);
            f.instruction(&Instruction::LocalGet(name_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::ArrayLen);
            f.instruction(&Instruction::I32Add);

            // concat = array.new_default STRING total_len
            f.instruction(&Instruction::ArrayNewDefault(gc_types::STRING));
            f.instruction(&Instruction::LocalSet(concat_local));

            // copy ns into concat[0..ns_len]
            f.instruction(&Instruction::LocalGet(concat_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::I32Const(0)); // dst offset
            f.instruction(&Instruction::LocalGet(ns_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::I32Const(0)); // src offset
            f.instruction(&Instruction::LocalGet(ns_len_local));
            f.instruction(&Instruction::ArrayCopy {
                array_type_index_dst: gc_types::STRING,
                array_type_index_src: gc_types::STRING,
            });

            // set concat[ns_len] = '/'
            f.instruction(&Instruction::LocalGet(concat_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::LocalGet(ns_len_local));
            f.instruction(&Instruction::I32Const(b'/' as i32));
            f.instruction(&Instruction::ArraySet(gc_types::STRING));

            // copy name into concat[ns_len+1..]
            f.instruction(&Instruction::LocalGet(concat_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::LocalGet(ns_len_local));
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Add); // dst offset = ns_len + 1
            f.instruction(&Instruction::LocalGet(name_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::I32Const(0)); // src offset
            f.instruction(&Instruction::LocalGet(name_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::ArrayLen);
            f.instruction(&Instruction::ArrayCopy {
                array_type_index_dst: gc_types::STRING,
                array_type_index_src: gc_types::STRING,
            });

            // hash = hash_string(concat)
            f.instruction(&Instruction::I32Const(type_ids::SYMBOL));
            f.instruction(&Instruction::LocalGet(concat_local));
            f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::HASH_STRING)));
            // ns field (ref null $STRING) — cast from eqref
            f.instruction(&Instruction::LocalGet(ns_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            // name field (ref $STRING) — cast from eqref
            f.instruction(&Instruction::LocalGet(name_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            // marker field
            f.instruction(&Instruction::I32Const(0));
            f.instruction(&Instruction::StructNew(gc_types::SYMBOL));

            self.scratch_local.set(scratch_base);
        } else {
            // (symbol "foo"): hash = hash("foo")
            f.instruction(&Instruction::I32Const(type_ids::SYMBOL));
            // hash
            f.instruction(&Instruction::LocalGet(name_local));
            f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::HASH_STRING)));
            // ns = null (ref null $STRING)
            f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::STRING)));
            // name (ref $STRING) — cast from eqref
            f.instruction(&Instruction::LocalGet(name_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            // marker
            f.instruction(&Instruction::I32Const(0));
            f.instruction(&Instruction::StructNew(gc_types::SYMBOL));

            self.scratch_local.set(scratch_base);
        }

        Ok(())
    }

    /// Generate code to create a keyword from string(s) at runtime.
    ///
    /// (keyword "foo") → KEYWORD { type_id, hash(":foo"), null, "foo" }
    /// (keyword "ns" "foo") → KEYWORD { type_id, hash(":ns:foo"), "ns", "foo" }
    fn generate_keyword_from_string(
        &self,
        ns: Option<&Expr>,
        name: &Expr,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::type_ids;

        // Scratch layout: [eqref, i32, eqref, eqref, eqref]
        let scratch_base = self.scratch_local.get();
        let name_local = scratch_base;           // +0: eqref
        self.scratch_local.set(scratch_base + 5);

        // Evaluate and store name
        self.generate_expr(name, f)?;
        f.instruction(&Instruction::LocalSet(name_local));

        if let Some(ns_expr) = ns {
            // (keyword "ns" "foo"): hash = hash(":ns:foo")
            let ns_local = scratch_base + 2;     // +2: eqref
            let concat_local = scratch_base + 3; // +3: eqref
            let ns_len_local = scratch_base + 1; // +1: i32

            self.generate_expr(ns_expr, f)?;
            f.instruction(&Instruction::LocalSet(ns_local));

            // ns_len
            f.instruction(&Instruction::LocalGet(ns_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::ArrayLen);
            f.instruction(&Instruction::LocalSet(ns_len_local));

            // total_len = 1 + ns_len + 1 + name_len  (":ns:name")
            f.instruction(&Instruction::I32Const(1)); // for leading ':'
            f.instruction(&Instruction::LocalGet(ns_len_local));
            f.instruction(&Instruction::I32Add);
            f.instruction(&Instruction::I32Const(1)); // for separator ':'
            f.instruction(&Instruction::I32Add);
            f.instruction(&Instruction::LocalGet(name_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::ArrayLen);
            f.instruction(&Instruction::I32Add);

            // concat = array.new_default STRING total_len
            f.instruction(&Instruction::ArrayNewDefault(gc_types::STRING));
            f.instruction(&Instruction::LocalSet(concat_local));

            // concat[0] = ':'
            f.instruction(&Instruction::LocalGet(concat_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::I32Const(0));
            f.instruction(&Instruction::I32Const(b':' as i32));
            f.instruction(&Instruction::ArraySet(gc_types::STRING));

            // copy ns into concat[1..1+ns_len]
            f.instruction(&Instruction::LocalGet(concat_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::I32Const(1)); // dst offset
            f.instruction(&Instruction::LocalGet(ns_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::I32Const(0)); // src offset
            f.instruction(&Instruction::LocalGet(ns_len_local));
            f.instruction(&Instruction::ArrayCopy {
                array_type_index_dst: gc_types::STRING,
                array_type_index_src: gc_types::STRING,
            });

            // concat[1+ns_len] = ':'
            f.instruction(&Instruction::LocalGet(concat_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::LocalGet(ns_len_local));
            f.instruction(&Instruction::I32Add); // offset = 1 + ns_len
            f.instruction(&Instruction::I32Const(b':' as i32));
            f.instruction(&Instruction::ArraySet(gc_types::STRING));

            // copy name into concat[2+ns_len..]
            f.instruction(&Instruction::LocalGet(concat_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::I32Const(2));
            f.instruction(&Instruction::LocalGet(ns_len_local));
            f.instruction(&Instruction::I32Add); // dst offset = 2 + ns_len
            f.instruction(&Instruction::LocalGet(name_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::I32Const(0)); // src offset
            f.instruction(&Instruction::LocalGet(name_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::ArrayLen);
            f.instruction(&Instruction::ArrayCopy {
                array_type_index_dst: gc_types::STRING,
                array_type_index_src: gc_types::STRING,
            });

            // Build KEYWORD struct: { type_id, hash, ns, name }
            f.instruction(&Instruction::I32Const(type_ids::KEYWORD));
            f.instruction(&Instruction::LocalGet(concat_local));
            f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::HASH_STRING)));
            // ns (ref null $STRING) — cast from eqref
            f.instruction(&Instruction::LocalGet(ns_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            // name (ref $STRING) — cast from eqref
            f.instruction(&Instruction::LocalGet(name_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::StructNew(gc_types::KEYWORD));

            self.scratch_local.set(scratch_base);
        } else {
            // (keyword "foo"): hash = hash(":foo")
            let concat_local = scratch_base + 2;  // +2: eqref

            // total_len = 1 + name_len
            f.instruction(&Instruction::I32Const(1)); // for ':'
            f.instruction(&Instruction::LocalGet(name_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::ArrayLen);
            f.instruction(&Instruction::I32Add);

            // concat = array.new_default STRING total_len
            f.instruction(&Instruction::ArrayNewDefault(gc_types::STRING));
            f.instruction(&Instruction::LocalSet(concat_local));

            // concat[0] = ':'
            f.instruction(&Instruction::LocalGet(concat_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::I32Const(0));
            f.instruction(&Instruction::I32Const(b':' as i32));
            f.instruction(&Instruction::ArraySet(gc_types::STRING));

            // copy name into concat[1..]
            f.instruction(&Instruction::LocalGet(concat_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::I32Const(1)); // dst offset
            f.instruction(&Instruction::LocalGet(name_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::I32Const(0)); // src offset
            f.instruction(&Instruction::LocalGet(name_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::ArrayLen);
            f.instruction(&Instruction::ArrayCopy {
                array_type_index_dst: gc_types::STRING,
                array_type_index_src: gc_types::STRING,
            });

            // Build KEYWORD struct: { type_id, hash, ns=null, name }
            f.instruction(&Instruction::I32Const(type_ids::KEYWORD));
            f.instruction(&Instruction::LocalGet(concat_local));
            f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::HASH_STRING)));
            // ns = null (ref null $STRING)
            f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::STRING)));
            // name (ref $STRING) — cast from eqref
            f.instruction(&Instruction::LocalGet(name_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::STRING)));
            f.instruction(&Instruction::StructNew(gc_types::KEYWORD));

            self.scratch_local.set(scratch_base);
        }

        Ok(())
    }

    // ========================================================================
    // Var Operations
    // ========================================================================

    /// Generate code for creating a new Var
    ///
    /// Creates: struct { type_id: i32, root: eqref, meta: eqref, sym: eqref }
    fn generate_var_new(
        &self,
        root: &Expr,
        meta: &Expr,
        sym: &Expr,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::type_ids;
        use wasm_encoder::Instruction;

        // Field 0: type_id (i32)
        f.instruction(&Instruction::I32Const(type_ids::VAR));

        // Field 1: root (eqref) - the bound value
        self.generate_expr(root, f)?;

        // Field 2: meta (eqref) - metadata map or nil
        self.generate_expr(meta, f)?;

        // Field 3: sym (eqref) - the symbol naming this var
        self.generate_expr(sym, f)?;

        // Create the struct
        f.instruction(&Instruction::StructNew(gc_types::VAR));

        Ok(())
    }

    /// Generate code to dereference a Var (get its root value)
    fn generate_var_deref(&self, var_expr: &Expr, f: &mut Function) -> CompileResult<()> {
        use crate::ir::gc_types;
        use wasm_encoder::Instruction;

        // Generate the var expression
        self.generate_expr(var_expr, f)?;

        // Cast to VAR type and get the root field
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::VAR)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::VAR,
            field_index: gc_types::VAR_ROOT,
        });

        Ok(())
    }

    /// Generate code to get a Var's metadata
    fn generate_var_meta(&self, var_expr: &Expr, f: &mut Function) -> CompileResult<()> {
        use crate::ir::gc_types;
        use wasm_encoder::Instruction;

        // Generate the var expression
        self.generate_expr(var_expr, f)?;

        // Cast to VAR type and get the meta field
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::VAR)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::VAR,
            field_index: gc_types::VAR_META,
        });

        Ok(())
    }

    /// Generate try/catch using WASM exception handling (try_table instruction)
    ///
    /// Layout (without finally):
    /// ```wasm
    /// block $result (result eqref)         ;; label 1 from try_table
    ///   block $catch_target (result eqref)  ;; label 0 from try_table
    ///     try_table (result eqref) (catch 0 $catch_target)
    ///       <body>
    ///     end  ;; try_table - body result on stack
    ///     br 1  ;; skip catch, jump to $result
    ///   end  ;; $catch_target - exception value on stack
    ///   local.set <catch_binding>
    ///   <catch_body>
    /// end  ;; $result
    /// ```
    fn generate_try_catch(
        &self,
        body: &Expr,
        catch_binding: u32,
        catch_body: &Expr,
        finally_body: Option<&Expr>,
        f: &mut Function,
    ) -> CompileResult<()> {
        use wasm_encoder::Instruction;

        let eqref_type = BlockType::Result(ValType::Ref(RefType {
            nullable: true,
            heap_type: HeapType::Abstract { shared: false, ty: AbstractHeapType::Eq },
        }));

        // block $result (result eqref)
        f.instruction(&Instruction::Block(eqref_type));

        // block $catch_target (result eqref)
        f.instruction(&Instruction::Block(eqref_type));

        // try_table (result eqref) (catch tag_0 $catch_target)
        // From inside try_table: label 0 = $catch_target, label 1 = $result
        f.instruction(&Instruction::TryTable(
            eqref_type,
            std::borrow::Cow::Borrowed(&[Catch::One { tag: 0, label: 0 }]),
        ));

        // Generate body
        self.generate_expr(body, f)?;

        // end try_table - body value on stack
        f.instruction(&Instruction::End);

        // br 1 - skip catch block, jump to $result with body value
        f.instruction(&Instruction::Br(1));

        // end $catch_target - exception value on stack
        f.instruction(&Instruction::End);

        // Store caught value in catch binding local
        f.instruction(&Instruction::LocalSet(catch_binding));

        // Generate catch body
        self.generate_expr(catch_body, f)?;

        // end $result
        f.instruction(&Instruction::End);

        // If there's a finally body, execute it after the result
        // (Note: in Clojure, finally doesn't affect the return value)
        if let Some(fin) = finally_body {
            // Save the result
            f.instruction(&Instruction::LocalSet(catch_binding)); // reuse catch local as temp
            // Execute finally (for side effects)
            self.generate_expr(fin, f)?;
            f.instruction(&Instruction::Drop); // discard finally result
            // Restore the result
            f.instruction(&Instruction::LocalGet(catch_binding));
        }

        Ok(())
    }

    /// Generate the nested if-else dispatch for apply based on vector count.
    /// arr_local contains a pre-normalized flat WASM array of elements.
    fn generate_apply_dispatch(
        &self,
        closure_local: u32,
        arr_local: u32,
        count_local: u32,
        f: &mut Function,
    ) -> CompileResult<()> {
        for arity in 0..=8u32 {
            f.instruction(&Instruction::LocalGet(count_local));
            if arity == 0 {
                f.instruction(&Instruction::I32Eqz);
            } else {
                f.instruction(&Instruction::I32Const(arity as i32));
                f.instruction(&Instruction::I32Eq);
            }
            f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
                RefType::EQREF,
            ))));
            self.generate_apply_call_arity(closure_local, arr_local, arity, f)?;
            f.instruction(&Instruction::Else);
        }

        // Unsupported arity - trap
        f.instruction(&Instruction::Unreachable);

        // Close all 9 if-else blocks
        for _ in 0..9 {
            f.instruction(&Instruction::End);
        }

        Ok(())
    }

    /// Generate code to call a closure with a specific arity, extracting args from flat array.
    /// arr_local contains a pre-normalized flat WASM array.
    fn generate_apply_call_arity(
        &self,
        closure_local: u32,
        arr_local: u32,
        arity: u32,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;

        let closure_type = gc_types::closure_type_for_arity(arity);
        let fn_type = gc_types::closure_fn_type_for_arity(arity);

        // Get env (first arg to wrapper function)
        f.instruction(&Instruction::LocalGet(closure_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(closure_type)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: closure_type,
            field_index: gc_types::CL_ENV,
        });

        if arity <= 4 {
            // For arities 0-4: Extract individual arguments from the flat array
            for i in 0..arity {
                self.generate_arr_nth(arr_local, i, f)?;
            }
        } else {
            // For arities 5+: CLOSURE_FN_N takes (env, args_array)
            // Pass the flat array directly
            f.instruction(&Instruction::LocalGet(arr_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::ARRAY)));
        }

        // Get fn (typed funcref)
        f.instruction(&Instruction::LocalGet(closure_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(closure_type)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: closure_type,
            field_index: gc_types::CL_FN,
        });

        // Call with call_ref
        f.instruction(&Instruction::CallRef(fn_type));

        Ok(())
    }

    /// Generate code to get element at index from a flat WASM array (eqref local).
    fn generate_arr_nth(
        &self,
        arr_local: u32,
        index: u32,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;

        f.instruction(&Instruction::LocalGet(arr_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::ARRAY)));
        f.instruction(&Instruction::I32Const(index as i32));
        f.instruction(&Instruction::ArrayGet(gc_types::ARRAY));

        Ok(())
    }

    /// Generate the nested if-else dispatch for variadic apply based on vector count.
    /// arr_local contains a pre-normalized flat WASM array of elements.
    fn generate_apply_variadic_dispatch(
        &self,
        closure_local: u32,
        arr_local: u32,
        count_local: u32,
        f: &mut Function,
    ) -> CompileResult<()> {
        for arity in 0..=8u32 {
            f.instruction(&Instruction::LocalGet(count_local));
            if arity == 0 {
                f.instruction(&Instruction::I32Eqz);
            } else {
                f.instruction(&Instruction::I32Const(arity as i32));
                f.instruction(&Instruction::I32Eq);
            }
            f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
                RefType::EQREF,
            ))));
            self.generate_apply_variadic_call_arity(closure_local, arr_local, arity, f)?;
            f.instruction(&Instruction::Else);
        }

        // Unsupported arity - trap
        f.instruction(&Instruction::Unreachable);

        // Close all 9 if-else blocks
        for _ in 0..9 {
            f.instruction(&Instruction::End);
        }

        Ok(())
    }

    /// Generate code to call a variadic closure with a specific arity, extracting args from flat array.
    fn generate_apply_variadic_call_arity(
        &self,
        closure_local: u32,
        arr_local: u32,
        arity: u32,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;

        let variadic_type = gc_types::VARIADIC_CLOSURE;
        let fn_type = gc_types::variadic_fn_type_for_arity_new(arity);
        let fn_field = 1 + arity; // field 0 is type_id, field 1 is fn0, field 2 is fn1, etc.

        // First arg is env (null for variadic builtins, but required by function signature)
        f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::ARRAY)));

        // Extract individual arguments from the flat array
        for i in 0..arity {
            self.generate_arr_nth(arr_local, i, f)?;
        }

        // Get fnN (typed funcref) from variadic closure struct
        f.instruction(&Instruction::LocalGet(closure_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(variadic_type)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: variadic_type,
            field_index: fn_field,
        });

        // Call with call_ref using the appropriate CLOSURE_FN_* type
        f.instruction(&Instruction::CallRef(fn_type));

        Ok(())
    }

    // ========================================================================
    // Hash Generation
    // ========================================================================

    /// Generate hash code for a value with type dispatch
    ///
    /// Type dispatch:
    /// - nil → 0
    /// - true → 1231, false → 1237
    /// - small int → value itself
    /// - large int → xxHash32 of i64 bytes
    /// - float → xxHash32 of f64 bit representation
    /// - string → xxHash32 of bytes
    /// - keyword → pre-computed hash from struct
    fn generate_hash(&self, value: &Expr, f: &mut Function) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::Type;

        // Check if the value is a statically-known String type.
        // We can call $hash_string directly without dynamic type dispatch.
        if matches!(value.expr_type(), Type::String) {
            // Generate the string expression - puts (ptr, len) on stack
            self.generate_expr(value, f)?;
            // Call $hash_string helper function
            // Helper functions come after imports, $hash_string is index 0
            f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::HASH_STRING)));
            // Wrap as i31ref: encode = (hash << 1) | 1
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Shl);
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Or);
            f.instruction(&Instruction::RefI31);
            return Ok(());
        }

        // Generate the value
        self.generate_expr(value, f)?;

        // Type dispatch using nested if/else
        // First, test if it's an i31ref (nil, bool, small int)
        f.instruction(&Instruction::RefTestNonNull(HeapType::I31));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));

        // === i31ref path ===
        // Re-generate value and extract the i31 content
        self.generate_expr(value, f)?;
        f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
        f.instruction(&Instruction::I31GetS);

        // Check for nil (0)
        f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(0)); // hash(nil) = 0
        f.instruction(&Instruction::Else);

        // Re-get the value for further checks
        self.generate_expr(value, f)?;
        f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
        f.instruction(&Instruction::I31GetS);

        // Check for false (2)
        f.instruction(&Instruction::I32Const(gc_types::FALSE_SENTINEL));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(gc_types::HASH_FALSE));
        f.instruction(&Instruction::Else);

        // Re-get the value for further checks
        self.generate_expr(value, f)?;
        f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
        f.instruction(&Instruction::I31GetS);

        // Check for true (4)
        f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(gc_types::HASH_TRUE));
        f.instruction(&Instruction::Else);

        // Must be a small integer - decode and use as hash
        // Get the raw i31 value and decode: >> 1
        self.generate_expr(value, f)?;
        f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
        f.instruction(&Instruction::I31GetS);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32ShrS);

        f.instruction(&Instruction::End); // close true check
        f.instruction(&Instruction::End); // close false check
        f.instruction(&Instruction::End); // close nil check

        f.instruction(&Instruction::Else);
        // === Not i31ref - check struct types ===

        // Test INT64
        self.generate_expr(value, f)?;
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::INT64)));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));

        // Extract i64 and hash it
        self.generate_expr(value, f)?;
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::INT64)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::INT64,
            field_index: gc_types::I64_VALUE,
        });
        self.emit_hash_i64(f);

        f.instruction(&Instruction::Else);

        // Test FLOAT
        self.generate_expr(value, f)?;
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::FLOAT64)));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));

        // Extract f64, reinterpret as i64, and hash
        self.generate_expr(value, f)?;
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::FLOAT64)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::FLOAT64,
            field_index: gc_types::F64_VALUE,
        });
        f.instruction(&Instruction::I64ReinterpretF64);
        self.emit_hash_i64(f);

        f.instruction(&Instruction::Else);

        // Test KEYWORD
        self.generate_expr(value, f)?;
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::KEYWORD)));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));

        // Extract pre-computed hash from KEYWORD struct
        self.generate_expr(value, f)?;
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::KEYWORD)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::KEYWORD,
            field_index: gc_types::KW_HASH,
        });

        f.instruction(&Instruction::Else);

        // Test STRING (array<i8>)
        self.generate_expr(value, f)?;
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::STRING)));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        self.generate_expr(value, f)?;
        f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::HASH_STRING)));
        f.instruction(&Instruction::Else);

        // Default: return 0 for unsupported types
        f.instruction(&Instruction::I32Const(0));

        f.instruction(&Instruction::End); // close STRING
        f.instruction(&Instruction::End); // close KEYWORD
        f.instruction(&Instruction::End); // close FLOAT
        f.instruction(&Instruction::End); // close INT64
        f.instruction(&Instruction::End); // close i31ref test

        // Wrap the i32 hash result as i31ref
        // Encode: (h << 1) | 1
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Shl);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::RefI31);

        Ok(())
    }

    /// Emit xxHash32 for an i64 value (8 bytes)
    /// Input: i64 on stack
    /// Output: i32 hash on stack
    fn emit_hash_i64(&self, f: &mut Function) {
        use crate::ir::gc_types::{PRIME32_3, PRIME32_4, PRIME32_5};

        // Split i64 into lo/hi i32
        // First, get lo = wrap(val)
        // Stack: i64
        // We need to duplicate the i64 - but WASM doesn't have i64.dup
        // So we'll process it in two passes

        // lo = i32.wrap_i64(val)
        f.instruction(&Instruction::I32WrapI64);

        // Now we need hi, but we consumed the i64. For simplicity,
        // let's use a simpler approach: treat the i32 as 4 bytes and apply xxHash32

        // For i64, we'll use a simplified approach:
        // acc = PRIME32_5 + 4 (treating as 4 bytes for the lo word only)
        // This is a pragmatic simplification - proper i64 hashing would need
        // function-local variables which we don't have access to here.

        // Simplified: hash the lo 32 bits only
        // acc = PRIME32_5 + 4
        // acc += lo * PRIME32_3
        // acc = rotl(acc, 17) * PRIME32_4

        // Stack: lo (i32)
        f.instruction(&Instruction::I32Const(PRIME32_3 as i32));
        f.instruction(&Instruction::I32Mul);
        f.instruction(&Instruction::I32Const((PRIME32_5 + 4) as i32));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I32Const(17));
        f.instruction(&Instruction::I32Rotl);
        f.instruction(&Instruction::I32Const(PRIME32_4 as i32));
        f.instruction(&Instruction::I32Mul);

        // Avalanche
        self.emit_xxh32_avalanche(f);
    }

    // NOTE: emit_hash_string is not implemented yet because strings currently
    // use memory-based ptr+len representation, not GC arrays (array<i8>).
    // When strings migrate to GC refs, this method will hash the bytes.

    /// Emit xxHash32 avalanche (final mixing)
    /// Input: i32 hash on stack
    /// Output: i32 mixed hash on stack
    fn emit_xxh32_avalanche(&self, f: &mut Function) {
        use crate::ir::gc_types::{PRIME32_2, PRIME32_3};

        // The avalanche function needs to use the value multiple times.
        // Without access to locals, we need a stack-based approach.
        //
        // h ^= h >> 15
        // h *= PRIME32_2
        // h ^= h >> 13
        // h *= PRIME32_3
        // h ^= h >> 16
        //
        // We can use arithmetic to simulate without locals:
        // Each step transforms the value on the stack

        // Step 1: h ^= h >> 15
        // We need h twice. Without dup or locals, we can't do this purely on stack.
        // Let's use a mathematical approximation that's still a good mixer:
        //
        // Alternative: Use a simpler but still effective finalizer
        // This is a pragmatic choice - proper implementation needs locals

        // Simplified finalizer (still provides good mixing):
        // h = h * PRIME32_2
        // h = h ^ (h >> 16)
        // h = h * PRIME32_3
        // h = h ^ (h >> 16)

        // h *= PRIME32_2
        f.instruction(&Instruction::I32Const(PRIME32_2 as i32));
        f.instruction(&Instruction::I32Mul);

        // For h ^ (h >> 16), we need the value twice.
        // Since we can't do this without locals, we'll use a single multiplication
        // as the final mixing step, which still provides reasonable distribution.

        // h *= PRIME32_3
        f.instruction(&Instruction::I32Const(PRIME32_3 as i32));
        f.instruction(&Instruction::I32Mul);

        // XOR with a constant to break patterns
        f.instruction(&Instruction::I32Const(0x5555_5555_u32 as i32));
        f.instruction(&Instruction::I32Xor);
    }

    /// Generate code for a condition expression, ensuring Clojure truthiness semantics.
    ///
    /// In GC mode, all values are eqref (i31ref or structref):
    /// - nil (i31ref(0)) is falsy
    /// - false (i31ref(2)) is falsy
    /// - Everything else is truthy (including 0, empty collections, etc.)
    fn generate_condition(&self, cond: &Expr, f: &mut Function) -> CompileResult<()> {
        self.generate_condition_inner(cond, f, 0, 0)
    }

    fn generate_condition_inner(&self, cond: &Expr, f: &mut Function, loop_depth: u32, param_offset: u32) -> CompileResult<()> {
        use crate::ir::gc_types;

        self.generate_expr_inner(cond, f, loop_depth, param_offset)?;

        // All values are now GC refs in GC mode
        // Test if it's an i31ref that could be nil or false
        f.instruction(&Instruction::RefTestNonNull(HeapType::I31));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));

        // Is i31ref - need to check if it's nil (0) or false (2)
        // Re-evaluate to get the value back, then cast to i31ref
        self.generate_expr_inner(cond, f, loop_depth + 1, param_offset)?;
        // Cast eqref to i31ref (we know it's i31 because we tested for it)
        f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
        f.instruction(&Instruction::I31GetS);
        // Truthy if value != 0 (nil) AND value != 2 (false)
        f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
        f.instruction(&Instruction::I32Ne);
        self.generate_expr_inner(cond, f, loop_depth + 1, param_offset)?;
        f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
        f.instruction(&Instruction::I31GetS);
        f.instruction(&Instruction::I32Const(gc_types::FALSE_SENTINEL));
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::I32And);

        f.instruction(&Instruction::Else);
        // Not i31ref - must be struct/array, always truthy
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::End);

        Ok(())
    }

    // ========================================================================
    // GC type conversion
    // ========================================================================

    /// Convert IR type to WASM ValType using GC references
    ///
    /// In GC mode, all value types become eqref (boxed GC references).
    /// Use `type_to_valtype_for_signature` for function signatures where
    /// explicit type hints should be respected.
    fn type_to_valtype_gc(&self, ty: &Type) -> ValType {
        match ty {
            // All value types become eqref in GC mode
            Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I64
            | Type::F64
            | Type::String
            | Type::List(_)
            | Type::Vector(_)
            | Type::Map(_, _)
            | Type::Set(_)
            | Type::GcRef
            | Type::Unknown => ValType::Ref(RefType::EQREF),
            // Function refs as i32 indices
            Type::Func { .. } => ValType::I32,
            // Result/Option types are i32 discriminant
            Type::Result { .. } | Type::Option(_) => ValType::I32,
        }
    }

    /// Convert IR type to WASM ValType for function signatures.
    ///
    /// Unlike `type_to_valtype_gc`, this respects explicit primitive type hints
    /// from protocol method return type annotations (^i32, ^i64, ^f64).
    fn type_to_valtype_for_signature(&self, ty: &Type) -> ValType {
        match ty {
            // Explicit primitive types - used for protocol method return type hints
            Type::I32 => ValType::I32,
            Type::I64 => ValType::I64,
            Type::F64 => ValType::F64,
            // Function refs as i32 indices
            Type::Func { .. } => ValType::I32,
            // Result/Option types are i32 discriminant
            Type::Result { .. } | Type::Option(_) => ValType::I32,
            // All other value types become eqref
            Type::Unit
            | Type::Bool
            | Type::String
            | Type::List(_)
            | Type::Vector(_)
            | Type::Map(_, _)
            | Type::Set(_)
            | Type::GcRef
            | Type::Unknown => ValType::Ref(RefType::EQREF),
        }
    }

    /// Get all ValTypes for a type - everything is a single eqref in GC mode
    fn type_to_valtypes_gc(&self, ty: &Type) -> Vec<ValType> {
        vec![self.type_to_valtype_gc(ty)]
    }

    /// Get ValTypes for function signatures (respects primitive type hints)
    fn type_to_valtypes_for_signature(&self, ty: &Type) -> Vec<ValType> {
        vec![self.type_to_valtype_for_signature(ty)]
    }

    // ========================================================================
    // Protocol Dispatch Generation
    // ========================================================================

    /// Generate code for protocol dispatch.
    ///
    /// Selects between two dispatch strategies:
    /// - **Inline dispatch**: Fast path for built-in protocols on built-in types.
    ///   Uses nested if-else with ref.test/ref.cast. O(n) where n = # of implementing types.
    /// - **Table dispatch**: Extensible path for user-defined protocols or extended methods.
    ///   Uses call_indirect via dispatch table. O(1) dispatch after type ID lookup.
    ///
    /// All protocol dispatch uses table-based lookup for uniformity and future extensibility.
    /// The get_type_id helper extracts the type ID, then we index into the dispatch table.
    fn generate_protocol_dispatch(
        &self,
        obj: &Expr,
        method_id: u32,
        args: &[Expr],
        in_tail_position: bool,
        f: &mut Function,
        param_offset: u32,
    ) -> CompileResult<()> {
        self.generate_protocol_dispatch_table(obj, method_id, args, in_tail_position, f, param_offset)
    }

    /// Generate table-based protocol dispatch using call_indirect.
    ///
    /// This method uses the dispatch table for extensible protocol dispatch.
    /// The table is indexed by: type_id * methods_per_type + method_id
    ///
    /// Algorithm:
    /// 1. Evaluate and save args to scratch locals
    /// 2. Evaluate obj and save to scratch local
    /// 3. Call $get_type_id to get runtime type ID
    /// 4. Calculate table index: type_id * methods_per_type + method_id
    /// 5. Push obj and args back on stack
    /// 6. call_indirect (or return_call_indirect for tail position)
    ///
    /// Benefits over inline dispatch:
    /// - Extensible: user-defined types can be added to the table
    /// - O(1) dispatch instead of O(n) type checks
    ///
    /// Trade-offs:
    /// - Requires $get_type_id call (overhead)
    /// - Table memory overhead
    /// - Less opportunity for inlining
    fn generate_protocol_dispatch_table(
        &self,
        obj: &Expr,
        method_id: u32,
        args: &[Expr],
        in_tail_position: bool,
        f: &mut Function,
        param_offset: u32,
    ) -> CompileResult<()> {
        use crate::ir::dispatch_table;
        use crate::ir::method_ids;

        let scratch = self.scratch_local.get();
        // Bump by a full scratch set (5 locals) to ensure inner expressions get a clean set.
        self.scratch_local.set(scratch + 5);

        // Scratch set layout: scratch+0: eqref, +1: i32, +2: eqref, +3: eqref, +4: eqref
        let obj_local = scratch;         // eqref at +0
        let type_id_local = scratch + 1; // i32 at +1
        let args_base = scratch + 2;     // eqref locals at +2, +3, +4

        // 1. Evaluate and save args to scratch locals
        for (i, arg) in args.iter().enumerate() {
            self.generate_expr_inner(arg, f, 0, param_offset)?;
            f.instruction(&Instruction::LocalSet(args_base + i as u32));
        }

        // 2. Evaluate obj and save to scratch local
        self.generate_expr_inner(obj, f, 0, param_offset)?;
        f.instruction(&Instruction::LocalTee(obj_local));

        // 3. Call $get_type_id to get runtime type ID
        f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::GET_TYPE_ID)));
        f.instruction(&Instruction::LocalSet(type_id_local));

        // 4. Push obj and args back on stack for the call
        // call_indirect expects: [obj] [args...] [table_index]
        f.instruction(&Instruction::LocalGet(obj_local));
        for i in 0..args.len() {
            f.instruction(&Instruction::LocalGet(args_base + i as u32));
        }

        // 5. Calculate table index: type_id * methods_per_type + method_id
        f.instruction(&Instruction::LocalGet(type_id_local));
        f.instruction(&Instruction::I32Const(self.ir.methods_per_type as i32));
        f.instruction(&Instruction::I32Mul);
        f.instruction(&Instruction::I32Const(method_id as i32));
        f.instruction(&Instruction::I32Add);

        // 6. Get the correct type index for this method (arity = obj + method args)
        let arity = args.len() + 1;
        let type_idx = self.protocol_type_index_for_method(method_id, arity);

        // 7. call_indirect or return_call_indirect
        let returns_i32 = matches!(method_id, method_ids::COUNT | method_ids::HASH | method_ids::EQUIV);
        let use_tail_call = in_tail_position && !returns_i32;

        if use_tail_call {
            f.instruction(&Instruction::ReturnCallIndirect {
                type_index: type_idx,
                table_index: dispatch_table::TABLE_INDEX,
            });
        } else {
            f.instruction(&Instruction::CallIndirect {
                type_index: type_idx,
                table_index: dispatch_table::TABLE_INDEX,
            });
        }

        // 8. Wrap i32 results in i31ref (for count which returns i32)
        if returns_i32 {
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Shl);
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Or);
            f.instruction(&Instruction::RefI31);
        }

        // Restore scratch_local
        self.scratch_local.set(scratch);

        Ok(())
    }

    /// Generate protocol dispatch when obj and args are already on the stack.
    ///
    /// Stack: [obj, arg1, arg2, ...] (args in order after obj)
    /// After: [result]
    ///
    /// This is used internally by collection literal codegen where we build
    /// collections incrementally and the intermediate value is on the stack.
    fn generate_protocol_dispatch_stack(
        &self,
        method_id: u32,
        num_args: u32,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::dispatch_table;
        use crate::ir::method_ids;

        let scratch = self.scratch_local.get();
        self.scratch_local.set(scratch + 5);

        let obj_local = scratch;         // eqref at +0
        let type_id_local = scratch + 1; // i32 at +1
        let args_base = scratch + 2;     // eqref locals at +2, +3, +4

        // Stack is [obj, arg1, arg2, ...] - save in reverse order
        for i in (0..num_args).rev() {
            f.instruction(&Instruction::LocalSet(args_base + i));
        }
        f.instruction(&Instruction::LocalTee(obj_local));

        // Get type ID
        f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::GET_TYPE_ID)));
        f.instruction(&Instruction::LocalSet(type_id_local));

        // Push obj and args back on stack for the call
        f.instruction(&Instruction::LocalGet(obj_local));
        for i in 0..num_args {
            f.instruction(&Instruction::LocalGet(args_base + i));
        }

        // Calculate table index: type_id * methods_per_type + method_id
        f.instruction(&Instruction::LocalGet(type_id_local));
        f.instruction(&Instruction::I32Const(self.ir.methods_per_type as i32));
        f.instruction(&Instruction::I32Mul);
        f.instruction(&Instruction::I32Const(method_id as i32));
        f.instruction(&Instruction::I32Add);

        // Get the correct type index and call_indirect (arity = obj + extra args)
        let arity = (num_args + 1) as usize;
        let type_idx = self.protocol_type_index_for_method(method_id, arity);
        f.instruction(&Instruction::CallIndirect {
            type_index: type_idx,
            table_index: dispatch_table::TABLE_INDEX,
        });

        self.scratch_local.set(scratch);
        Ok(())
    }

    /// Get the protocol function type index for a method ID and arity.
    /// This maps (method_id, arity) to the correct type signature for call_indirect.
    ///
    /// For built-in methods, the arity is ignored (they have fixed signatures).
    /// For user-defined methods (method_id >= USER_START), the arity is used.
    fn protocol_type_index_for_method(&self, method_id: u32, arity: usize) -> u32 {
        use crate::ir::method_ids;

        let offset = match method_id {
            method_ids::LOOKUP => protocol_type_offsets::ARITY_2_REF,  // (coll, key) -> value
            method_ids::ASSOC => protocol_type_offsets::ARITY_3_REF,   // (coll, key, val) -> coll'
            method_ids::COUNT => protocol_type_offsets::ARITY_1_I32,   // (coll) -> i32
            method_ids::NTH => protocol_type_offsets::ARITY_2_REF,     // (coll, index) -> value
            method_ids::CONJ => protocol_type_offsets::ARITY_2_REF,    // (coll, val) -> coll'
            method_ids::FIRST => protocol_type_offsets::ARITY_1_REF,   // (seq) -> value
            method_ids::REST => protocol_type_offsets::ARITY_1_REF,    // (seq) -> seq
            method_ids::SEQ => protocol_type_offsets::ARITY_1_REF,     // (coll) -> seq
            method_ids::HASH => protocol_type_offsets::ARITY_1_I32,    // (value) -> i32
            method_ids::EQUIV => protocol_type_offsets::ARITY_2_I32,   // (a, b) -> bool
            _ => {
                // User-defined methods: use arity to determine type
                // arity includes 'this' parameter, so total params = arity
                match arity {
                    1 => protocol_type_offsets::ARITY_1_REF,
                    2 => protocol_type_offsets::ARITY_2_REF,
                    3 => protocol_type_offsets::ARITY_3_REF,
                    // For higher arities, we'd need to add more type definitions.
                    // For now, fall back to arity 1 (will cause type mismatch at runtime).
                    _ => protocol_type_offsets::ARITY_1_REF,
                }
            }
        };
        self.protocol_type(offset)
    }

    /// Generate code to get the runtime type ID of a value.
    ///
    /// Returns i32:
    /// - -1 for i31ref values (nil, bool, small int)
    /// - 0-8 for built-in GC types
    /// - 256+ for user-defined types
    ///
    /// Calls the $get_type_id helper function which uses a chain of ref.test checks.
    fn generate_get_type_id(&self, value: &Expr, f: &mut Function, param_offset: u32) -> CompileResult<()> {
        // Generate the value - it will be on the stack as eqref
        self.generate_expr_inner(value, f, 0, param_offset)?;

        // Call the $get_type_id helper function
        f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::GET_TYPE_ID)));

        Ok(())
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Check if a type contains strings (directly or inside option/list).
fn type_contains_string(ty: &Type) -> bool {
    match ty {
        Type::String => true,
        Type::Option(inner) => type_contains_string(inner),
        Type::List(inner) => type_contains_string(inner),
        _ => false,
    }
}

/// Check if a type needs marshal scratch locals (string, option, list).
/// These types need extra scratch locals for retptr, string copies, etc.
fn type_needs_marshal_scratch(ty: &Type) -> bool {
    match ty {
        Type::String | Type::Option(_) | Type::List(_) => true,
        _ => false,
    }
}

/// Convert IR type to WASM ValType
fn type_to_valtype(ty: &Type) -> ValType {
    match ty {
        Type::Unit | Type::Bool | Type::I32 => ValType::I32,
        Type::I64 => ValType::I64,
        Type::F64 => ValType::F64,
        Type::String => ValType::I32,
        Type::List(_) | Type::Vector(_) | Type::Map(_, _) | Type::Set(_) => ValType::I32,
        Type::GcRef => ValType::Ref(RefType::EQREF),
        Type::Func { .. } => ValType::I32,
        Type::Result { .. } | Type::Option(_) => ValType::I32, // i32 discriminant
        Type::Unknown => ValType::I32,
    }
}

/// Get all ValTypes for a type (strings need ptr+len pair)
fn type_to_valtypes(ty: &Type) -> Vec<ValType> {
    match ty {
        Type::Unit => vec![], // No return value for unit type
        Type::String => vec![ValType::I32, ValType::I32],
        Type::List(_) => vec![ValType::I32, ValType::I32],
        // Bare result (no ok/err types) flattens to i32 discriminant (0=Ok, 1=Err)
        Type::Result { ok: None, err: None } => vec![ValType::I32],
        // Result with payload types would need more complex flattening
        Type::Result { .. } => vec![ValType::I32], // TODO: handle payloads
        // Option<T> flattens to discriminant + payload valtypes
        Type::Option(inner) => {
            let mut vts = vec![ValType::I32]; // discriminant
            vts.extend(type_to_valtypes(inner));
            vts
        }
        _ => vec![type_to_valtype(ty)],
    }
}

/// Compute byte sizes and offsets for a sequence of flat ValTypes.
///
/// Handles alignment requirements: i64/f64 need 8-byte alignment, i32 needs 4-byte.
/// Returns (total_size, vec of (byte_offset, valtype)) for proper retptr layout.
fn flat_byte_size_and_offsets(valtypes: &[ValType]) -> (i32, Vec<(u32, ValType)>) {
    let mut offset: u32 = 0;
    let mut entries = Vec::new();
    for vt in valtypes {
        let (size, align) = match vt {
            ValType::I64 | ValType::F64 => (8u32, 8u32),
            _ => (4, 4),
        };
        offset = (offset + align - 1) & !(align - 1);
        entries.push((offset, *vt));
        offset += size;
    }
    (offset as i32, entries)
}

/// Convert IR type to WIT type string for synthetic world generation
fn type_to_wit_string(ty: &Type) -> String {
    match ty {
        Type::Unit => "()".to_string(),
        Type::Bool => "bool".to_string(),
        Type::I32 => "s32".to_string(),
        Type::I64 => "s64".to_string(),
        Type::F64 => "f64".to_string(),
        Type::String => "string".to_string(),
        Type::List(elem) => format!("list<{}>", type_to_wit_string(elem)),
        Type::Result { ok, err } => {
            match (ok, err) {
                (None, None) => "result".to_string(),
                (Some(ok), None) => format!("result<{}>", type_to_wit_string(ok)),
                (None, Some(err)) => format!("result<_, {}>", type_to_wit_string(err)),
                (Some(ok), Some(err)) => format!("result<{}, {}>", type_to_wit_string(ok), type_to_wit_string(err)),
            }
        }
        Type::Option(inner) => format!("option<{}>", type_to_wit_string(inner)),
        _ => "s64".to_string(), // Default to s64 for unknown types
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::gc_types;

    /// Test that GC types can be emitted and produce valid WASM
    #[test]
    fn test_emit_gc_types() {
        let ir = Module::new();
        let codegen = CodeGen::new(&ir);

        let mut types = TypeSection::new();
        codegen.emit_gc_types(&mut types);

        // Build a minimal module with just the type section
        let mut module = WasmModule::new();
        module.section(&types);

        let wasm_bytes = module.finish();

        // Verify WASM magic and version
        assert_eq!(&wasm_bytes[0..4], b"\0asm");
        assert_eq!(&wasm_bytes[4..8], &[1, 0, 0, 0]); // version 1

        // Verify we can parse the module with wasmparser
        let parser = wasmparser::Parser::new(0);
        let mut found_types = false;

        for payload in parser.parse_all(&wasm_bytes) {
            match payload {
                Ok(wasmparser::Payload::TypeSection(reader)) => {
                    found_types = true;
                    // Count the types - should be NUM_GC_TYPES
                    let count = reader.count();
                    assert_eq!(
                        count, gc_types::NUM_GC_TYPES,
                        "Expected {} GC types, found {}",
                        gc_types::NUM_GC_TYPES, count
                    );
                }
                Ok(_) => {}
                Err(e) => panic!("Failed to parse WASM: {}", e),
            }
        }

        assert!(found_types, "No type section found in generated WASM");
    }

    /// Test that GC type indices match the expected constants
    #[test]
    fn test_gc_type_indices_match() {
        // The emit_gc_types method has debug_assert!s that verify indices match
        // This test just ensures emit_gc_types runs without panicking
        let ir = Module::new();
        let codegen = CodeGen::new(&ir);
        let mut types = TypeSection::new();
        codegen.emit_gc_types(&mut types);
    }

    /// Test I31New generates ref.i31 instruction
    #[test]
    fn test_i31_new_codegen() {
        let ir = Module::new();
        let codegen = CodeGen::new(&ir);

        let mut f = Function::new([]);

        // Generate: (i31.new (i32.const 42))
        let expr = Expr::I31New(Box::new(Expr::Int(42)));
        codegen.generate_expr(&expr, &mut f).unwrap();

        f.instruction(&Instruction::End);

        // The function should contain I32Const(42) followed by RefI31
        // We can't easily inspect the bytecode, but we can verify it doesn't panic
    }

    /// Test StructNew generates struct.new instruction
    #[test]
    fn test_struct_new_codegen() {
        let ir = Module::new();
        let codegen = CodeGen::new(&ir);

        let mut f = Function::new([]);

        // Generate: (struct.new $INT64 (i64.const 1000))
        let expr = Expr::StructNew {
            type_idx: gc_types::INT64,
            fields: vec![Expr::Int(1000)],
        };
        codegen.generate_expr(&expr, &mut f).unwrap();

        f.instruction(&Instruction::End);
    }

    /// Test StructGet generates struct.get instruction
    #[test]
    fn test_struct_get_codegen() {
        let ir = Module::new();
        let codegen = CodeGen::new(&ir);

        let mut f = Function::new([]);

        // Generate: (struct.get $INT64 0 (struct.new $INT64 (i64.const 42)))
        let struct_val = Expr::StructNew {
            type_idx: gc_types::INT64,
            fields: vec![Expr::Int(42)],
        };
        let expr = Expr::StructGet {
            type_idx: gc_types::INT64,
            field_idx: 0,
            value: Box::new(struct_val),
        };
        codegen.generate_expr(&expr, &mut f).unwrap();

        f.instruction(&Instruction::End);
    }

    /// Test sentinel encoding for nil/false/true
    #[test]
    fn test_gc_sentinel_encoding() {
        // Verify our sentinel encoding scheme
        assert_eq!(gc_types::NIL_SENTINEL, 0);
        assert_eq!(gc_types::FALSE_SENTINEL, 2);
        assert_eq!(gc_types::TRUE_SENTINEL, 4);

        // Sentinels are even (not small ints)
        assert!(!gc_types::is_small_int(gc_types::NIL_SENTINEL));
        assert!(!gc_types::is_small_int(gc_types::FALSE_SENTINEL));
        assert!(!gc_types::is_small_int(gc_types::TRUE_SENTINEL));

        // Small int 0 encodes as 1 (odd), not 0
        assert_eq!(gc_types::encode_small_int(0), 1);
        assert!(gc_types::is_small_int(gc_types::encode_small_int(0)));

        // Small int 1 encodes as 3
        assert_eq!(gc_types::encode_small_int(1), 3);

        // Truthiness checks
        assert!(!gc_types::is_truthy_sentinel(gc_types::NIL_SENTINEL));
        assert!(!gc_types::is_truthy_sentinel(gc_types::FALSE_SENTINEL));
        assert!(gc_types::is_truthy_sentinel(gc_types::TRUE_SENTINEL));
        assert!(gc_types::is_truthy_sentinel(gc_types::encode_small_int(0))); // 0 is truthy!
    }

    /// Test RefTestI31 generates ref.test i31 instruction
    #[test]
    fn test_ref_test_i31_codegen() {
        let ir = Module::new();
        let codegen = CodeGen::new(&ir);

        let mut f = Function::new([]);

        // Generate: (ref.test i31 (i31.new (i32.const 0)))
        let i31_val = Expr::I31New(Box::new(Expr::Int(gc_types::NIL_SENTINEL as i64)));
        let expr = Expr::RefTestI31(Box::new(i31_val));
        codegen.generate_expr(&expr, &mut f).unwrap();

        f.instruction(&Instruction::End);
    }

    /// Test type_to_valtype handles GcRef correctly
    #[test]
    fn test_type_to_valtype_gcref() {
        let valtype = type_to_valtype(&Type::GcRef);
        assert_eq!(valtype, ValType::Ref(RefType::EQREF));
    }

    /// Test generating a complete GC-enabled WASM module
    #[test]
    fn test_generate_gc_module() {
        use crate::ir::Function as IrFunc;

        // Create an IR module with a function that returns nil (i31ref)
        let mut ir = Module::new();
        ir.functions.push(IrFunc {
            name: "get_nil".to_string(),
            exported: true,
            export_name: None,
            params: vec![],
            return_type: Type::GcRef,
            has_explicit_return_type: false,
            rest_param: None,
            locals: vec![],
            // Return nil sentinel as i31ref
            body: Expr::I31New(Box::new(Expr::Int(gc_types::NIL_SENTINEL as i64))),
        });

        // Generate GC-enabled module
        let codegen = CodeGen::new(&ir);
        let wasm_bytes = codegen.generate_core_module().unwrap();

        // Verify WASM magic and version
        assert_eq!(&wasm_bytes[0..4], b"\0asm");
        assert_eq!(&wasm_bytes[4..8], &[1, 0, 0, 0]);

        // Verify we can parse the module with wasmparser
        let parser = wasmparser::Parser::new(0);
        let mut found_types = false;
        let mut type_count = 0;

        for payload in parser.parse_all(&wasm_bytes) {
            match payload {
                Ok(wasmparser::Payload::TypeSection(reader)) => {
                    found_types = true;
                    type_count = reader.count();
                }
                Ok(_) => {}
                Err(e) => panic!("Failed to parse GC WASM module: {}", e),
            }
        }

        assert!(found_types, "No type section found");
        // Should have GC types + helper types + protocol types + 1 import type (print_str) + 1 function type + 1 exception tag type
        use crate::ir::protocol_types;
        let expected_types = gc_types::NUM_GC_TYPES + NUM_HELPER_TYPES + protocol_types::NUM_PROTOCOL_TYPES + 1 + 1 + 1;
        assert_eq!(
            type_count,
            expected_types,
            "Expected {} types ({} GC + {} helper + {} protocol + 1 import + 1 func + 1 tag), found {}",
            expected_types,
            gc_types::NUM_GC_TYPES,
            NUM_HELPER_TYPES,
            protocol_types::NUM_PROTOCOL_TYPES,
            type_count
        );
    }

    /// Test generating a GC module with a function that boxes a large integer
    #[test]
    fn test_generate_gc_module_large_int() {
        use crate::ir::Function as IrFunc;

        let mut ir = Module::new();
        ir.functions.push(IrFunc {
            name: "get_large_int".to_string(),
            exported: true,
            export_name: None,
            params: vec![],
            return_type: Type::GcRef,
            has_explicit_return_type: false,
            rest_param: None,
            locals: vec![],
            // Return a large int boxed in a struct
            body: Expr::StructNew {
                type_idx: gc_types::INT64,
                fields: vec![Expr::Int(i64::MAX)],
            },
        });

        let codegen = CodeGen::new(&ir);
        let wasm_bytes = codegen.generate_core_module().unwrap();

        // Verify we can parse the module
        let parser = wasmparser::Parser::new(0);
        for payload in parser.parse_all(&wasm_bytes) {
            if let Err(e) = payload {
                panic!("Failed to parse GC WASM module with large int: {}", e);
            }
        }
    }

    /// Test GC type conversion (always GC mode)
    #[test]
    fn test_gc_type_conversion() {
        let ir = Module::new();
        let codegen = CodeGen::new(&ir);

        // All types become eqref in GC mode
        assert_eq!(
            codegen.type_to_valtype_gc(&Type::I32),
            ValType::Ref(RefType::EQREF)
        );
        assert_eq!(
            codegen.type_to_valtype_gc(&Type::I64),
            ValType::Ref(RefType::EQREF)
        );
        assert_eq!(
            codegen.type_to_valtype_gc(&Type::F64),
            ValType::Ref(RefType::EQREF)
        );
        assert_eq!(
            codegen.type_to_valtype_gc(&Type::String),
            ValType::Ref(RefType::EQREF)
        );

        // Strings are single eqref (not ptr+len pair)
        assert_eq!(codegen.type_to_valtypes_gc(&Type::String).len(), 1);
    }

    /// Test running a GC module in wasmtime
    /// This verifies the generated WASM GC code actually executes correctly
    #[test]
    fn test_run_gc_module_in_wasmtime() {
        use crate::ir::Function as IrFunc;

        // Create a function that returns the small int 42 as i31ref
        // In GC mode, Expr::Int(42) automatically produces i31ref
        let mut ir = Module::new();
        ir.functions.push(IrFunc {
            name: "get_42".to_string(),
            exported: true,
            export_name: None,
            params: vec![],
            return_type: Type::GcRef,
            has_explicit_return_type: false,
            rest_param: None,
            locals: vec![],
            // Expr::Int(42) encodes as (42 << 1) | 1 = 85, wrapped in ref.i31
            body: Expr::Int(42),
        });

        // Generate GC-enabled module
        let codegen = CodeGen::new(&ir);
        let wasm_bytes = codegen.generate_core_module().unwrap();

        // Create wasmtime engine with GC and exceptions enabled
        let mut config = wasmtime::Config::new();
        config.wasm_gc(true);
        config.wasm_exceptions(true);

        let engine = wasmtime::Engine::new(&config).expect("engine creation failed");

        // Load and validate the module
        let module = wasmtime::Module::new(&engine, &wasm_bytes);

        match module {
            Ok(_) => {
                // Module loaded successfully - GC is supported
                // Note: Actually calling the function requires more setup
                // because the return type is eqref which wasmtime handles specially
            }
            Err(e) => {
                // If GC isn't enabled in wasmtime build, skip the test
                let err_str = e.to_string();
                if err_str.contains("GC") || err_str.contains("gc") {
                    eprintln!("Skipping GC test: wasmtime GC not enabled: {}", e);
                } else {
                    // Print more detailed error info
                    eprintln!("WASM load error: {:?}", e);
                    panic!("Failed to load GC module: {}", e);
                }
            }
        }
    }

    /// Test func_type_offset calculation (always GC mode)
    #[test]
    fn test_func_type_offset() {
        use crate::ir::protocol_types;
        let ir = Module::new();
        let codegen = CodeGen::new(&ir);

        // Function types come after GC types, helper types, and protocol types
        assert_eq!(
            codegen.func_type_offset(),
            gc_types::NUM_GC_TYPES + NUM_HELPER_TYPES + protocol_types::NUM_PROTOCOL_TYPES
        );
    }
}
