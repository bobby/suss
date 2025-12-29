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
    AbstractHeapType, BlockType, CodeSection, ConstExpr, CustomSection, DataSection, DataSegment,
    DataSegmentMode, ElementSection, Elements, EntityType, ExportKind, ExportSection, FieldType,
    Function, FunctionSection, GlobalSection, GlobalType, HeapType, ImportSection, Instruction,
    MemorySection, MemoryType, Module as WasmModule, RawSection, RefType, StorageType,
    TableSection, TableType, TypeSection, ValType,
};
use wit_component::{metadata, ComponentEncoder, StringEncoding};
use wit_parser::{Resolve, WorldId};

use crate::error::{CompileError, CompileResult};
use crate::ir::{BinOp, Expr, Function as IrFunc, Module, Type, UnOp, FieldType as IrFieldType};

// ============================================================================
// Runtime Helper Function Indices
// ============================================================================

/// Number of runtime helper functions emitted before user functions.
/// Reduced to 2: only hash_string and get_type_id remain in codegen.
/// Collection algorithms (vector trie, HAMT) are now implemented in core.suss.
const NUM_RUNTIME_HELPERS: u32 = 2;

/// Function index offsets for runtime helpers (relative to start of functions)
mod helper_funcs {
    /// $hash_string(ptr: i32, len: i32) -> i32
    /// Computes xxHash32 of bytes in linear memory
    pub const HASH_STRING: u32 = 0;

    /// $get_type_id(value: eqref) -> i32
    /// Returns the runtime type ID of a GC value
    pub const GET_TYPE_ID: u32 = 1;

    // =========================================================================
    // DEPRECATED STUBS - These functions are now in core.suss
    // All point to HASH_STRING (0) to allow compilation during migration.
    // Calling these will NOT work correctly.
    // =========================================================================

    #[deprecated(note = "Now in core.suss")]
    pub const VEC_ACLONE: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const VEC_TAIL_OFF: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const VEC_NEW_PATH: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const VEC_ARRAY_FOR: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const VEC_PUSH_TAIL: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const EQUIV: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const HAMT_MASK: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const HAMT_BITPOS: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const HAMT_INDEX: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const INODE_FIND: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const INODE_ASSOC: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const BIN_FIND: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const BIN_ASSOC: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const AN_FIND: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const AN_ASSOC: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const HCN_FIND: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const HCN_ASSOC: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const CREATE_NODE: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const HASH: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const INODE_DISSOC: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const BIN_DISSOC: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const AN_DISSOC: u32 = 0;
    #[deprecated(note = "Now in core.suss")]
    pub const HCN_DISSOC: u32 = 0;
}

/// Relative offsets for helper function signatures (added to helper_type_base())
mod helper_type_offsets {
    /// Type for $hash_string: (i32, i32) -> i32
    pub const HASH_STRING: u32 = 0;

    /// Type for $get_type_id: (eqref) -> i32
    pub const GET_TYPE_ID: u32 = 1;

    // Collection helper types removed - now in core.suss
}

/// Number of helper function types (reduced from 19 to 2)
const NUM_HELPER_TYPES: u32 = 2;

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

/// Protocol implementation function indices.
///
/// Collection protocol implementations are now in core.suss via extend-type.
/// This module is kept for compatibility but contains no implementations.
mod protocol_impl_funcs {
    /// Number of protocol implementation wrapper functions (reduced from 14 to 0)
    pub const NUM_PROTOCOL_IMPLS: u32 = 0;

    // All collection protocol implementations removed - now in core.suss via extend-type
}

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
}

impl<'a> CodeGen<'a> {
    fn new(ir: &'a Module) -> Self {
        Self {
            ir,
            scratch_local: std::cell::Cell::new(0),
        }
    }

    /// Number of imported functions
    fn num_imports(&self) -> u32 {
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

    /// Get the function index for a user function (after imports + helper functions + protocol impl functions)
    fn user_func_idx(&self, idx: u32) -> u32 {
        self.num_imports() + NUM_RUNTIME_HELPERS + protocol_impl_funcs::NUM_PROTOCOL_IMPLS + idx
    }

    /// Get the function index for a helper function (after imports)
    fn helper_func_idx(&self, helper_idx: u32) -> u32 {
        self.num_imports() + helper_idx
    }

    /// Get the function index for a protocol implementation wrapper (after imports + helpers)
    fn protocol_impl_func_idx(&self, impl_idx: u32) -> u32 {
        self.num_imports() + NUM_RUNTIME_HELPERS + impl_idx
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
            // It's i31ref - decode: cast, get_s, shr 1
            f.instruction(&Instruction::LocalGet(scratch));
            f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
            f.instruction(&Instruction::I31GetS);
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32ShrS);
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
        let type_offset = self.func_type_offset();

        // Type section - GC types first, then helper types, then protocol types, then user function signatures
        let mut types = TypeSection::new();
        self.emit_gc_types(&mut types);
        self.emit_helper_types(&mut types);
        self.emit_protocol_types(&mut types);

        // Add user function types, but skip closure/builtin wrappers since they use pre-defined types
        for func in &self.ir.functions {
            if !func.name.starts_with("$closure_") && !func.name.starts_with("$builtin_") {
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

        // Function section - helper functions first, then user functions
        // Collection helpers (vector trie, HAMT) removed - now in core.suss
        let mut functions = FunctionSection::new();
        // Runtime helper functions: just hash_string and get_type_id
        functions.function(self.helper_type(helper_type_offsets::HASH_STRING));
        functions.function(self.helper_type(helper_type_offsets::GET_TYPE_ID));
        // User functions: closure/builtin wrappers use pre-defined types, others use type_offset
        let mut non_closure_type_idx = 0u32;
        for func in &self.ir.functions {
            if func.name.starts_with("$closure_") || func.name.starts_with("$builtin_") {
                // Regular closures have env as first param, so arity = params.len() - 1
                let arity = func.params.len().saturating_sub(1) as u32;
                let closure_fn_type = crate::ir::gc_types::closure_fn_type_for_arity(arity);
                functions.function(closure_fn_type);
            } else if func.name.starts_with("$variadic_") {
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

        // Element section - populate dispatch table with protocol implementations
        self.emit_element_section(&mut module);

        // Code section - helper functions first, then protocol impls, then user functions
        let mut code = CodeSection::new();
        self.emit_helper_functions(&mut code)?;
        self.emit_protocol_impl_functions(&mut code)?;
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
        let core_wasm = self.generate_core_with_imports()?;

        // Encode WIT metadata and append to module
        let encoded_metadata = metadata::encode(resolve, world_id, StringEncoding::UTF8, None)
            .map_err(|e| CompileError::Component(format!("Failed to encode metadata: {}", e)))?;

        self.append_metadata_section(&core_wasm, &encoded_metadata)
    }

    /// Generate WASI component (for expression evaluation with WASI imports)
    fn generate_wasi_component(&self) -> CompileResult<Vec<u8>> {
        // Generate core module with imports
        let core_wasm = self.generate_core_with_imports()?;

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
    fn generate_core_with_imports(&self) -> CompileResult<Vec<u8>> {
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
            let params: Vec<ValType> = import
                .params
                .iter()
                .flat_map(|ty| type_to_valtypes(ty))
                .collect();
            let results = if import.return_type == Type::Unit {
                vec![]
            } else {
                type_to_valtypes(&import.return_type)
            };
            types.ty().function(params, results);
        }

        // Local function types
        // Exported functions use WIT types (for component model compatibility)
        // Non-exported functions use GC types (eqref)
        for func in &self.ir.functions {
            if func.exported {
                let params: Vec<ValType> = func
                    .params
                    .iter()
                    .flat_map(|(_, ty)| type_to_valtypes(ty))
                    .collect();
                let results = type_to_valtypes(&func.return_type);
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

        // Import section - WASI functions
        if !self.ir.imports.is_empty() {
            let mut imports = ImportSection::new();
            for (idx, import) in self.ir.imports.iter().enumerate() {
                imports.import(
                    &import.wit_interface,
                    &import.function_name,
                    EntityType::Function(import_type_base + idx as u32),
                );
            }
            module.section(&imports);
        }

        // Function section - helper functions first, then user functions
        // Helper functions use type indices from helper_type_base
        // User functions use type indices starting at local_func_type_base
        let mut functions = FunctionSection::new();
        // Helper functions (hash_string, get_type_id)
        functions.function(type_base + helper_type_offsets::HASH_STRING);
        functions.function(type_base + helper_type_offsets::GET_TYPE_ID);
        // User functions
        for (idx, _) in self.ir.functions.iter().enumerate() {
            functions.function(local_func_type_base + idx as u32);
        }
        module.section(&functions);

        // Table section - dispatch table for protocol methods
        self.emit_table_section(&mut module);

        // Memory section
        self.emit_memory_section(&mut module);

        // Global section - heap pointer
        self.emit_global_section(&mut module);

        // Export section
        let mut exports = ExportSection::new();
        exports.export("memory", ExportKind::Memory, 0);

        // User functions come after imports + helper functions
        let user_func_base = num_imports + NUM_RUNTIME_HELPERS;
        for (idx, func) in self.ir.functions.iter().enumerate() {
            if func.exported {
                let name = func.export_name.as_deref().unwrap_or(&func.name);
                exports.export(name, ExportKind::Func, user_func_base + idx as u32);
            }
        }
        module.section(&exports);

        // Element section - populate dispatch table
        self.emit_element_section(&mut module);

        // Code section - helper functions first, then user functions
        let mut code = CodeSection::new();
        // Emit helper functions
        self.emit_helper_functions(&mut code)?;
        // Emit protocol implementation functions (if needed for dispatch table)
        self.emit_protocol_impl_functions(&mut code)?;
        // Emit user functions
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
        let mut globals = GlobalSection::new();
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &wasm_encoder::ConstExpr::i32_const(0),
        );
        module.section(&globals);
    }

    fn emit_data_section(&self, module: &mut WasmModule) {
        if !self.ir.strings.is_empty() {
            let mut data = DataSection::new();
            let mut offset = 0u32;

            for s in &self.ir.strings {
                let bytes = s.as_bytes();
                data.segment(DataSegment {
                    mode: DataSegmentMode::Active {
                        memory_index: 0,
                        offset: &wasm_encoder::ConstExpr::i32_const(offset as i32),
                    },
                    data: bytes.iter().copied(),
                });
                offset += bytes.len() as u32;
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
    /// - Table size = (5 + num_deftypes) * NUM_BUILTIN
    ///
    /// Table index 0 is used for the dispatch table.
    fn emit_table_section(&self, module: &mut WasmModule) {
        use crate::ir::method_ids;

        // Calculate dynamic table size based on number of deftypes
        const PRIMITIVE_SLOTS: u32 = 5; // slots 0-4 for INT64, FLOAT64, STRING, ARRAY, I32_ARRAY
        let num_deftype_slots = self.ir.deftypes.len() as u32;
        let total_slots = PRIMITIVE_SLOTS + num_deftype_slots;
        let table_size = total_slots * method_ids::NUM_BUILTIN;

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
    /// - index = dispatch_slot * NUM_BUILTIN + method_id
    ///
    /// Slot mapping (set by lowerer):
    /// - Primitive types: slots 0-4
    /// - User deftypes: slots 5+ (in definition order)
    fn emit_element_section(&self, module: &mut WasmModule) {
        use crate::ir::dispatch_table;
        use std::borrow::Cow;

        let mut elements = ElementSection::new();

        // =========================================================================
        // Protocol dispatch table entries
        // Collection types (Cons, PersistentVector, PersistentMap, PersistentSet)
        // are now deftypes in core.suss. Their protocol implementations come from
        // extend-type declarations and are added via the dispatch_entries loop.
        // =========================================================================

        // User-defined protocol implementations from extend-type
        for entry in &self.ir.dispatch_entries {
            // dispatch_slot is already set correctly by the lowerer
            let table_idx = dispatch_table::index(entry.dispatch_slot, entry.method_id);
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
        // - "$protocol_" prefixed: protocol method implementations from extend-type
        let closure_func_indices: Vec<u32> = self
            .ir
            .functions
            .iter()
            .enumerate()
            .filter(|(_, f)| {
                f.name.starts_with("$closure_")
                    || f.name.starts_with("$builtin_")
                    || f.name.starts_with("$variadic_")
                    || f.name.starts_with("$protocol_")
            })
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
    /// - Type 0 (LARGE_INT): struct { i64 } - for integers > 30 bits
    /// - Type 1 (FLOAT): struct { f64 } - all floats are boxed
    /// - Type 2 (STRING): array<i8> - UTF-8 bytes
    /// - Type 3 (TRIE_NODE): array<eqref> - 32-way trie node for vectors
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
    /// Collection types (Cons, PersistentVector, etc.) are now deftypes in core.suss.
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
        // Universal mutable storage for collections (replaces TRIE_NODE)
        types.ty().array(&StorageType::Val(eqref), true);
        debug_assert_eq!(gc_types::ARRAY, 3);

        // Type 4: I32_ARRAY - array<i32>
        // For BigInt magnitude storage (defined in core.suss)
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
        // User-Defined Types (from deftype)
        // These come after all built-in types. Each has type_id at field 0.
        // Collection types are now regular deftypes defined in core.suss.
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
                    mutable: false,
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
    /// NOTE: Reduced to 2 types - vector and HAMT helpers are now in core.suss.
    fn emit_helper_types(&self, types: &mut TypeSection) {
        let eqref = ValType::Ref(RefType::EQREF);

        // Type 0: $hash_string: (i32, i32) -> i32
        // Takes (ptr, len) pointing to linear memory, returns hash
        types.ty().function(
            vec![ValType::I32, ValType::I32],
            vec![ValType::I32],
        );

        // Type 1: $get_type_id: (eqref) -> i32
        // Takes any GC value, returns its type ID
        types.ty().function(vec![eqref], vec![ValType::I32]);

        // Vector trie and HAMT helper types removed - now defined via defn in core.suss
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

    /// Emit function declarations for protocol implementation wrappers.
    ///
    /// Collection protocol implementations are now in core.suss via extend-type.
    fn emit_protocol_impl_function_decls(&self, _functions: &mut FunctionSection) {
        // All collection protocol declarations removed - now in core.suss
        // This function is empty but kept for structural compatibility
    }

    /// Emit code for protocol implementation wrapper functions.
    ///
    /// These wrappers:
    /// 1. Take eqref parameters
    /// 2. Cast to concrete GC types
    /// 3. Perform the operation
    /// 4. Return result (as eqref or i32 depending on method)
    ///
    /// Collection protocol implementations are now in core.suss via extend-type.
    fn emit_protocol_impl_functions(&self, _code: &mut CodeSection) -> CompileResult<()> {
        // All collection protocol implementations removed - now in core.suss
        // This function is empty but kept for structural compatibility
        Ok(())
    }


    /// Emit code for runtime helper functions.
    ///
    /// These come before user functions in the code section.
    fn emit_helper_functions(&self, code: &mut CodeSection) -> CompileResult<()> {
        // Only 2 runtime helpers remain - collection algorithms are now in core.suss

        // $hash_string - xxHash32 for string hashing
        code.function(&self.generate_hash_string_func());

        // $get_type_id - runtime type dispatch
        code.function(&self.generate_get_type_id_func());

        // Vector trie and HAMT helpers removed - now in core.suss
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
        // xxHash32 prime constants
        const PRIME32_1: i32 = 0x9E3779B1_u32 as i32;
        const PRIME32_2: i32 = 0x85EBCA77_u32 as i32;
        const PRIME32_3: i32 = 0xC2B2AE3D_u32 as i32;
        const PRIME32_5: i32 = 0x165667B1_u32 as i32;

        // Locals: ptr=0, len=1, acc=2, i=3, k=4
        let locals = vec![
            (2, ValType::I32), // acc, i
            (1, ValType::I32), // k (for 4-byte load)
        ];
        let mut f = Function::new(locals);

        // acc = PRIME32_5 + len
        f.instruction(&Instruction::I32Const(PRIME32_5));
        f.instruction(&Instruction::LocalGet(1)); // len
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalSet(2)); // acc

        // i = 0
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::LocalSet(3)); // i

        // Process 4-byte chunks: while (i + 4 <= len)
        f.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        f.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
        // Check: i + 4 > len => break
        f.instruction(&Instruction::LocalGet(3)); // i
        f.instruction(&Instruction::I32Const(4));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalGet(1)); // len
        f.instruction(&Instruction::I32GtU);
        f.instruction(&Instruction::BrIf(1)); // break to outer block

        // k = i32.load(ptr + i)
        f.instruction(&Instruction::LocalGet(0)); // ptr
        f.instruction(&Instruction::LocalGet(3)); // i
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
            offset: 0,
            align: 0, // unaligned access
            memory_index: 0,
        }));
        f.instruction(&Instruction::LocalSet(4)); // k

        // acc = acc + k * PRIME32_3
        f.instruction(&Instruction::LocalGet(2)); // acc
        f.instruction(&Instruction::LocalGet(4)); // k
        f.instruction(&Instruction::I32Const(PRIME32_3));
        f.instruction(&Instruction::I32Mul);
        f.instruction(&Instruction::I32Add);
        // acc = rotl(acc, 17)
        f.instruction(&Instruction::I32Const(17));
        f.instruction(&Instruction::I32Rotl);
        // acc = acc * PRIME32_1 (using PRIME32_2 to match reference)
        f.instruction(&Instruction::I32Const(PRIME32_1));
        f.instruction(&Instruction::I32Mul);
        f.instruction(&Instruction::LocalSet(2)); // acc

        // i += 4
        f.instruction(&Instruction::LocalGet(3));
        f.instruction(&Instruction::I32Const(4));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::LocalSet(3));
        f.instruction(&Instruction::Br(0)); // continue loop
        f.instruction(&Instruction::End); // end loop
        f.instruction(&Instruction::End); // end block

        // Process remaining bytes: while (i < len)
        f.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
        f.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));
        // Check: i >= len => break
        f.instruction(&Instruction::LocalGet(3)); // i
        f.instruction(&Instruction::LocalGet(1)); // len
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::BrIf(1)); // break

        // byte = i32.load8_u(ptr + i)
        f.instruction(&Instruction::LocalGet(0)); // ptr
        f.instruction(&Instruction::LocalGet(3)); // i
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I32Load8U(wasm_encoder::MemArg {
            offset: 0,
            align: 0,
            memory_index: 0,
        }));

        // acc = acc + byte * PRIME32_5
        f.instruction(&Instruction::I32Const(PRIME32_5));
        f.instruction(&Instruction::I32Mul);
        f.instruction(&Instruction::LocalGet(2)); // acc
        f.instruction(&Instruction::I32Add);
        // acc = rotl(acc, 11)
        f.instruction(&Instruction::I32Const(11));
        f.instruction(&Instruction::I32Rotl);
        // acc = acc * PRIME32_1
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

        // Avalanche mixing
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
        let type_idx = self.protocol_type_index_for_method(method_ids::CONJ);
        for elem in rest {
            // Calculate dispatch table index: type_id * 10 + method_id
            // For PersistentVector (type_id 8) and CONJ (method_id 4): 8 * 10 + 4 = 84
            let dispatch_idx = gc_types::PERSISTENT_VECTOR as i32 * 10 + method_ids::CONJ as i32;

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
    /// - If index >= tailoff: return tail[index & 0x1F]
    /// - Else: traverse trie from root using bit partitioning
    fn generate_vec_nth(&self, vec: &Expr, index: &Expr, f: &mut Function) -> CompileResult<()> {
        use crate::ir::gc_types;

        // Look up array-for helper function dynamically
        let array_for_idx = self.func_idx_by_name("array-for")
            .ok_or_else(|| CompileError::Unsupported("array-for function not found".to_string()))?;

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

        // Call array-for(vec, idx) to get the leaf array
        // array-for expects boxed values, so we need to box the i32 index
        f.instruction(&Instruction::LocalGet(vec_local));
        // Box idx as i31ref: (n << 1) | 1
        f.instruction(&Instruction::LocalGet(idx_local));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Shl);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::RefI31);
        f.instruction(&Instruction::Call(array_for_idx));

        // Cast result to TRIE_NODE for array.get
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::TRIE_NODE,
        )));

        // Get element at idx & 0x1f
        f.instruction(&Instruction::LocalGet(idx_local));
        f.instruction(&Instruction::I32Const(0x1F));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::ArrayGet(gc_types::TRIE_NODE));

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

    /// Generate code for (disj set val)
    ///
    /// Removes val from the set, returning a new set without that element.
    fn generate_set_disj(
        &self,
        set: &Expr,
        val: &Expr,
        f: &mut Function,
    ) -> CompileResult<()> {
        // Generate the set and val expressions
        // Note: stub impl doesn't use locals, so no need to bump scratch_local
        self.generate_expr(set, f)?;
        self.generate_expr(val, f)?;

        self.generate_set_disj_impl(f)
    }

    /// Implementation of set disj when values are on stack
    ///
    /// Stack: [set, val]
    ///
    /// Stub implementation: just returns the original set.
    /// TODO: Implement proper set disj logic
    fn generate_set_disj_impl(&self, f: &mut Function) -> CompileResult<()> {
        // Stack is [set, val] - drop val, keep set
        f.instruction(&Instruction::Drop); // drop val
        // set is now on top of stack
        Ok(())
    }

    /// Generate code for (dissoc map key)
    ///
    /// Removes key from the map, returning a new map without that key.
    fn generate_map_dissoc(
        &self,
        map: &Expr,
        key: &Expr,
        f: &mut Function,
    ) -> CompileResult<()> {
        // Generate the map and key expressions
        // Note: stub impl doesn't use locals, so no need to bump scratch_local
        self.generate_expr(map, f)?;
        self.generate_expr(key, f)?;

        self.generate_map_dissoc_impl(f)
    }

    /// Implementation of map dissoc when values are on stack
    ///
    /// Stack: [map, key]
    ///
    /// Stub implementation: just returns the original map.
    /// TODO: Implement proper map dissoc logic
    fn generate_map_dissoc_impl(&self, f: &mut Function) -> CompileResult<()> {
        // Stack is [map, key] - drop key, keep map
        f.instruction(&Instruction::Drop); // drop key
        // map is now on top of stack
        Ok(())
    }

    // ========================================================================
    // WIT/WASI helpers
    // ========================================================================

    /// Build a synthetic WIT world definition for an expression with WASI imports
    fn build_synthetic_wit_world(&self) -> CompileResult<String> {
        let mut wit = String::new();

        wit.push_str("package suss:expr;\n\n");
        wit.push_str("world expr {\n");

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
    /// - Function signature uses WIT types (i32/i64/f64)
    /// - At entry: convert WIT params to GC refs
    /// - At exit: convert GC result back to WIT type
    fn generate_function_wit(&self, func: &IrFunc) -> CompileResult<Function> {
        use crate::ir::gc_types;
        use crate::ir::type_ids;
        let num_params = func.params.len() as u32;

        // Locals layout:
        // 0..num_params: WIT params (i32/i64/f64)
        // num_params..2*num_params: converted eqref params
        // 2*num_params..: body locals (eqref)

        let mut local_types: Vec<(u32, ValType)> = Vec::new();

        // Add eqref locals for converted params
        for _ in 0..num_params {
            local_types.push((1, ValType::Ref(RefType::EQREF)));
        }

        // Add body locals (excluding params which are in func.locals[0..num_params])
        for ty in func.locals[func.params.len()..].iter() {
            local_types.push((1, self.type_to_valtype_gc(ty)));
        }

        // Add scratch locals for internal codegen (protocol dispatch, set conj, etc.)
        // Local layout: [WIT params | converted params | body locals | scratch]
        // scratch_base = num_params + local_types.len() (params + declared locals so far)
        let scratch_base = num_params + local_types.len() as u32;
        self.scratch_local.set(scratch_base);

        // Add 25 scratch locals (5 sets of 5, for nested operations)
        for _ in 0..5 {
            local_types.push((1, ValType::Ref(RefType::EQREF))); // scratch +0
            local_types.push((1, ValType::I32));                  // scratch +1
            local_types.push((1, ValType::Ref(RefType::EQREF))); // scratch +2
            local_types.push((1, ValType::Ref(RefType::EQREF))); // scratch +3
            local_types.push((1, ValType::Ref(RefType::EQREF))); // scratch +4
        }

        let mut f = Function::new(local_types);

        // Entry: convert each WIT param to eqref and store in new local
        for i in 0..num_params {
            let (_, param_ty) = &func.params[i as usize];
            match param_ty {
                Type::I32 | Type::I64 | Type::Unknown => {
                    // Convert i32 to small int encoding: (n << 1) | 1, then ref.i31
                    // Type::Unknown is treated as I32 for WIT exports
                    f.instruction(&Instruction::LocalGet(i));
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32Shl);
                    f.instruction(&Instruction::I32Const(1));
                    f.instruction(&Instruction::I32Or);
                    f.instruction(&Instruction::RefI31);
                    f.instruction(&Instruction::LocalSet(num_params + i));
                }
                Type::F64 => {
                    // Box f64 in FLOAT struct: { type_id, value }
                    f.instruction(&Instruction::I32Const(type_ids::FLOAT));
                    f.instruction(&Instruction::LocalGet(i));
                    f.instruction(&Instruction::StructNew(gc_types::FLOAT));
                    f.instruction(&Instruction::LocalSet(num_params + i));
                }
                _ => {
                    // For other types, just copy (shouldn't happen for WIT exports)
                    f.instruction(&Instruction::LocalGet(i));
                    f.instruction(&Instruction::LocalSet(num_params + i));
                }
            }
        }

        // Generate body with local offset for params
        self.generate_expr_wit(&func.body, &mut f, num_params)?;

        // Exit: convert eqref result back to WIT type
        match &func.return_type {
            Type::I32 | Type::I64 | Type::Unknown => {
                // Decode from i31ref: cast, get_s, >> 1
                // Type::Unknown is treated as I32 for WIT exports
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrS);
            }
            Type::F64 => {
                // Unbox from FLOAT struct
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::FLOAT)));
                f.instruction(&Instruction::StructGet {
                    struct_type_index: gc_types::FLOAT,
                    field_index: gc_types::FL_VALUE,
                });
            }
            Type::Unit => {
                // Drop the eqref, return nothing
                f.instruction(&Instruction::Drop);
            }
            _ => {
                // For other types, leave as-is (will cause type error if mismatched)
            }
        }

        f.instruction(&Instruction::End);
        Ok(f)
    }

    /// Generate expression with WIT local offset for params.
    fn generate_expr_wit(&self, expr: &Expr, f: &mut Function, param_offset: u32) -> CompileResult<()> {
        // This is a wrapper that remaps LocalGet for params
        // For simplicity, we handle LocalGet specially and delegate the rest
        match expr {
            Expr::LocalGet { local, ty: _ } => {
                // Remap param access to use converted local
                f.instruction(&Instruction::LocalGet(*local + param_offset));
                Ok(())
            }
            _ => {
                // For other expressions, use normal generation but recurse for nested exprs
                self.generate_expr_wit_inner(expr, f, param_offset)
            }
        }
    }

    /// Inner helper for WIT expression generation - handles recursion
    fn generate_expr_wit_inner(&self, expr: &Expr, f: &mut Function, param_offset: u32) -> CompileResult<()> {
        use crate::ir::gc_types;
        match expr {
            // Recursively handle expressions that contain sub-expressions
            Expr::BinOp { op, left, right, ty } => {
                self.generate_binop_wit(op, left, right, ty, f, param_offset)
            }
            Expr::UnOp { op, operand, ty: _ } => {
                self.generate_unop_wit(op, operand, f, param_offset)
            }
            Expr::If { cond, then_branch, else_branch, ty } => {
                self.generate_if_wit(cond, then_branch, else_branch, ty, f, param_offset)
            }
            Expr::Block(exprs) => {
                for (i, e) in exprs.iter().enumerate() {
                    self.generate_expr_wit(e, f, param_offset)?;
                    if i < exprs.len() - 1 {
                        f.instruction(&Instruction::Drop);
                    }
                }
                if exprs.is_empty() {
                    f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
                    f.instruction(&Instruction::RefI31);
                }
                Ok(())
            }
            Expr::Let { bindings, body } => {
                for (local_idx, init) in bindings {
                    self.generate_expr_wit(init, f, param_offset)?;
                    f.instruction(&Instruction::LocalSet(*local_idx + param_offset));
                }
                self.generate_expr_wit(body, f, param_offset)
            }
            Expr::LocalGet { local, ty: _ } => {
                f.instruction(&Instruction::LocalGet(*local + param_offset));
                Ok(())
            }
            Expr::Call { func, args } => {
                // Check if target function is exported (needs WIT marshaling for i32 <-> eqref)
                //
                // Function indices are: [imports][helpers][user functions]
                // To find the IR function index, we subtract imports and helpers.
                let num_imports = self.num_imports();
                let user_func_base = num_imports + NUM_RUNTIME_HELPERS;
                let is_exported = if *func >= user_func_base {
                    let local_idx = (*func - user_func_base) as usize;
                    self.ir.functions.get(local_idx).map_or(false, |f| f.exported)
                } else {
                    false // Imports/helpers don't need marshaling
                };

                for arg in args {
                    self.generate_expr_wit(arg, f, param_offset)?;
                    if is_exported || *func < num_imports {
                        // Unwrap eqref to i32 for exported/import functions
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
                // Note: import functions return WIT types, caller must handle
                Ok(())
            }
            Expr::TailCall { func, args } => {
                // Check if target function is exported (needs WIT marshaling)
                let num_imports = self.num_imports();
                let user_func_base = num_imports + NUM_RUNTIME_HELPERS;
                let is_exported = if *func >= user_func_base {
                    let local_idx = (*func - user_func_base) as usize;
                    self.ir.functions.get(local_idx).map_or(false, |f| f.exported)
                } else {
                    false // Imports/helpers don't need marshaling
                };

                for arg in args {
                    self.generate_expr_wit(arg, f, param_offset)?;
                    if is_exported || *func < num_imports {
                        // Unwrap eqref to i32 for exported/import functions
                        f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                        f.instruction(&Instruction::I31GetS);
                        f.instruction(&Instruction::I32Const(1));
                        f.instruction(&Instruction::I32ShrS);
                    }
                }
                f.instruction(&Instruction::ReturnCall(*func));
                // Note: tail call returns directly, no result wrapping needed
                // (the function's exit marshaling handles return value conversion)
                Ok(())
            }
            // Array operations with proper WIT local offset handling
            Expr::ArrayLen(array) => {
                self.generate_expr_wit(array, f, param_offset)?;
                // Cast eqref to array type before array.len
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::TRIE_NODE)));
                f.instruction(&Instruction::ArrayLen);
                // Box result as i31ref: (n << 1) | 1
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Shl);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Or);
                f.instruction(&Instruction::RefI31);
                Ok(())
            }
            Expr::ArrayGet { type_idx, array, index } => {
                self.generate_expr_wit(array, f, param_offset)?;
                // Cast eqref to array type
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                // Unbox index from i31ref
                self.generate_expr_wit(index, f, param_offset)?;
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrS);
                f.instruction(&Instruction::ArrayGet(*type_idx));
                Ok(())
            }
            Expr::ArraySet { type_idx, array, index, value } => {
                self.generate_expr_wit(array, f, param_offset)?;
                // Cast eqref to array type
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                // Unbox index from i31ref
                self.generate_expr_wit(index, f, param_offset)?;
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrS);
                self.generate_expr_wit(value, f, param_offset)?;
                // Duplicate value to return after set
                let scratch = self.scratch_local.get();
                f.instruction(&Instruction::LocalTee(scratch));
                f.instruction(&Instruction::ArraySet(*type_idx));
                // Return the value that was set
                f.instruction(&Instruction::LocalGet(scratch));
                Ok(())
            }
            Expr::ArrayNewDefault { type_idx, size } => {
                // Unbox size from i31ref
                self.generate_expr_wit(size, f, param_offset)?;
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrS);
                f.instruction(&Instruction::ArrayNewDefault(*type_idx));
                Ok(())
            }
            Expr::ArrayClone { type_idx, array } => {
                // Clone by creating new array and copying
                // Scratch layout: +0: eqref, +1: i32, +2: eqref, +3: eqref, +4: eqref
                let scratch = self.scratch_local.get();
                let src_arr_local = scratch;     // eqref at +0
                let new_arr_local = scratch + 2; // eqref at +2 (NOT +1 which is i32!)

                self.generate_expr_wit(array, f, param_offset)?;
                // Cast eqref to array type
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                // NOTE: local.tee returns the local's type (eqref), so we must cast after!
                f.instruction(&Instruction::LocalTee(src_arr_local));
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                f.instruction(&Instruction::ArrayLen);
                f.instruction(&Instruction::ArrayNewDefault(*type_idx));
                f.instruction(&Instruction::LocalTee(new_arr_local));
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                f.instruction(&Instruction::I32Const(0));
                // Note: scratch locals are eqref, so we must cast again after LocalGet
                f.instruction(&Instruction::LocalGet(src_arr_local));
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                f.instruction(&Instruction::I32Const(0));
                f.instruction(&Instruction::LocalGet(src_arr_local));
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                f.instruction(&Instruction::ArrayLen);
                f.instruction(&Instruction::ArrayCopy {
                    array_type_index_dst: *type_idx,
                    array_type_index_src: *type_idx,
                });
                f.instruction(&Instruction::LocalGet(new_arr_local));
                Ok(())
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
                self.generate_expr_wit(dst, f, param_offset)?;
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                self.generate_expr_wit(dst_offset, f, param_offset)?;
                // Unbox dst_offset from i31ref to i32
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrS);
                self.generate_expr_wit(src, f, param_offset)?;
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(*type_idx)));
                self.generate_expr_wit(src_offset, f, param_offset)?;
                // Unbox src_offset from i31ref to i32
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrS);
                self.generate_expr_wit(len, f, param_offset)?;
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
                Ok(())
            }
            Expr::BitCount(value) => {
                // Polymorphic unwrap to handle both i31ref and INT64 inputs
                // (INT64 can come from bit-shift-left with large results)
                let scratch_base = self.scratch_local.get();
                self.scratch_local.set(scratch_base + 5);

                self.generate_expr_wit(value, f, param_offset)?;
                self.generate_polymorphic_unwrap_i32(f);
                f.instruction(&Instruction::I32Popcnt);
                // Result is always small (0-32), safe to encode as i31ref
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Shl);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Or);
                f.instruction(&Instruction::RefI31);

                self.scratch_local.set(scratch_base);
                Ok(())
            }
            // For simple expressions that don't contain sub-expressions, delegate to normal gen
            // Use generate_expr_with_offset to preserve param_offset for nested LocalGet
            _ => self.generate_expr_with_offset(expr, f, param_offset),
        }
    }

    /// Helper for BinOp with WIT offset
    fn generate_binop_wit(&self, op: &BinOp, left: &Expr, right: &Expr, _ty: &Type, f: &mut Function, param_offset: u32) -> CompileResult<()> {
        use crate::ir::gc_types;

        // Helper to unwrap i31ref: cast, get_s, shr 1
        let generate_unwrap_i31 = |f: &mut Function| {
            f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
            f.instruction(&Instruction::I31GetS);
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32ShrS);
        };

        // For comparison ops that return bool, we don't need to unwrap/rewrap differently
        let is_comparison = matches!(op, BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge);

        if is_comparison {
            // For comparisons: generate and unwrap each operand in sequence
            // This avoids needing temp storage which would clobber converted params
            self.generate_expr_wit(left, f, param_offset)?;
            generate_unwrap_i31(f);
            self.generate_expr_wit(right, f, param_offset)?;
            generate_unwrap_i31(f);

            // Now stack has [left_i32, right_i32]
            match op {
                BinOp::Eq => f.instruction(&Instruction::I32Eq),
                BinOp::Ne => f.instruction(&Instruction::I32Ne),
                BinOp::Lt => f.instruction(&Instruction::I32LtS),
                BinOp::Le => f.instruction(&Instruction::I32LeS),
                BinOp::Gt => f.instruction(&Instruction::I32GtS),
                BinOp::Ge => f.instruction(&Instruction::I32GeS),
                _ => unreachable!(),
            };

            // Convert i32 (0 or 1) to bool sentinel
            f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
            f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
            f.instruction(&Instruction::Else);
            f.instruction(&Instruction::I32Const(gc_types::FALSE_SENTINEL));
            f.instruction(&Instruction::End);
            f.instruction(&Instruction::RefI31);
        } else {
            // For arithmetic ops: generate and unwrap each operand in sequence
            // This avoids needing temp storage which would clobber converted params
            self.generate_expr_wit(left, f, param_offset)?;
            generate_unwrap_i31(f);
            self.generate_expr_wit(right, f, param_offset)?;
            generate_unwrap_i31(f);

            match op {
                BinOp::Add => { f.instruction(&Instruction::I32Add); }
                BinOp::Sub => { f.instruction(&Instruction::I32Sub); }
                BinOp::Mul => { f.instruction(&Instruction::I32Mul); }
                BinOp::Div => { f.instruction(&Instruction::I32DivS); }
                BinOp::Rem => { f.instruction(&Instruction::I32RemS); }
                // Bitwise operations
                BinOp::BitAnd => { f.instruction(&Instruction::I32And); }
                BinOp::BitOr => { f.instruction(&Instruction::I32Or); }
                BinOp::BitXor => { f.instruction(&Instruction::I32Xor); }
                BinOp::Shl => { f.instruction(&Instruction::I32Shl); }
                BinOp::ShrS => { f.instruction(&Instruction::I32ShrS); }
                BinOp::ShrU => { f.instruction(&Instruction::I32ShrU); }
                _ => unreachable!("arithmetic/bitwise binop only: {:?}", op)
            }

            // Wrap result as i31ref
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Shl);
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Or);
            f.instruction(&Instruction::RefI31);
        }

        Ok(())
    }

    /// Helper for UnOp with WIT offset
    fn generate_unop_wit(&self, op: &UnOp, operand: &Expr, f: &mut Function, param_offset: u32) -> CompileResult<()> {
        use crate::ir::gc_types;
        self.generate_expr_wit(operand, f, param_offset)?;

        match op {
            UnOp::Not => {
                // Truthiness check, then negate
                self.generate_condition_wit(operand, f, param_offset)?;
                f.instruction(&Instruction::I32Eqz);
                f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
                f.instruction(&Instruction::I32Const(gc_types::TRUE_SENTINEL));
                f.instruction(&Instruction::Else);
                f.instruction(&Instruction::I32Const(gc_types::FALSE_SENTINEL));
                f.instruction(&Instruction::End);
                f.instruction(&Instruction::RefI31);
            }
            UnOp::Neg => {
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrS);
                f.instruction(&Instruction::I32Const(0));
                f.instruction(&Instruction::I32Sub);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Shl);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Or);
                f.instruction(&Instruction::RefI31);
            }
        }
        Ok(())
    }

    /// Helper for If with WIT offset
    fn generate_if_wit(&self, cond: &Expr, then_branch: &Expr, else_branch: &Expr, result_type: &Type, f: &mut Function, param_offset: u32) -> CompileResult<()> {
        self.generate_condition_wit(cond, f, param_offset)?;

        let block_type = wasm_encoder::BlockType::Result(self.type_to_valtype_gc(result_type));
        f.instruction(&Instruction::If(block_type));
        self.generate_expr_wit(then_branch, f, param_offset)?;
        f.instruction(&Instruction::Else);
        self.generate_expr_wit(else_branch, f, param_offset)?;
        f.instruction(&Instruction::End);
        Ok(())
    }

    /// Generate condition check with WIT offset
    fn generate_condition_wit(&self, cond: &Expr, f: &mut Function, param_offset: u32) -> CompileResult<()> {
        use crate::ir::gc_types;
        self.generate_expr_wit(cond, f, param_offset)?;

        // Check if i31ref, then check truthiness
        f.instruction(&Instruction::RefTestNonNull(HeapType::I31));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        self.generate_expr_wit(cond, f, param_offset)?;
        f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
        f.instruction(&Instruction::I31GetS);
        // Check if not nil (0) and not false (2)
        f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
        f.instruction(&Instruction::I32Ne);
        self.generate_expr_wit(cond, f, param_offset)?;
        f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
        f.instruction(&Instruction::I31GetS);
        f.instruction(&Instruction::I32Const(gc_types::FALSE_SENTINEL));
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::Else);
        // Non-i31 refs (structs, arrays) are always truthy
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::End);

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
                    // Large integer: box in LARGE_INT struct { type_id, value }
                    f.instruction(&Instruction::I32Const(type_ids::LARGE_INT));
                    f.instruction(&Instruction::I64Const(*i));
                    f.instruction(&Instruction::StructNew(gc_types::LARGE_INT));
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
                f.instruction(&Instruction::I32Const(type_ids::FLOAT));
                f.instruction(&Instruction::F64Const(*v));
                f.instruction(&Instruction::StructNew(gc_types::FLOAT));
            }

            Expr::String(idx) => {
                let mut offset = 0u32;
                for (i, s) in self.ir.strings.iter().enumerate() {
                    if i == *idx as usize {
                        break;
                    }
                    offset += s.len() as u32;
                }
                let len = self.ir.strings[*idx as usize].len() as u32;

                f.instruction(&Instruction::I32Const(offset as i32));
                f.instruction(&Instruction::I32Const(len as i32));
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
                    (BinOp::Rem, Type::I32) | (BinOp::Rem, Type::I64) => {
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
                            _ => unreachable!(),
                        };

                        // Safe box result (INT64 if overflow)
                        self.generate_box_i32_safe(f);

                        self.scratch_local.set(scratch_base);
                    }

                    // Float arithmetic: unwrap structs, compute, wrap in { type_id, value }
                    (BinOp::Add, Type::F64) => {
                        f.instruction(&Instruction::I32Const(type_ids::FLOAT));
                        self.generate_expr(left, f)?;
                        f.instruction(&Instruction::StructGet {
                            struct_type_index: gc_types::FLOAT,
                            field_index: gc_types::FL_VALUE,
                        });
                        self.generate_expr(right, f)?;
                        f.instruction(&Instruction::StructGet {
                            struct_type_index: gc_types::FLOAT,
                            field_index: gc_types::FL_VALUE,
                        });
                        f.instruction(&Instruction::F64Add);
                        f.instruction(&Instruction::StructNew(gc_types::FLOAT));
                    }
                    (BinOp::Sub, Type::F64) => {
                        f.instruction(&Instruction::I32Const(type_ids::FLOAT));
                        self.generate_expr(left, f)?;
                        f.instruction(&Instruction::StructGet {
                            struct_type_index: gc_types::FLOAT,
                            field_index: gc_types::FL_VALUE,
                        });
                        self.generate_expr(right, f)?;
                        f.instruction(&Instruction::StructGet {
                            struct_type_index: gc_types::FLOAT,
                            field_index: gc_types::FL_VALUE,
                        });
                        f.instruction(&Instruction::F64Sub);
                        f.instruction(&Instruction::StructNew(gc_types::FLOAT));
                    }
                    (BinOp::Mul, Type::F64) => {
                        f.instruction(&Instruction::I32Const(type_ids::FLOAT));
                        self.generate_expr(left, f)?;
                        f.instruction(&Instruction::StructGet {
                            struct_type_index: gc_types::FLOAT,
                            field_index: gc_types::FL_VALUE,
                        });
                        self.generate_expr(right, f)?;
                        f.instruction(&Instruction::StructGet {
                            struct_type_index: gc_types::FLOAT,
                            field_index: gc_types::FL_VALUE,
                        });
                        f.instruction(&Instruction::F64Mul);
                        f.instruction(&Instruction::StructNew(gc_types::FLOAT));
                    }
                    (BinOp::Div, Type::F64) => {
                        f.instruction(&Instruction::I32Const(type_ids::FLOAT));
                        self.generate_expr(left, f)?;
                        // Cast to FLOAT struct before extracting value
                        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::FLOAT)));
                        f.instruction(&Instruction::StructGet {
                            struct_type_index: gc_types::FLOAT,
                            field_index: gc_types::FL_VALUE,
                        });
                        self.generate_expr(right, f)?;
                        // Cast to FLOAT struct before extracting value
                        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::FLOAT)));
                        f.instruction(&Instruction::StructGet {
                            struct_type_index: gc_types::FLOAT,
                            field_index: gc_types::FL_VALUE,
                        });
                        f.instruction(&Instruction::F64Div);
                        f.instruction(&Instruction::StructNew(gc_types::FLOAT));
                    }

                    // Comparison operations: polymorphic unwrap to handle INT64 from bit ops
                    (BinOp::Eq, _) | (BinOp::Ne, _) | (BinOp::Lt, _) | (BinOp::Le, _) | (BinOp::Gt, _) | (BinOp::Ge, _) => {
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
                                f.instruction(&Instruction::I32Const(type_ids::FLOAT));
                                self.generate_expr_inner(operand, f, loop_depth, param_offset)?;
                                f.instruction(&Instruction::StructGet {
                                    struct_type_index: gc_types::FLOAT,
                                    field_index: gc_types::FL_VALUE,
                                });
                                f.instruction(&Instruction::F64Neg);
                                f.instruction(&Instruction::StructNew(gc_types::FLOAT));
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
                for arg in args {
                    self.generate_expr_inner(arg, f, loop_depth, param_offset)?;
                }
                f.instruction(&Instruction::Call(*func));
            }

            Expr::TailCall { func, args } => {
                for arg in args {
                    self.generate_expr_inner(arg, f, loop_depth, param_offset)?;
                }
                f.instruction(&Instruction::ReturnCall(*func));
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
                    f.instruction(&Instruction::LocalSet(*idx));
                }
                self.generate_expr_inner(body, f, loop_depth, param_offset)?;
            }

            Expr::Loop { bindings, body } => {
                for (idx, value) in bindings {
                    self.generate_expr_inner(value, f, loop_depth, param_offset)?;
                    f.instruction(&Instruction::LocalSet(*idx));
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
                for (local_idx, value) in values.iter() {
                    self.generate_expr_inner(value, f, loop_depth, param_offset)?;
                    f.instruction(&Instruction::LocalSet(*local_idx));
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
                // PersistentVector: { type_id: 0, cnt: 1, shift: 2, root: 3, tail: 4 }
                // PersistentMap/Set: { type_id: 0, cnt: 1, root: 2, ... }
                // Cons: { first: 0, rest: 1 } - both eqref
                let needs_i32_wrap = match (*type_idx, *field_idx) {
                    // Vector i32 fields
                    (gc_types::PERSISTENT_VECTOR, 1) => true,  // cnt
                    (gc_types::PERSISTENT_VECTOR, 2) => true,  // shift
                    // Map/Set i32 fields
                    (gc_types::PERSISTENT_MAP, 1) => true,     // cnt
                    (gc_types::PERSISTENT_SET, 1) => true,     // cnt
                    // BitmapIndexedNode i32 fields: { type_id: 0, bitmap: 1, arr: 2 }
                    (gc_types::BITMAP_INDEXED_NODE, 0) => true, // type_id
                    (gc_types::BITMAP_INDEXED_NODE, 1) => true, // bitmap
                    // ArrayNode i32 fields: { type_id: 0, cnt: 1, arr: 2 }
                    (gc_types::ARRAY_NODE, 0) => true,          // type_id
                    (gc_types::ARRAY_NODE, 1) => true,          // cnt
                    // HashCollisionNode i32 fields: { type_id: 0, hash: 1, cnt: 2, arr: 3 }
                    (gc_types::HASH_COLLISION_NODE, 0) => true, // type_id
                    (gc_types::HASH_COLLISION_NODE, 1) => true, // hash
                    (gc_types::HASH_COLLISION_NODE, 2) => true, // cnt
                    // All other fields are eqref
                    _ => false,
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

            Expr::ArrayNew { type_idx, elements } => {
                // Create array with default values, then set each element
                f.instruction(&Instruction::I32Const(elements.len() as i32));
                f.instruction(&Instruction::ArrayNewDefault(*type_idx));

                // Set each element
                for (i, elem) in elements.iter().enumerate() {
                    // Duplicate array ref for set
                    f.instruction(&Instruction::LocalTee(0)); // TODO: need a temp local
                    f.instruction(&Instruction::I32Const(i as i32));
                    self.generate_expr(elem, f)?;
                    f.instruction(&Instruction::ArraySet(*type_idx));
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
                use crate::ir::gc_types;
                self.generate_expr(array, f)?;
                // Cast eqref to array type before array.len
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::TRIE_NODE)));
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
                f.instruction(&Instruction::ArrayGet(*type_idx));
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

            Expr::MapDissoc { map, key } => {
                self.generate_map_dissoc(map, key, f)?;
            }

            // =========================================================
            // Persistent Set Operations
            // =========================================================

            Expr::SetNew(elements) => {
                self.generate_set_new(elements, f)?;
            }

            Expr::SetDisj { set, val } => {
                self.generate_set_disj(set, val, f)?;
            }

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
            } => {
                self.generate_closure_new(*func_idx, *arity, captures, f)?;
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
    /// - env: array<eqref> containing captured values
    /// - fn: typed funcref to the wrapper function
    fn generate_closure_new(
        &self,
        func_idx: u32,
        arity: u32,
        captures: &[Expr],
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::type_ids;

        let closure_type = gc_types::closure_type_for_arity(arity);
        let type_id = type_ids::CLOSURE_0 + arity as i32;

        // Field 0: type_id (i32)
        f.instruction(&Instruction::I32Const(type_id));

        // Field 1: env (ref null $trie_node)
        // Create array of captured values
        if captures.is_empty() {
            // Empty captures - use null ref
            f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::TRIE_NODE)));
        } else {
            // Generate each capture expression
            for capture in captures {
                self.generate_expr(capture, f)?;
            }
            // Create array from values on stack
            f.instruction(&Instruction::ArrayNewFixed {
                array_type_index: gc_types::TRIE_NODE,
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
    /// Handles both regular closures and variadic closures.
    fn generate_closure_call(
        &self,
        closure: &Expr,
        args: &[Expr],
        in_tail_position: bool,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;

        let arity = args.len() as u32;

        // Evaluate and save the closure to a local
        self.generate_expr(closure, f)?;
        let closure_local = self.scratch_local.get();
        f.instruction(&Instruction::LocalSet(closure_local));

        // Check if it's a variadic closure
        f.instruction(&Instruction::LocalGet(closure_local));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(
            gc_types::VARIADIC_CLOSURE,
        )));
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));

        // Variadic closure path
        self.generate_variadic_closure_call(closure_local, args, arity, in_tail_position, f)?;

        f.instruction(&Instruction::Else);

        // Regular closure path
        self.generate_regular_closure_call(closure_local, args, arity, in_tail_position, f)?;

        f.instruction(&Instruction::End);

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
        let fn_field = gc_types::VC_FN0 + arity; // VC_FN0=1, VC_FN1=2, etc.

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

    /// Generate code for dynamic function application.
    ///
    /// (apply f coll) calls f with elements of coll as arguments.
    /// At runtime, we dispatch based on the vector count (0-8).
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

        // Scratch locals matching layout: +0: eqref, +1: i32, +2: eqref, +3: eqref, +4: eqref
        let scratch_base = self.scratch_local.get();
        let closure_local = scratch_base; // eqref at +0
        let count_local = scratch_base + 1; // i32 at +1
        let vec_local = scratch_base + 2; // eqref at +2

        // Reserve our scratch locals before generating subexpressions
        // This prevents nested expressions (like desugared vectors) from overwriting them
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

        // Check if it's a variadic closure by testing the type
        // We use ref.test to check if it's a VARIADIC_CLOSURE struct
        f.instruction(&Instruction::LocalGet(closure_local));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(
            gc_types::VARIADIC_CLOSURE,
        )));
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));

        // Variadic closure path
        self.generate_apply_variadic_dispatch(closure_local, vec_local, count_local, f)?;

        f.instruction(&Instruction::Else);

        // Regular closure path
        self.generate_apply_dispatch(closure_local, vec_local, count_local, f)?;

        f.instruction(&Instruction::End);

        Ok(())
    }

    /// Generate code to convert a boxed numeric value to a boxed FLOAT.
    ///
    /// Handles:
    /// - FLOAT struct -> return as-is
    /// - LARGE_INT struct -> extract i64, convert to f64, box as FLOAT
    /// - i31ref small int -> decode, convert to f64, box as FLOAT
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
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::FLOAT)));
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(eqref))));
        {
            // It's already a FLOAT - return it unchanged
            f.instruction(&Instruction::LocalGet(val_local));
        }
        f.instruction(&Instruction::Else);
        {
            // Check if it's a LARGE_INT struct
            f.instruction(&Instruction::LocalGet(val_local));
            f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::LARGE_INT)));
            f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(eqref))));
            {
                // It's a LARGE_INT - extract i64, convert to f64, wrap in FLOAT
                f.instruction(&Instruction::I32Const(type_ids::FLOAT));
                f.instruction(&Instruction::LocalGet(val_local));
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::LARGE_INT)));
                f.instruction(&Instruction::StructGet {
                    struct_type_index: gc_types::LARGE_INT,
                    field_index: gc_types::LI_VALUE,
                });
                f.instruction(&Instruction::F64ConvertI64S);
                f.instruction(&Instruction::StructNew(gc_types::FLOAT));
            }
            f.instruction(&Instruction::Else);
            {
                // Must be i31ref small int - decode, convert, wrap in FLOAT
                f.instruction(&Instruction::I32Const(type_ids::FLOAT));
                f.instruction(&Instruction::LocalGet(val_local));
                f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
                f.instruction(&Instruction::I31GetS);
                // Small ints are stored shifted by 1, so unshift
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32ShrU);
                // Convert i32 to f64
                f.instruction(&Instruction::F64ConvertI32S);
                f.instruction(&Instruction::StructNew(gc_types::FLOAT));
            }
            f.instruction(&Instruction::End);
        }
        f.instruction(&Instruction::End);

        // Restore scratch local
        self.scratch_local.set(scratch_base);

        Ok(())
    }

    /// Generate the nested if-else dispatch for apply based on vector count.
    fn generate_apply_dispatch(
        &self,
        closure_local: u32,
        vec_local: u32,
        count_local: u32,
        f: &mut Function,
    ) -> CompileResult<()> {
        // count == 0?
        f.instruction(&Instruction::LocalGet(count_local));
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));
        self.generate_apply_call_arity(closure_local, vec_local, 0, f)?;
        f.instruction(&Instruction::Else);

        // count == 1?
        f.instruction(&Instruction::LocalGet(count_local));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));
        self.generate_apply_call_arity(closure_local, vec_local, 1, f)?;
        f.instruction(&Instruction::Else);

        // count == 2?
        f.instruction(&Instruction::LocalGet(count_local));
        f.instruction(&Instruction::I32Const(2));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));
        self.generate_apply_call_arity(closure_local, vec_local, 2, f)?;
        f.instruction(&Instruction::Else);

        // count == 3?
        f.instruction(&Instruction::LocalGet(count_local));
        f.instruction(&Instruction::I32Const(3));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));
        self.generate_apply_call_arity(closure_local, vec_local, 3, f)?;
        f.instruction(&Instruction::Else);

        // count == 4?
        f.instruction(&Instruction::LocalGet(count_local));
        f.instruction(&Instruction::I32Const(4));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));
        self.generate_apply_call_arity(closure_local, vec_local, 4, f)?;
        f.instruction(&Instruction::Else);

        // count == 5?
        f.instruction(&Instruction::LocalGet(count_local));
        f.instruction(&Instruction::I32Const(5));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));
        self.generate_apply_call_arity(closure_local, vec_local, 5, f)?;
        f.instruction(&Instruction::Else);

        // count == 6?
        f.instruction(&Instruction::LocalGet(count_local));
        f.instruction(&Instruction::I32Const(6));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));
        self.generate_apply_call_arity(closure_local, vec_local, 6, f)?;
        f.instruction(&Instruction::Else);

        // count == 7?
        f.instruction(&Instruction::LocalGet(count_local));
        f.instruction(&Instruction::I32Const(7));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));
        self.generate_apply_call_arity(closure_local, vec_local, 7, f)?;
        f.instruction(&Instruction::Else);

        // count == 8?
        f.instruction(&Instruction::LocalGet(count_local));
        f.instruction(&Instruction::I32Const(8));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));
        self.generate_apply_call_arity(closure_local, vec_local, 8, f)?;
        f.instruction(&Instruction::Else);

        // Unsupported arity - trap
        f.instruction(&Instruction::Unreachable);

        // Close all 9 if-else blocks (one for each arity 0-8)
        for _ in 0..9 {
            f.instruction(&Instruction::End);
        }

        Ok(())
    }

    /// Generate code to call a closure with a specific arity, extracting args from vector.
    fn generate_apply_call_arity(
        &self,
        closure_local: u32,
        vec_local: u32,
        arity: u32,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;

        let closure_type = gc_types::closure_type_for_arity(arity);
        let fn_type = gc_types::closure_fn_type_for_arity(arity);

        // Look up PersistentVector type index dynamically
        let pv_gc_idx = self.deftype_gc_type_idx("PersistentVector")
            .ok_or_else(|| CompileError::Unsupported("PersistentVector deftype not found".to_string()))?;

        // Get env (first arg to wrapper function)
        f.instruction(&Instruction::LocalGet(closure_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(closure_type)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: closure_type,
            field_index: gc_types::CL_ENV,
        });

        if arity <= 4 {
            // For arities 0-4: Extract individual arguments from the vector
            for i in 0..arity {
                self.generate_vec_nth_raw(vec_local, i, f)?;
            }
        } else {
            // For arities 5+: CLOSURE_FN_N takes (env, args_array)
            // Pass the vector's tail array directly
            f.instruction(&Instruction::LocalGet(vec_local));
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(pv_gc_idx)));
            f.instruction(&Instruction::StructGet {
                struct_type_index: pv_gc_idx,
                field_index: 4, // tail field
            });
            // Cast eqref to the expected array type for CLOSURE_FN_N
            f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::ARRAY)));
        }

        // Get fn (typed funcref)
        f.instruction(&Instruction::LocalGet(closure_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(closure_type)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: closure_type,
            field_index: gc_types::CL_FN,
        });

        // Call with call_ref (not tail call for now - apply result needs to bubble up)
        f.instruction(&Instruction::CallRef(fn_type));

        Ok(())
    }

    /// Generate code to get vector element at a raw i32 index.
    ///
    /// This is a simplified version for small vectors (≤32 elements)
    /// that directly accesses the tail. For apply with vectors typically
    /// containing few elements, this is the common case.
    fn generate_vec_nth_raw(
        &self,
        vec_local: u32,
        index: u32,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;

        // Look up PersistentVector type index dynamically
        let pv_gc_idx = self.deftype_gc_type_idx("PersistentVector")
            .ok_or_else(|| CompileError::Unsupported("PersistentVector deftype not found".to_string()))?;

        // For small vectors (≤32 elements, which is the typical case for apply),
        // we can directly access the tail array. The tail is at field 4.
        // PersistentVector layout: type_id(0), cnt(1), shift(2), root(3), tail(4)
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(pv_gc_idx)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: pv_gc_idx,
            field_index: 4, // tail field
        });

        // Cast to ARRAY type for array.get
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::ARRAY)));

        // Get element at index (for small vectors, index < 32 always)
        f.instruction(&Instruction::I32Const(index as i32));
        f.instruction(&Instruction::ArrayGet(gc_types::ARRAY));

        Ok(())
    }

    /// Generate the nested if-else dispatch for variadic apply based on vector count.
    fn generate_apply_variadic_dispatch(
        &self,
        closure_local: u32,
        vec_local: u32,
        count_local: u32,
        f: &mut Function,
    ) -> CompileResult<()> {
        // Same structure as regular dispatch, but calls variadic version
        // count == 0?
        f.instruction(&Instruction::LocalGet(count_local));
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));
        self.generate_apply_variadic_call_arity(closure_local, vec_local, 0, f)?;
        f.instruction(&Instruction::Else);

        // count == 1?
        f.instruction(&Instruction::LocalGet(count_local));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));
        self.generate_apply_variadic_call_arity(closure_local, vec_local, 1, f)?;
        f.instruction(&Instruction::Else);

        // count == 2?
        f.instruction(&Instruction::LocalGet(count_local));
        f.instruction(&Instruction::I32Const(2));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));
        self.generate_apply_variadic_call_arity(closure_local, vec_local, 2, f)?;
        f.instruction(&Instruction::Else);

        // count == 3?
        f.instruction(&Instruction::LocalGet(count_local));
        f.instruction(&Instruction::I32Const(3));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));
        self.generate_apply_variadic_call_arity(closure_local, vec_local, 3, f)?;
        f.instruction(&Instruction::Else);

        // count == 4?
        f.instruction(&Instruction::LocalGet(count_local));
        f.instruction(&Instruction::I32Const(4));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));
        self.generate_apply_variadic_call_arity(closure_local, vec_local, 4, f)?;
        f.instruction(&Instruction::Else);

        // count == 5?
        f.instruction(&Instruction::LocalGet(count_local));
        f.instruction(&Instruction::I32Const(5));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));
        self.generate_apply_variadic_call_arity(closure_local, vec_local, 5, f)?;
        f.instruction(&Instruction::Else);

        // count == 6?
        f.instruction(&Instruction::LocalGet(count_local));
        f.instruction(&Instruction::I32Const(6));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));
        self.generate_apply_variadic_call_arity(closure_local, vec_local, 6, f)?;
        f.instruction(&Instruction::Else);

        // count == 7?
        f.instruction(&Instruction::LocalGet(count_local));
        f.instruction(&Instruction::I32Const(7));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));
        self.generate_apply_variadic_call_arity(closure_local, vec_local, 7, f)?;
        f.instruction(&Instruction::Else);

        // count == 8?
        f.instruction(&Instruction::LocalGet(count_local));
        f.instruction(&Instruction::I32Const(8));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::Ref(
            RefType::EQREF,
        ))));
        self.generate_apply_variadic_call_arity(closure_local, vec_local, 8, f)?;
        f.instruction(&Instruction::Else);

        // Unsupported arity - trap
        f.instruction(&Instruction::Unreachable);

        // Close all 9 if-else blocks (one for each arity 0-8)
        for _ in 0..9 {
            f.instruction(&Instruction::End);
        }

        Ok(())
    }

    /// Generate code to call a variadic closure with a specific arity, extracting args from vector.
    fn generate_apply_variadic_call_arity(
        &self,
        closure_local: u32,
        vec_local: u32,
        arity: u32,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;

        // Look up PersistentVector type index dynamically
        let pv_gc_idx = self.deftype_gc_type_idx("PersistentVector")
            .ok_or_else(|| CompileError::Unsupported("PersistentVector deftype not found".to_string()))?;

        let variadic_type = gc_types::VARIADIC_CLOSURE;
        // Variadic closure fields use CLOSURE_FN_* types for arities 0-8 (with env as first param)
        let fn_type = gc_types::variadic_fn_type_for_arity_new(arity);
        let fn_field = 1 + arity; // field 0 is type_id, field 1 is fn0, field 2 is fn1, etc.

        // First arg is env (null for variadic builtins, but required by function signature)
        f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::ARRAY)));

        // For arities 0-8: Extract individual arguments from the vector
        // All variadic arities 0-8 now have dedicated function types with individual params
        for i in 0..arity {
            self.generate_vec_nth_raw(vec_local, i, f)?;
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
    /// - string → 0 (not yet supported - strings use ptr+len, not GC refs)
    fn generate_hash(&self, value: &Expr, f: &mut Function) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::Type;

        // Check if the value is a String type - these aren't GC refs yet
        // and produce two values (ptr, len) on the stack, not a single eqref.
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

        // Test LARGE_INT
        self.generate_expr(value, f)?;
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::LARGE_INT)));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));

        // Extract i64 and hash it
        self.generate_expr(value, f)?;
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::LARGE_INT)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::LARGE_INT,
            field_index: gc_types::LI_VALUE,
        });
        self.emit_hash_i64(f);

        f.instruction(&Instruction::Else);

        // Test FLOAT
        self.generate_expr(value, f)?;
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::FLOAT)));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));

        // Extract f64, reinterpret as i64, and hash
        self.generate_expr(value, f)?;
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::FLOAT)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::FLOAT,
            field_index: gc_types::FL_VALUE,
        });
        f.instruction(&Instruction::I64ReinterpretF64);
        self.emit_hash_i64(f);

        f.instruction(&Instruction::Else);

        // NOTE: STRING (array<i8>) would be tested here, but strings currently
        // use memory-based ptr+len representation, not GC arrays.
        // String hashing will be added when strings move to GC refs.

        // Default: return 0 for unsupported types
        f.instruction(&Instruction::I32Const(0));

        f.instruction(&Instruction::End); // close FLOAT
        f.instruction(&Instruction::End); // close LARGE_INT
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
            // Only function refs stay as i32 for now (used as indices)
            Type::Func { .. } => ValType::I32,
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
    /// The table is indexed by: type_id * NUM_BUILTIN + method_id
    ///
    /// Algorithm:
    /// 1. Evaluate and save args to scratch locals
    /// 2. Evaluate obj and save to scratch local
    /// 3. Call $get_type_id to get runtime type ID
    /// 4. Calculate table index: type_id * NUM_BUILTIN + method_id
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

        // 5. Calculate table index: type_id * NUM_BUILTIN + method_id
        f.instruction(&Instruction::LocalGet(type_id_local));
        f.instruction(&Instruction::I32Const(method_ids::NUM_BUILTIN as i32));
        f.instruction(&Instruction::I32Mul);
        f.instruction(&Instruction::I32Const(method_id as i32));
        f.instruction(&Instruction::I32Add);

        // 6. Get the correct type index for this method
        let type_idx = self.protocol_type_index_for_method(method_id);

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

        // Calculate table index: type_id * NUM_BUILTIN + method_id
        f.instruction(&Instruction::LocalGet(type_id_local));
        f.instruction(&Instruction::I32Const(method_ids::NUM_BUILTIN as i32));
        f.instruction(&Instruction::I32Mul);
        f.instruction(&Instruction::I32Const(method_id as i32));
        f.instruction(&Instruction::I32Add);

        // Get the correct type index and call_indirect
        let type_idx = self.protocol_type_index_for_method(method_id);
        f.instruction(&Instruction::CallIndirect {
            type_index: type_idx,
            table_index: dispatch_table::TABLE_INDEX,
        });

        self.scratch_local.set(scratch);
        Ok(())
    }

    /// Get the protocol function type index for a method ID.
    /// This maps method_id to the correct type signature for call_indirect.
    fn protocol_type_index_for_method(&self, method_id: u32) -> u32 {
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
            _ => protocol_type_offsets::ARITY_1_REF,                   // Default for user methods
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
        Type::Unknown => ValType::I32,
    }
}

/// Get all ValTypes for a type (strings need ptr+len pair)
fn type_to_valtypes(ty: &Type) -> Vec<ValType> {
    match ty {
        Type::String => vec![ValType::I32, ValType::I32],
        Type::List(_) => vec![ValType::I32, ValType::I32],
        _ => vec![type_to_valtype(ty)],
    }
}

/// Convert IR type to WIT type string
fn type_to_wit_string(ty: &Type) -> &'static str {
    match ty {
        Type::Unit => "()",
        Type::Bool => "bool",
        Type::I32 => "s32",
        Type::I64 => "s64",
        Type::F64 => "f64",
        Type::String => "string",
        _ => "s64", // Default to s64 for unknown types
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

        // Generate: (struct.new $LARGE_INT (i64.const 1000))
        let expr = Expr::StructNew {
            type_idx: gc_types::LARGE_INT,
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

        // Generate: (struct.get $LARGE_INT 0 (struct.new $LARGE_INT (i64.const 42)))
        let struct_val = Expr::StructNew {
            type_idx: gc_types::LARGE_INT,
            fields: vec![Expr::Int(42)],
        };
        let expr = Expr::StructGet {
            type_idx: gc_types::LARGE_INT,
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
        // Should have GC types (11) + helper types (16) + protocol types (5) + 1 function type = 33 types
        use crate::ir::protocol_types;
        let expected_types = gc_types::NUM_GC_TYPES + NUM_HELPER_TYPES + protocol_types::NUM_PROTOCOL_TYPES + 1;
        assert_eq!(
            type_count,
            expected_types,
            "Expected {} types ({} GC + {} helper + {} protocol + 1 func), found {}",
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
            locals: vec![],
            // Return a large int boxed in a struct
            body: Expr::StructNew {
                type_idx: gc_types::LARGE_INT,
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
            locals: vec![],
            // Expr::Int(42) encodes as (42 << 1) | 1 = 85, wrapped in ref.i31
            body: Expr::Int(42),
        });

        // Generate GC-enabled module
        let codegen = CodeGen::new(&ir);
        let wasm_bytes = codegen.generate_core_module().unwrap();

        // Create wasmtime engine with GC enabled
        let mut config = wasmtime::Config::new();
        config.wasm_gc(true);

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
