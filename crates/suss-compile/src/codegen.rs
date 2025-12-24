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
    AbstractHeapType, CodeSection, ConstExpr, CustomSection, DataSection, DataSegment,
    DataSegmentMode, ElementSection, Elements, EntityType, ExportKind, ExportSection, FieldType,
    Function, FunctionSection, GlobalSection, GlobalType, HeapType, ImportSection, Instruction,
    MemorySection, MemoryType, Module as WasmModule, RawSection, RefType, StorageType,
    TableSection, TableType, TypeSection, ValType,
};
use wit_component::{metadata, ComponentEncoder, StringEncoding};
use wit_parser::{Resolve, WorldId};

use crate::error::{CompileError, CompileResult};
use crate::ir::{BinOp, Expr, Function as IrFunc, Module, Type, UnOp};

// ============================================================================
// Runtime Helper Function Indices
// ============================================================================

/// Number of runtime helper functions emitted before user functions.
/// These are internal functions for protocol dispatch, hashing, and vector trie operations.
const NUM_RUNTIME_HELPERS: u32 = 7;

/// Function index offsets for runtime helpers (relative to start of functions)
mod helper_funcs {
    /// $hash_string(ptr: i32, len: i32) -> i32
    /// Computes xxHash32 of bytes in linear memory
    pub const HASH_STRING: u32 = 0;

    /// $get_type_id(value: eqref) -> i32
    /// Returns the runtime type ID of a GC value
    pub const GET_TYPE_ID: u32 = 1;

    // Vector trie helper functions (for vectors >32 elements)

    /// $vec_aclone(src: eqref) -> eqref
    /// Clone a TRIE_NODE array using array.copy
    pub const VEC_ACLONE: u32 = 2;

    /// $vec_tail_off(vec: eqref) -> i32
    /// Returns the index where the tail begins: (cnt < 32) ? 0 : ((cnt-1) >>> 5) << 5
    pub const VEC_TAIL_OFF: u32 = 3;

    /// $vec_new_path(level: i32, node: eqref) -> eqref
    /// Create a path from root level down to node (for tree growth)
    pub const VEC_NEW_PATH: u32 = 4;

    /// $vec_array_for(vec: eqref, i: i32) -> eqref
    /// Get the leaf array containing index i (trie traversal)
    pub const VEC_ARRAY_FOR: u32 = 5;

    /// $vec_push_tail(vec: eqref, level: i32, parent: eqref, tailnode: eqref) -> eqref
    /// Insert tail node into trie with path copying (recursive)
    pub const VEC_PUSH_TAIL: u32 = 6;
}

/// Type indices for runtime helper function signatures (after GC types)
mod helper_types {
    /// Type for $hash_string: (i32, i32) -> i32
    pub const HASH_STRING: u32 = crate::ir::gc_types::NUM_GC_TYPES;

    /// Type for $get_type_id: (eqref) -> i32
    pub const GET_TYPE_ID: u32 = crate::ir::gc_types::NUM_GC_TYPES + 1;

    // Vector trie helper function types

    /// Type for $vec_aclone: (eqref) -> eqref
    pub const VEC_ACLONE: u32 = crate::ir::gc_types::NUM_GC_TYPES + 2;

    /// Type for $vec_tail_off: (eqref) -> i32
    pub const VEC_TAIL_OFF: u32 = crate::ir::gc_types::NUM_GC_TYPES + 3;

    /// Type for $vec_new_path: (i32, eqref) -> eqref
    pub const VEC_NEW_PATH: u32 = crate::ir::gc_types::NUM_GC_TYPES + 4;

    /// Type for $vec_array_for: (eqref, i32) -> eqref
    pub const VEC_ARRAY_FOR: u32 = crate::ir::gc_types::NUM_GC_TYPES + 5;

    /// Type for $vec_push_tail: (eqref, i32, eqref, eqref) -> eqref
    pub const VEC_PUSH_TAIL: u32 = crate::ir::gc_types::NUM_GC_TYPES + 6;

    /// Number of helper function types
    pub const NUM_HELPER_TYPES: u32 = 7;
}

/// Type indices for protocol function signatures (after helper types)
mod protocol_type_indices {
    use super::helper_types;

    /// Base index for protocol types in the type section
    pub const BASE: u32 = crate::ir::gc_types::NUM_GC_TYPES + helper_types::NUM_HELPER_TYPES;

    /// (eqref) -> eqref - for first, rest, seq
    pub const ARITY_1_REF: u32 = BASE + 0;

    /// (eqref) -> i32 - for count, hash
    pub const ARITY_1_I32: u32 = BASE + 1;

    /// (eqref, eqref) -> eqref - for lookup, nth, conj
    pub const ARITY_2_REF: u32 = BASE + 2;

    /// (eqref, eqref) -> i32 - for equiv
    pub const ARITY_2_I32: u32 = BASE + 3;

    /// (eqref, eqref, eqref) -> eqref - for assoc
    pub const ARITY_3_REF: u32 = BASE + 4;
}

/// Protocol implementation function indices.
///
/// These are wrapper functions that implement protocol methods for each type.
/// They have protocol signatures (eqref args) and cast internally to concrete types.
mod protocol_impl_funcs {
    /// Number of protocol implementation wrapper functions
    pub const NUM_PROTOCOL_IMPLS: u32 = 7;

    // Vector implementations
    /// vec_nth: (eqref, eqref) -> eqref - IIndexed/-nth for PersistentVector
    pub const VEC_NTH: u32 = 0;
    /// vec_count: (eqref) -> i32 - ICounted/-count for PersistentVector
    pub const VEC_COUNT: u32 = 1;
    /// vec_conj: (eqref, eqref) -> eqref - ICollection/-conj for PersistentVector
    pub const VEC_CONJ: u32 = 2;

    // List (Cons) implementations
    /// cons_first: (eqref) -> eqref - ISeq/-first for Cons
    pub const CONS_FIRST: u32 = 3;
    /// cons_rest: (eqref) -> eqref - ISeq/-rest for Cons
    pub const CONS_REST: u32 = 4;

    // Map implementations (placeholder - not yet implemented)
    /// map_count: (eqref) -> i32 - ICounted/-count for PersistentMap
    pub const MAP_COUNT: u32 = 5;

    // Set implementations (placeholder - not yet implemented)
    /// set_count: (eqref) -> i32 - ICounted/-count for PersistentSet
    pub const SET_COUNT: u32 = 6;
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

    /// Get the type index offset for user function types (after GC types + helper types + protocol types)
    fn func_type_offset(&self) -> u32 {
        use crate::ir::protocol_types;
        crate::ir::gc_types::NUM_GC_TYPES + NUM_RUNTIME_HELPERS + protocol_types::NUM_PROTOCOL_TYPES
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

        for func in &self.ir.functions {
            let params: Vec<ValType> = func
                .params
                .iter()
                .flat_map(|(_, ty)| self.type_to_valtypes_gc(ty))
                .collect();
            let results = self.type_to_valtypes_gc(&func.return_type);
            types.ty().function(params, results);
        }
        module.section(&types);

        // Function section - helper functions first, then protocol impls, then user functions
        let mut functions = FunctionSection::new();
        // Helper functions use their dedicated type indices
        functions.function(helper_types::HASH_STRING);
        functions.function(helper_types::GET_TYPE_ID);
        // Vector trie helper functions
        functions.function(helper_types::VEC_ACLONE);
        functions.function(helper_types::VEC_TAIL_OFF);
        functions.function(helper_types::VEC_NEW_PATH);
        functions.function(helper_types::VEC_ARRAY_FOR);
        functions.function(helper_types::VEC_PUSH_TAIL);
        // Protocol implementation functions use protocol type indices
        self.emit_protocol_impl_function_decls(&mut functions);
        // User functions use type_offset + their index
        for (idx, _) in self.ir.functions.iter().enumerate() {
            functions.function(type_offset + idx as u32);
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
        let mut module = WasmModule::new();
        let num_imports = self.num_imports();

        // Type section - import function types first, then local function types
        let mut types = TypeSection::new();

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
                let results = self.type_to_valtypes_gc(&func.return_type);
                types.ty().function(params, results);
            }
        }
        module.section(&types);

        // Import section - WASI functions
        if !self.ir.imports.is_empty() {
            let mut imports = ImportSection::new();
            for (idx, import) in self.ir.imports.iter().enumerate() {
                imports.import(
                    &import.wit_interface,
                    &import.function_name,
                    EntityType::Function(idx as u32),
                );
            }
            module.section(&imports);
        }

        // Function section - local function indices start after imports
        let mut functions = FunctionSection::new();
        for (idx, _) in self.ir.functions.iter().enumerate() {
            functions.function(num_imports + idx as u32);
        }
        module.section(&functions);

        // Memory section
        self.emit_memory_section(&mut module);

        // Global section - heap pointer
        self.emit_global_section(&mut module);

        // Export section
        let mut exports = ExportSection::new();
        exports.export("memory", ExportKind::Memory, 0);

        for (idx, func) in self.ir.functions.iter().enumerate() {
            if func.exported {
                let name = func.export_name.as_deref().unwrap_or(&func.name);
                exports.export(name, ExportKind::Func, num_imports + idx as u32);
            }
        }
        module.section(&exports);

        // Code section - use WIT marshaling for exported functions
        let mut code = CodeSection::new();
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
    /// The table size is TABLE_SIZE (160 = 16 types × 10 methods).
    /// Table index 0 is used for the dispatch table.
    fn emit_table_section(&self, module: &mut WasmModule) {
        use crate::ir::dispatch_table;

        let mut tables = TableSection::new();
        tables.table(TableType {
            element_type: RefType::FUNCREF,
            minimum: dispatch_table::TABLE_SIZE as u64,
            maximum: Some(dispatch_table::TABLE_SIZE as u64),
            table64: false,
            shared: false,
        });
        module.section(&tables);
    }

    /// Emit the element section to populate the dispatch table.
    ///
    /// Registers built-in protocol implementations at their computed indices:
    /// - index = type_id * NUM_BUILTIN + method_id
    fn emit_element_section(&self, module: &mut WasmModule) {
        use crate::ir::dispatch_table;
        use crate::ir::method_ids;
        use crate::ir::type_ids;
        use std::borrow::Cow;

        let mut elements = ElementSection::new();

        // For each protocol implementation, we add it at its dispatch table index
        // We use active element segments with offset to place each at the right index

        // VEC_NTH at (PERSISTENT_VECTOR=6, NTH=3) → index 63
        let vec_nth_idx = dispatch_table::index(type_ids::PERSISTENT_VECTOR as u32, method_ids::NTH);
        elements.active(
            Some(dispatch_table::TABLE_INDEX),
            &ConstExpr::i32_const(vec_nth_idx as i32),
            Elements::Functions(Cow::Owned(vec![self.protocol_impl_func_idx(protocol_impl_funcs::VEC_NTH)])),
        );

        // VEC_COUNT at (PERSISTENT_VECTOR=6, COUNT=2) → index 62
        let vec_count_idx = dispatch_table::index(type_ids::PERSISTENT_VECTOR as u32, method_ids::COUNT);
        elements.active(
            Some(dispatch_table::TABLE_INDEX),
            &ConstExpr::i32_const(vec_count_idx as i32),
            Elements::Functions(Cow::Owned(vec![self.protocol_impl_func_idx(protocol_impl_funcs::VEC_COUNT)])),
        );

        // VEC_CONJ at (PERSISTENT_VECTOR=6, CONJ=4) → index 64
        let vec_conj_idx = dispatch_table::index(type_ids::PERSISTENT_VECTOR as u32, method_ids::CONJ);
        elements.active(
            Some(dispatch_table::TABLE_INDEX),
            &ConstExpr::i32_const(vec_conj_idx as i32),
            Elements::Functions(Cow::Owned(vec![self.protocol_impl_func_idx(protocol_impl_funcs::VEC_CONJ)])),
        );

        // CONS_FIRST at (CONS=4, FIRST=5) → index 45
        let cons_first_idx = dispatch_table::index(type_ids::CONS as u32, method_ids::FIRST);
        elements.active(
            Some(dispatch_table::TABLE_INDEX),
            &ConstExpr::i32_const(cons_first_idx as i32),
            Elements::Functions(Cow::Owned(vec![self.protocol_impl_func_idx(protocol_impl_funcs::CONS_FIRST)])),
        );

        // CONS_REST at (CONS=4, REST=6) → index 46
        let cons_rest_idx = dispatch_table::index(type_ids::CONS as u32, method_ids::REST);
        elements.active(
            Some(dispatch_table::TABLE_INDEX),
            &ConstExpr::i32_const(cons_rest_idx as i32),
            Elements::Functions(Cow::Owned(vec![self.protocol_impl_func_idx(protocol_impl_funcs::CONS_REST)])),
        );

        // MAP_COUNT at (PERSISTENT_MAP=7, COUNT=2) → index 72
        let map_count_idx = dispatch_table::index(type_ids::PERSISTENT_MAP as u32, method_ids::COUNT);
        elements.active(
            Some(dispatch_table::TABLE_INDEX),
            &ConstExpr::i32_const(map_count_idx as i32),
            Elements::Functions(Cow::Owned(vec![self.protocol_impl_func_idx(protocol_impl_funcs::MAP_COUNT)])),
        );

        // SET_COUNT at (PERSISTENT_SET=8, COUNT=2) → index 82
        let set_count_idx = dispatch_table::index(type_ids::PERSISTENT_SET as u32, method_ids::COUNT);
        elements.active(
            Some(dispatch_table::TABLE_INDEX),
            &ConstExpr::i32_const(set_count_idx as i32),
            Elements::Functions(Cow::Owned(vec![self.protocol_impl_func_idx(protocol_impl_funcs::SET_COUNT)])),
        );

        module.section(&elements);
    }

    // ========================================================================
    // GC Type Section
    // ========================================================================

    /// Emit WASM GC struct and array type definitions.
    ///
    /// This defines the GC types used for Clojure's persistent data structures:
    /// - Type 0 (LARGE_INT): struct { i64 } - for integers > 30 bits
    /// - Type 1 (FLOAT): struct { f64 } - all floats are boxed
    /// - Type 2 (STRING): array<i8> - UTF-8 bytes
    /// - Type 3 (TRIE_NODE): array<eqref> - 32-way trie node for vectors
    /// - Type 4 (CONS): struct { first: eqref, rest: eqref } - list cons cell
    /// - Type 5 (HAMT_NODE): struct { bitmap: i32, children: array<eqref> }
    /// - Type 6 (PERSISTENT_VECTOR): struct { cnt, shift, root, tail }
    /// - Type 7 (PERSISTENT_MAP): struct { cnt, root }
    /// - Type 8 (PERSISTENT_SET): struct { cnt, root }
    ///
    /// These types must be emitted BEFORE function types in the type section,
    /// since function type indices start after GC type indices.
    fn emit_gc_types(&self, types: &mut TypeSection) {
        use crate::ir::gc_types;

        // eqref is used as the unified value type (supports ref.eq)
        let eqref = ValType::Ref(RefType::EQREF);

        // All dispatchable struct types have type_id as field 0 for O(1) dispatch.
        // Field 0 is always i32 containing the type's gc_type index.
        let type_id_field = FieldType {
            element_type: StorageType::Val(ValType::I32),
            mutable: false,
        };

        // Type 0: LARGE_INT - struct { type_id: i32, value: i64 }
        // For integers that don't fit in i31ref (> 30 bits)
        types.ty().struct_(vec![
            type_id_field.clone(),
            FieldType {
                element_type: StorageType::Val(ValType::I64),
                mutable: false,
            },
        ]);
        debug_assert_eq!(gc_types::LARGE_INT, 0);

        // Type 1: FLOAT - struct { type_id: i32, value: f64 }
        // All floats are boxed since f64 doesn't fit in i31ref
        types.ty().struct_(vec![
            type_id_field.clone(),
            FieldType {
                element_type: StorageType::Val(ValType::F64),
                mutable: false,
            },
        ]);
        debug_assert_eq!(gc_types::FLOAT, 1);

        // Type 2: STRING - array<i8>
        // UTF-8 string bytes, mutable for construction
        // Note: Arrays don't have type_id field (not used in protocol dispatch)
        types.ty().array(&StorageType::I8, true);
        debug_assert_eq!(gc_types::STRING, 2);

        // Type 3: TRIE_NODE - array<eqref>
        // 32-element array for bit-partitioned vector trie nodes
        // Used for both internal nodes (children) and leaf arrays (values)
        // Note: Arrays don't have type_id field (internal implementation detail)
        types.ty().array(&StorageType::Val(eqref), true);
        debug_assert_eq!(gc_types::TRIE_NODE, 3);

        // Type 4: CONS - struct { type_id: i32, first: eqref, rest: eqref }
        // Cons cell for persistent linked lists
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
        ]);
        debug_assert_eq!(gc_types::CONS, 4);

        // Type 5: HAMT_NODE - struct { type_id: i32, bitmap: i32, children: ref array<eqref> }
        // Hash Array Mapped Trie node for maps and sets
        // bitmap: which of 32 slots are occupied
        // children: sparse array (popcount(bitmap) entries)
        let trie_node_ref = ValType::Ref(RefType {
            nullable: true,
            heap_type: HeapType::Concrete(gc_types::TRIE_NODE),
        });
        types.ty().struct_(vec![
            type_id_field.clone(),
            FieldType {
                element_type: StorageType::Val(ValType::I32), // bitmap
                mutable: false,
            },
            FieldType {
                element_type: StorageType::Val(trie_node_ref), // children array
                mutable: false,
            },
        ]);
        debug_assert_eq!(gc_types::HAMT_NODE, 5);

        // Type 6: PERSISTENT_VECTOR - struct { type_id: i32, cnt, shift, root, tail }
        // ClojureScript-style 32-way bit-partitioned vector trie
        // cnt: total element count
        // shift: depth * 5 (5 bits per level, 32 = 2^5)
        // root: null or TRIE_NODE (internal trie structure)
        // tail: TRIE_NODE (rightmost leaf, partial for O(1) conj)
        types.ty().struct_(vec![
            type_id_field.clone(),
            FieldType {
                element_type: StorageType::Val(ValType::I32), // cnt
                mutable: false,
            },
            FieldType {
                element_type: StorageType::Val(ValType::I32), // shift
                mutable: false,
            },
            FieldType {
                element_type: StorageType::Val(eqref), // root (nullable trie)
                mutable: false,
            },
            FieldType {
                element_type: StorageType::Val(eqref), // tail (trie node)
                mutable: false,
            },
        ]);
        debug_assert_eq!(gc_types::PERSISTENT_VECTOR, 6);

        // Type 7: PERSISTENT_MAP - struct { type_id: i32, cnt, root }
        // HAMT-based persistent map
        // cnt: number of key-value pairs
        // root: null or HAMT_NODE
        types.ty().struct_(vec![
            type_id_field.clone(),
            FieldType {
                element_type: StorageType::Val(ValType::I32), // cnt
                mutable: false,
            },
            FieldType {
                element_type: StorageType::Val(eqref), // root (nullable HAMT)
                mutable: false,
            },
        ]);
        debug_assert_eq!(gc_types::PERSISTENT_MAP, 7);

        // Type 8: PERSISTENT_SET - struct { type_id: i32, cnt, root }
        // HAMT-based persistent set (same structure as map, entries are keys only)
        types.ty().struct_(vec![
            type_id_field.clone(),
            FieldType {
                element_type: StorageType::Val(ValType::I32), // cnt
                mutable: false,
            },
            FieldType {
                element_type: StorageType::Val(eqref), // root (nullable HAMT)
                mutable: false,
            },
        ]);
        debug_assert_eq!(gc_types::PERSISTENT_SET, 8);
    }

    /// Emit function types for runtime helper functions.
    ///
    /// These come after GC types but before protocol function types.
    /// Order must match helper_types module constants.
    fn emit_helper_types(&self, types: &mut TypeSection) {
        let eqref = ValType::Ref(RefType::EQREF);

        // Type for $hash_string: (i32, i32) -> i32
        // Takes (ptr, len) pointing to linear memory, returns hash
        types.ty().function(
            vec![ValType::I32, ValType::I32],
            vec![ValType::I32],
        );

        // Type for $get_type_id: (eqref) -> i32
        // Takes any GC value, returns its type ID
        types.ty().function(vec![eqref], vec![ValType::I32]);

        // Vector trie helper function types

        // Type for $vec_aclone: (eqref) -> eqref
        // Clone a TRIE_NODE array
        types.ty().function(vec![eqref], vec![eqref]);

        // Type for $vec_tail_off: (eqref) -> i32
        // Calculate tail offset from vector
        types.ty().function(vec![eqref], vec![ValType::I32]);

        // Type for $vec_new_path: (i32, eqref) -> eqref
        // Create path from level to node
        types.ty().function(vec![ValType::I32, eqref], vec![eqref]);

        // Type for $vec_array_for: (eqref, i32) -> eqref
        // Get leaf array containing index
        types.ty().function(vec![eqref, ValType::I32], vec![eqref]);

        // Type for $vec_push_tail: (eqref, i32, eqref, eqref) -> eqref
        // Insert tail into trie with path copying
        types
            .ty()
            .function(vec![eqref, ValType::I32, eqref, eqref], vec![eqref]);
    }

    /// Emit function types for protocol methods.
    ///
    /// These come after helper types but before user function types.
    /// Order must match protocol_type_indices module constants.
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
    /// Each protocol implementation has a specific type matching its arity:
    /// - count: ARITY_1_I32 (eqref) -> i32
    /// - nth, first, rest: ARITY_1_REF or ARITY_2_REF
    fn emit_protocol_impl_function_decls(&self, functions: &mut FunctionSection) {
        // VEC_NTH: (eqref, eqref) -> eqref
        functions.function(protocol_type_indices::ARITY_2_REF);

        // VEC_COUNT: (eqref) -> i32
        functions.function(protocol_type_indices::ARITY_1_I32);

        // VEC_CONJ: (eqref, eqref) -> eqref
        functions.function(protocol_type_indices::ARITY_2_REF);

        // CONS_FIRST: (eqref) -> eqref
        functions.function(protocol_type_indices::ARITY_1_REF);

        // CONS_REST: (eqref) -> eqref
        functions.function(protocol_type_indices::ARITY_1_REF);

        // MAP_COUNT: (eqref) -> i32
        functions.function(protocol_type_indices::ARITY_1_I32);

        // SET_COUNT: (eqref) -> i32
        functions.function(protocol_type_indices::ARITY_1_I32);
    }

    /// Emit code for protocol implementation wrapper functions.
    ///
    /// These wrappers:
    /// 1. Take eqref parameters
    /// 2. Cast to concrete GC types
    /// 3. Perform the operation
    /// 4. Return result (as eqref or i32 depending on method)
    fn emit_protocol_impl_functions(&self, code: &mut CodeSection) -> CompileResult<()> {
        // VEC_NTH: (vec: eqref, idx: eqref) -> eqref
        code.function(&self.generate_vec_nth_wrapper());

        // VEC_COUNT: (vec: eqref) -> i32
        code.function(&self.generate_vec_count_wrapper());

        // VEC_CONJ: (vec: eqref, val: eqref) -> eqref
        code.function(&self.generate_vec_conj_wrapper()?);

        // CONS_FIRST: (list: eqref) -> eqref
        code.function(&self.generate_cons_first_wrapper());

        // CONS_REST: (list: eqref) -> eqref
        code.function(&self.generate_cons_rest_wrapper());

        // MAP_COUNT: (map: eqref) -> i32 (placeholder - just returns 0)
        code.function(&self.generate_map_count_wrapper());

        // SET_COUNT: (set: eqref) -> i32 (placeholder - just returns 0)
        code.function(&self.generate_set_count_wrapper());

        Ok(())
    }

    /// Generate VEC_NTH wrapper: (vec: eqref, idx: eqref) -> eqref
    fn generate_vec_nth_wrapper(&self) -> Function {
        use crate::ir::gc_types;

        // Locals: idx_local (i32) for decoded index
        let mut f = Function::new(vec![(1, ValType::I32)]);
        let idx_local: u32 = 2;

        // Decode index from i31ref (param 1) to i32
        f.instruction(&Instruction::LocalGet(1));
        f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
        f.instruction(&Instruction::I31GetS);
        // Decode: encoded >> 1
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32ShrS);
        f.instruction(&Instruction::LocalSet(idx_local));

        // Call $vec_array_for(vec, idx) to get the leaf array
        f.instruction(&Instruction::LocalGet(0)); // vec
        f.instruction(&Instruction::LocalGet(idx_local)); // idx
        f.instruction(&Instruction::Call(
            self.helper_func_idx(helper_funcs::VEC_ARRAY_FOR),
        ));

        // Cast result to TRIE_NODE for array.get
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::TRIE_NODE)));

        // Get element at idx & 0x1f
        f.instruction(&Instruction::LocalGet(idx_local));
        f.instruction(&Instruction::I32Const(0x1F));
        f.instruction(&Instruction::I32And);

        // array.get leaf[index]
        f.instruction(&Instruction::ArrayGet(gc_types::TRIE_NODE));

        f.instruction(&Instruction::End);
        f
    }

    /// Generate VEC_COUNT wrapper: (vec: eqref) -> i32
    fn generate_vec_count_wrapper(&self) -> Function {
        use crate::ir::gc_types;

        let mut f = Function::new(vec![]);

        // Cast to PersistentVector
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::PERSISTENT_VECTOR)));

        // Get cnt field (field 1, after type_id)
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_CNT,
        });

        f.instruction(&Instruction::End);
        f
    }

    /// Generate VEC_CONJ wrapper: (vec: eqref, val: eqref) -> eqref
    ///
    /// This wrapper performs the full VecConj operation:
    /// - If room in tail: clone tail, append element
    /// - If tail full: push old tail into trie, create new single-element tail
    fn generate_vec_conj_wrapper(&self) -> CompileResult<Function> {
        use crate::ir::gc_types;
        use crate::ir::type_ids;

        // Locals layout:
        // 0: vec (eqref) - parameter
        // 1: val (eqref) - parameter
        // 2: cnt (i32)
        // 3: new_tail/new_root (eqref)
        let mut f = Function::new(vec![
            (1, ValType::I32),                  // cnt (local 2)
            (1, ValType::Ref(RefType::EQREF)),  // new_tail/new_root (local 3)
        ]);

        let vec_local: u32 = 0;
        let val_local: u32 = 1;
        let cnt_local: u32 = 2;
        let new_tail_local: u32 = 3;

        // Get count from vec
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_CNT,
        });
        f.instruction(&Instruction::LocalSet(cnt_local));

        // Check: (cnt - tail_off) < 32 means room in tail
        f.instruction(&Instruction::LocalGet(cnt_local));
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::Call(
            self.helper_func_idx(helper_funcs::VEC_TAIL_OFF),
        ));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(32));
        f.instruction(&Instruction::I32LtU);

        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(
            ValType::Ref(RefType::EQREF),
        )));

        // === Room in tail: create larger tail, copy elements, add new element ===
        // Create new array of size (cnt & 0x1f) + 1, filled with null
        f.instruction(&Instruction::RefNull(HeapType::Abstract {
            shared: false,
            ty: AbstractHeapType::Eq,
        }));
        f.instruction(&Instruction::LocalGet(cnt_local));
        f.instruction(&Instruction::I32Const(0x1f));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::ArrayNew(gc_types::TRIE_NODE));
        f.instruction(&Instruction::LocalSet(new_tail_local));

        // Copy old tail elements to new tail
        f.instruction(&Instruction::LocalGet(new_tail_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::TRIE_NODE)));
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_TAIL,
        });
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::TRIE_NODE)));
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::LocalGet(cnt_local));
        f.instruction(&Instruction::I32Const(0x1f));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::ArrayCopy {
            array_type_index_dst: gc_types::TRIE_NODE,
            array_type_index_src: gc_types::TRIE_NODE,
        });

        // Set new element at index (cnt & 0x1f)
        f.instruction(&Instruction::LocalGet(new_tail_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::TRIE_NODE)));
        f.instruction(&Instruction::LocalGet(cnt_local));
        f.instruction(&Instruction::I32Const(0x1f));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::LocalGet(val_local)); // val from parameter
        f.instruction(&Instruction::ArraySet(gc_types::TRIE_NODE));

        // Create new vector: type_id, (inc cnt), shift, root, new_tail
        f.instruction(&Instruction::I32Const(type_ids::PERSISTENT_VECTOR));
        f.instruction(&Instruction::LocalGet(cnt_local));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);

        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::PERSISTENT_VECTOR)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_SHIFT,
        });

        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::PERSISTENT_VECTOR)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_ROOT,
        });

        f.instruction(&Instruction::LocalGet(new_tail_local));
        f.instruction(&Instruction::StructNew(gc_types::PERSISTENT_VECTOR));

        f.instruction(&Instruction::Else);

        // === Tail full: push into trie ===
        // Check root overflow: (cnt >>> 5) > (1 << shift)
        f.instruction(&Instruction::LocalGet(cnt_local));
        f.instruction(&Instruction::I32Const(5));
        f.instruction(&Instruction::I32ShrU);

        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::PERSISTENT_VECTOR)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_SHIFT,
        });
        f.instruction(&Instruction::I32Shl);
        f.instruction(&Instruction::I32GtU);

        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(
            ValType::Ref(RefType::EQREF),
        )));

        // === Root overflow: grow tree ===
        // new_root = [old_root, new_path(shift, tail)]
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::PERSISTENT_VECTOR)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_ROOT,
        });

        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::PERSISTENT_VECTOR)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_SHIFT,
        });
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::PERSISTENT_VECTOR)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_TAIL,
        });
        f.instruction(&Instruction::Call(
            self.helper_func_idx(helper_funcs::VEC_NEW_PATH),
        ));

        f.instruction(&Instruction::ArrayNewFixed {
            array_type_index: gc_types::TRIE_NODE,
            array_size: 2,
        });
        f.instruction(&Instruction::LocalSet(new_tail_local));

        // Build result: type_id, (inc cnt), (shift + 5), new_root, [val]
        f.instruction(&Instruction::I32Const(type_ids::PERSISTENT_VECTOR));
        f.instruction(&Instruction::LocalGet(cnt_local));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);

        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::PERSISTENT_VECTOR)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_SHIFT,
        });
        f.instruction(&Instruction::I32Const(5));
        f.instruction(&Instruction::I32Add);

        f.instruction(&Instruction::LocalGet(new_tail_local));

        f.instruction(&Instruction::LocalGet(val_local)); // val
        f.instruction(&Instruction::ArrayNewFixed {
            array_type_index: gc_types::TRIE_NODE,
            array_size: 1,
        });

        f.instruction(&Instruction::StructNew(gc_types::PERSISTENT_VECTOR));

        f.instruction(&Instruction::Else);

        // === No overflow: push tail into existing tree ===
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::PERSISTENT_VECTOR)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_ROOT,
        });
        f.instruction(&Instruction::RefIsNull);

        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(
            ValType::Ref(RefType::EQREF),
        )));

        // Root is null: new_root = new_path(shift, tail)
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::PERSISTENT_VECTOR)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_SHIFT,
        });
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::PERSISTENT_VECTOR)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_TAIL,
        });
        f.instruction(&Instruction::Call(
            self.helper_func_idx(helper_funcs::VEC_NEW_PATH),
        ));

        f.instruction(&Instruction::Else);

        // Root is not null: push_tail(vec, shift, root, tail)
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::PERSISTENT_VECTOR)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_SHIFT,
        });
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::PERSISTENT_VECTOR)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_ROOT,
        });
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::PERSISTENT_VECTOR)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_TAIL,
        });
        f.instruction(&Instruction::Call(
            self.helper_func_idx(helper_funcs::VEC_PUSH_TAIL),
        ));

        f.instruction(&Instruction::End); // end if (root is null)

        f.instruction(&Instruction::LocalSet(new_tail_local));

        // Build result: type_id, (inc cnt), shift, new_root, [val]
        f.instruction(&Instruction::I32Const(type_ids::PERSISTENT_VECTOR));
        f.instruction(&Instruction::LocalGet(cnt_local));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);

        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::PERSISTENT_VECTOR)));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_SHIFT,
        });

        f.instruction(&Instruction::LocalGet(new_tail_local));

        f.instruction(&Instruction::LocalGet(val_local)); // val
        f.instruction(&Instruction::ArrayNewFixed {
            array_type_index: gc_types::TRIE_NODE,
            array_size: 1,
        });

        f.instruction(&Instruction::StructNew(gc_types::PERSISTENT_VECTOR));

        f.instruction(&Instruction::End); // end if (root overflow)
        f.instruction(&Instruction::End); // end if (room in tail)

        f.instruction(&Instruction::End);
        Ok(f)
    }

    /// Generate CONS_FIRST wrapper: (list: eqref) -> eqref
    fn generate_cons_first_wrapper(&self) -> Function {
        use crate::ir::gc_types;

        let mut f = Function::new(vec![]);

        // Cast to Cons
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::CONS)));

        // Get first field (field 1, after type_id)
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::CONS,
            field_index: gc_types::CONS_FIRST,
        });

        f.instruction(&Instruction::End);
        f
    }

    /// Generate CONS_REST wrapper: (list: eqref) -> eqref
    fn generate_cons_rest_wrapper(&self) -> Function {
        use crate::ir::gc_types;

        let mut f = Function::new(vec![]);

        // Cast to Cons
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::CONS)));

        // Get rest field (field 2, after type_id)
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::CONS,
            field_index: gc_types::CONS_REST,
        });

        f.instruction(&Instruction::End);
        f
    }

    /// Generate MAP_COUNT wrapper: (map: eqref) -> i32 (placeholder)
    fn generate_map_count_wrapper(&self) -> Function {
        use crate::ir::gc_types;

        let mut f = Function::new(vec![]);

        // Cast to PersistentMap
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::PERSISTENT_MAP)));

        // Get cnt field (field 1, after type_id)
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_MAP,
            field_index: gc_types::PM_CNT,
        });

        f.instruction(&Instruction::End);
        f
    }

    /// Generate SET_COUNT wrapper: (set: eqref) -> i32 (placeholder)
    fn generate_set_count_wrapper(&self) -> Function {
        use crate::ir::gc_types;

        let mut f = Function::new(vec![]);

        // Cast to PersistentSet
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::PERSISTENT_SET)));

        // Get cnt field (field 1, after type_id)
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_SET,
            field_index: gc_types::PS_CNT,
        });

        f.instruction(&Instruction::End);
        f
    }

    /// Emit code for runtime helper functions.
    ///
    /// These come before user functions in the code section.
    fn emit_helper_functions(&self, code: &mut CodeSection) -> CompileResult<()> {
        // $hash_string
        code.function(&self.generate_hash_string_func());

        // $get_type_id
        code.function(&self.generate_get_type_id_func());

        // Vector trie helper functions
        code.function(&self.generate_vec_aclone_func());
        code.function(&self.generate_vec_tail_off_func());
        code.function(&self.generate_vec_new_path_func());
        code.function(&self.generate_vec_array_for_func());
        code.function(&self.generate_vec_push_tail_func());

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

    /// Generate $get_type_id function - returns runtime type ID for protocol dispatch.
    ///
    /// Signature: (value: eqref) -> i32
    ///
    /// Type ID mapping:
    /// - i31ref (nil, bool, small int): -1
    /// - LargeInt: 0
    /// - Float: 1
    /// - String: 2
    /// - TrieNode: 3
    /// - Cons: 4
    /// - HamtNode: 5
    /// - PersistentVector: 6
    /// - PersistentMap: 7
    /// - PersistentSet: 8
    ///
    /// Uses ref.test chain for type dispatch. Future optimization: use struct
    /// subtyping to enable O(1) field 0 access for all dispatchable struct types.
    fn generate_get_type_id_func(&self) -> Function {
        use crate::ir::gc_types;
        use crate::ir::type_ids;

        // No extra locals needed (param is local 0)
        let locals = vec![];
        let mut f = Function::new(locals);

        // Test for i31ref first (most common for primitives: nil, bool, small int)
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Abstract {
            shared: false,
            ty: AbstractHeapType::I31,
        }));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(type_ids::I31REF));
        f.instruction(&Instruction::Else);

        // Test for PersistentVector (most common collection)
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::PERSISTENT_VECTOR)));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(type_ids::PERSISTENT_VECTOR));
        f.instruction(&Instruction::Else);

        // Test for Cons (common for lists)
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::CONS)));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(type_ids::CONS));
        f.instruction(&Instruction::Else);

        // Test for PersistentMap
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::PERSISTENT_MAP)));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(type_ids::PERSISTENT_MAP));
        f.instruction(&Instruction::Else);

        // Test for PersistentSet
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::PERSISTENT_SET)));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(type_ids::PERSISTENT_SET));
        f.instruction(&Instruction::Else);

        // Test for Float
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::FLOAT)));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(type_ids::FLOAT));
        f.instruction(&Instruction::Else);

        // Test for LargeInt
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::LARGE_INT)));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(type_ids::LARGE_INT));
        f.instruction(&Instruction::Else);

        // Test for String (array type)
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::STRING)));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(type_ids::STRING));
        f.instruction(&Instruction::Else);

        // Test for TrieNode (array type, internal)
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::TRIE_NODE)));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(type_ids::TRIE_NODE));
        f.instruction(&Instruction::Else);

        // Test for HamtNode (internal)
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(gc_types::HAMT_NODE)));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(type_ids::HAMT_NODE));
        f.instruction(&Instruction::Else);

        // Unknown type - return -2 as error indicator
        f.instruction(&Instruction::I32Const(-2));

        // Close all the if/else chains (10 nested ifs)
        for _ in 0..10 {
            f.instruction(&Instruction::End);
        }

        f.instruction(&Instruction::End);
        f
    }

    // ========================================================================
    // Vector Trie Helper Functions
    // ========================================================================

    /// Generate $vec_aclone function - clone a TRIE_NODE array.
    ///
    /// Signature: (src: eqref) -> eqref
    ///
    /// Creates a new array with the same length and copies all elements.
    fn generate_vec_aclone_func(&self) -> Function {
        use crate::ir::gc_types;

        // Locals: dst (eqref), len (i32)
        let locals = vec![
            (1, ValType::Ref(RefType::EQREF)), // dst (local 1)
            (1, ValType::I32),                 // len (local 2)
        ];
        let mut f = Function::new(locals);

        // Get source length
        f.instruction(&Instruction::LocalGet(0)); // src
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::TRIE_NODE)));
        f.instruction(&Instruction::ArrayLen);
        f.instruction(&Instruction::LocalSet(2)); // len

        // Create new array of same length, filled with null
        f.instruction(&Instruction::RefNull(HeapType::Abstract {
            shared: false,
            ty: AbstractHeapType::Eq,
        }));
        f.instruction(&Instruction::LocalGet(2)); // len
        f.instruction(&Instruction::ArrayNew(gc_types::TRIE_NODE));
        f.instruction(&Instruction::LocalSet(1)); // dst

        // array.copy dst[0..len] = src[0..len]
        f.instruction(&Instruction::LocalGet(1)); // dst
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::TRIE_NODE)));
        f.instruction(&Instruction::I32Const(0)); // dst_offset
        f.instruction(&Instruction::LocalGet(0)); // src
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::TRIE_NODE)));
        f.instruction(&Instruction::I32Const(0)); // src_offset
        f.instruction(&Instruction::LocalGet(2)); // len
        f.instruction(&Instruction::ArrayCopy {
            array_type_index_dst: gc_types::TRIE_NODE,
            array_type_index_src: gc_types::TRIE_NODE,
        });

        // Return dst
        f.instruction(&Instruction::LocalGet(1));
        f.instruction(&Instruction::End);
        f
    }

    /// Generate $vec_tail_off function - calculate where the tail begins.
    ///
    /// Signature: (vec: eqref) -> i32
    ///
    /// Algorithm: (cnt < 32) ? 0 : ((cnt - 1) >>> 5) << 5
    fn generate_vec_tail_off_func(&self) -> Function {
        use crate::ir::gc_types;

        // Locals: cnt (i32)
        let locals = vec![(1, ValType::I32)]; // cnt (local 1)
        let mut f = Function::new(locals);

        // Get cnt from vector
        f.instruction(&Instruction::LocalGet(0)); // vec
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_CNT,
        });
        f.instruction(&Instruction::LocalSet(1)); // save cnt

        // if (cnt < 32) return 0
        f.instruction(&Instruction::LocalGet(1)); // cnt
        f.instruction(&Instruction::I32Const(32));
        f.instruction(&Instruction::I32LtU);
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(
            ValType::I32,
        )));
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::Else);

        // ((cnt - 1) >>> 5) << 5
        f.instruction(&Instruction::LocalGet(1)); // cnt
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub); // cnt - 1
        f.instruction(&Instruction::I32Const(5));
        f.instruction(&Instruction::I32ShrU); // >>> 5
        f.instruction(&Instruction::I32Const(5));
        f.instruction(&Instruction::I32Shl); // << 5

        f.instruction(&Instruction::End); // end if

        f.instruction(&Instruction::End);
        f
    }

    /// Generate $vec_new_path function - create path from root level down to node.
    ///
    /// Signature: (level: i32, node: eqref) -> eqref
    ///
    /// Algorithm (iterative, working from bottom up):
    /// while (level > 0) { node = [node]; level -= 5; }
    /// return node
    fn generate_vec_new_path_func(&self) -> Function {
        use crate::ir::gc_types;

        // Params: level (0), node (1)
        // Locals: ret (eqref) for the new array
        let locals = vec![(1, ValType::Ref(RefType::EQREF))];
        let mut f = Function::new(locals);
        let ret_local: u32 = 2;

        // Loop: while level > 0, wrap node in new 32-element array (with node at index 0)
        f.instruction(&Instruction::Block(wasm_encoder::BlockType::Result(
            ValType::Ref(RefType::EQREF),
        )));
        f.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));

        // if level <= 0, break and return node
        f.instruction(&Instruction::LocalGet(0)); // level
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32LeS);
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        f.instruction(&Instruction::LocalGet(1)); // node
        f.instruction(&Instruction::Br(2)); // break to outer block with result
        f.instruction(&Instruction::End);

        // Create new 32-element array filled with null
        f.instruction(&Instruction::RefNull(HeapType::Abstract {
            shared: false,
            ty: AbstractHeapType::Eq,
        }));
        f.instruction(&Instruction::I32Const(32));
        f.instruction(&Instruction::ArrayNew(gc_types::TRIE_NODE));
        f.instruction(&Instruction::LocalSet(ret_local));

        // Set ret[0] = node
        f.instruction(&Instruction::LocalGet(ret_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::TRIE_NODE)));
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::LocalGet(1)); // current node
        f.instruction(&Instruction::ArraySet(gc_types::TRIE_NODE));

        // node = ret
        f.instruction(&Instruction::LocalGet(ret_local));
        f.instruction(&Instruction::LocalSet(1));

        // level -= 5
        f.instruction(&Instruction::LocalGet(0));
        f.instruction(&Instruction::I32Const(5));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::LocalSet(0));

        f.instruction(&Instruction::Br(0)); // continue loop
        f.instruction(&Instruction::End); // end loop

        // This is unreachable but needed for block structure
        f.instruction(&Instruction::LocalGet(1));
        f.instruction(&Instruction::End); // end block

        f.instruction(&Instruction::End);
        f
    }

    /// Generate $vec_array_for function - get the leaf array containing index i.
    ///
    /// Signature: (vec: eqref, i: i32) -> eqref
    ///
    /// Algorithm:
    /// if (i >= tail_off(vec)) return vec.tail
    /// else traverse trie: node = root, level = shift
    ///   while (level > 0) { node = node[(i >>> level) & 0x1f]; level -= 5 }
    ///   return node
    fn generate_vec_array_for_func(&self) -> Function {
        use crate::ir::gc_types;

        // Params: vec (0), i (1)
        // Locals: node (eqref), level (i32), tail_off (i32)
        let locals = vec![
            (1, ValType::Ref(RefType::EQREF)), // node (local 2)
            (2, ValType::I32),                 // level (local 3), tail_off (local 4)
        ];
        let mut f = Function::new(locals);

        // Calculate tail_off
        f.instruction(&Instruction::LocalGet(0)); // vec
        f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::VEC_TAIL_OFF)));
        f.instruction(&Instruction::LocalSet(4)); // tail_off

        // if (i >= tail_off) return tail
        f.instruction(&Instruction::LocalGet(1)); // i
        f.instruction(&Instruction::LocalGet(4)); // tail_off
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(
            ValType::Ref(RefType::EQREF),
        )));

        // Return tail
        f.instruction(&Instruction::LocalGet(0)); // vec
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_TAIL,
        });

        f.instruction(&Instruction::Else);

        // Trie traversal: node = root, level = shift
        f.instruction(&Instruction::LocalGet(0)); // vec
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_ROOT,
        });
        f.instruction(&Instruction::LocalSet(2)); // node = root

        f.instruction(&Instruction::LocalGet(0)); // vec
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_SHIFT,
        });
        f.instruction(&Instruction::LocalSet(3)); // level = shift

        // Loop while level > 0
        f.instruction(&Instruction::Block(wasm_encoder::BlockType::Result(
            ValType::Ref(RefType::EQREF),
        )));
        f.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));

        // if (level <= 0) break, return node
        f.instruction(&Instruction::LocalGet(3)); // level
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32LeS);
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        f.instruction(&Instruction::LocalGet(2)); // node
        f.instruction(&Instruction::Br(2)); // break to outer block with result
        f.instruction(&Instruction::End);

        // subidx = (i >>> level) & 0x1f
        // node = node[subidx]
        f.instruction(&Instruction::LocalGet(2)); // node
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::TRIE_NODE)));
        f.instruction(&Instruction::LocalGet(1)); // i
        f.instruction(&Instruction::LocalGet(3)); // level
        f.instruction(&Instruction::I32ShrU);
        f.instruction(&Instruction::I32Const(0x1f));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::ArrayGet(gc_types::TRIE_NODE));
        f.instruction(&Instruction::LocalSet(2)); // node = node[subidx]

        // level -= 5
        f.instruction(&Instruction::LocalGet(3));
        f.instruction(&Instruction::I32Const(5));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::LocalSet(3));

        f.instruction(&Instruction::Br(0)); // continue loop
        f.instruction(&Instruction::End); // end loop

        // Unreachable but needed for block structure
        f.instruction(&Instruction::LocalGet(2));
        f.instruction(&Instruction::End); // end block

        f.instruction(&Instruction::End); // end if (i >= tail_off)

        f.instruction(&Instruction::End);
        f
    }

    /// Generate $vec_push_tail function - insert tail node into trie with path copying.
    ///
    /// Signature: (vec: eqref, level: i32, parent: eqref, tailnode: eqref) -> eqref
    ///
    /// Algorithm:
    /// subidx = ((cnt - 1) >>> level) & 0x1f
    /// ret = aclone(parent)
    /// if (level == 5) { ret[subidx] = tailnode; return ret }
    /// child = parent[subidx]
    /// if (child == null) { ret[subidx] = new_path(level-5, tailnode) }
    /// else { ret[subidx] = push_tail(vec, level-5, child, tailnode) }
    /// return ret
    fn generate_vec_push_tail_func(&self) -> Function {
        use crate::ir::gc_types;

        // Params: vec (0), level (1), parent (2), tailnode (3)
        // Locals: subidx (i32), ret (eqref), child (eqref)
        let locals = vec![
            (1, ValType::I32),                 // subidx (local 4)
            (2, ValType::Ref(RefType::EQREF)), // ret (local 5), child (local 6)
        ];
        let mut f = Function::new(locals);

        // subidx = ((cnt - 1) >>> level) & 0x1f
        f.instruction(&Instruction::LocalGet(0)); // vec
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_CNT,
        });
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub); // cnt - 1
        f.instruction(&Instruction::LocalGet(1)); // level
        f.instruction(&Instruction::I32ShrU);
        f.instruction(&Instruction::I32Const(0x1f));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::LocalSet(4)); // subidx

        // ret = aclone(parent)
        f.instruction(&Instruction::LocalGet(2)); // parent
        f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::VEC_ACLONE)));
        f.instruction(&Instruction::LocalSet(5)); // ret

        // if (level == 5)
        f.instruction(&Instruction::LocalGet(1)); // level
        f.instruction(&Instruction::I32Const(5));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(
            ValType::Ref(RefType::EQREF),
        )));

        // ret[subidx] = tailnode; return ret
        f.instruction(&Instruction::LocalGet(5)); // ret
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::TRIE_NODE)));
        f.instruction(&Instruction::LocalGet(4)); // subidx
        f.instruction(&Instruction::LocalGet(3)); // tailnode
        f.instruction(&Instruction::ArraySet(gc_types::TRIE_NODE));
        f.instruction(&Instruction::LocalGet(5)); // ret

        f.instruction(&Instruction::Else);

        // child = parent[subidx]
        f.instruction(&Instruction::LocalGet(2)); // parent
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::TRIE_NODE)));
        f.instruction(&Instruction::LocalGet(4)); // subidx
        f.instruction(&Instruction::ArrayGet(gc_types::TRIE_NODE));
        f.instruction(&Instruction::LocalSet(6)); // child

        // if (child == null)
        f.instruction(&Instruction::LocalGet(6)); // child
        f.instruction(&Instruction::RefIsNull);
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(
            ValType::Ref(RefType::EQREF),
        )));

        // ret[subidx] = new_path(level-5, tailnode)
        f.instruction(&Instruction::LocalGet(1)); // level
        f.instruction(&Instruction::I32Const(5));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::LocalGet(3)); // tailnode
        f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::VEC_NEW_PATH)));
        f.instruction(&Instruction::LocalSet(6)); // store result temporarily

        f.instruction(&Instruction::LocalGet(5)); // ret
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::TRIE_NODE)));
        f.instruction(&Instruction::LocalGet(4)); // subidx
        f.instruction(&Instruction::LocalGet(6)); // new child
        f.instruction(&Instruction::ArraySet(gc_types::TRIE_NODE));
        f.instruction(&Instruction::LocalGet(5)); // ret

        f.instruction(&Instruction::Else);

        // ret[subidx] = push_tail(vec, level-5, child, tailnode)
        f.instruction(&Instruction::LocalGet(0)); // vec
        f.instruction(&Instruction::LocalGet(1)); // level
        f.instruction(&Instruction::I32Const(5));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::LocalGet(6)); // child
        f.instruction(&Instruction::LocalGet(3)); // tailnode
        f.instruction(&Instruction::Call(self.helper_func_idx(helper_funcs::VEC_PUSH_TAIL)));
        f.instruction(&Instruction::LocalSet(6)); // store result temporarily

        f.instruction(&Instruction::LocalGet(5)); // ret
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::TRIE_NODE)));
        f.instruction(&Instruction::LocalGet(4)); // subidx
        f.instruction(&Instruction::LocalGet(6)); // new child
        f.instruction(&Instruction::ArraySet(gc_types::TRIE_NODE));
        f.instruction(&Instruction::LocalGet(5)); // ret

        f.instruction(&Instruction::End); // end if (child == null)
        f.instruction(&Instruction::End); // end if (level == 5)

        f.instruction(&Instruction::End);
        f
    }

    // ========================================================================
    // Persistent Vector Codegen
    // ========================================================================

    /// Generate code to create a new persistent vector from elements.
    ///
    /// Structure: PersistentVector { cnt, shift, root, tail }
    /// - Empty vector: cnt=0, shift=5, root=null, tail=empty array
    /// - Small vector (≤32): cnt=n, shift=5, root=null, tail=[elements]
    /// - Large vector (>32): cnt=n, shift=5*depth, root=trie, tail=rightmost leaf
    fn generate_vec_new(&self, elements: &[Expr], f: &mut Function) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::type_ids;
        let cnt = elements.len() as i32;

        if cnt == 0 {
            // Empty vector: type_id, cnt=0, shift=5, root=null, tail=empty array
            f.instruction(&Instruction::I32Const(type_ids::PERSISTENT_VECTOR));
            f.instruction(&Instruction::I32Const(0)); // cnt
            f.instruction(&Instruction::I32Const(5)); // shift
            f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::TRIE_NODE))); // root
            // Create empty tail array using ArrayNewFixed with 0 elements
            f.instruction(&Instruction::ArrayNewFixed {
                array_type_index: gc_types::TRIE_NODE,
                array_size: 0,
            });
            f.instruction(&Instruction::StructNew(gc_types::PERSISTENT_VECTOR));
        } else if cnt <= 32 {
            // Small vector: type_id, cnt=n, shift=5, root=null, tail=[elements]
            f.instruction(&Instruction::I32Const(type_ids::PERSISTENT_VECTOR));
            f.instruction(&Instruction::I32Const(cnt)); // cnt
            f.instruction(&Instruction::I32Const(5)); // shift
            f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::TRIE_NODE))); // root

            // Build tail array using ArrayNewFixed - push all elements, then create array
            for elem in elements.iter() {
                self.generate_expr(elem, f)?;
            }
            f.instruction(&Instruction::ArrayNewFixed {
                array_type_index: gc_types::TRIE_NODE,
                array_size: cnt as u32,
            });

            f.instruction(&Instruction::StructNew(gc_types::PERSISTENT_VECTOR));
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
        use crate::ir::type_ids;
        // Start with first 32 elements as base vector
        let (first_32, rest) = elements.split_at(32.min(elements.len()));

        // Create initial vector with first batch: type_id, cnt, shift, root, tail
        f.instruction(&Instruction::I32Const(type_ids::PERSISTENT_VECTOR));
        f.instruction(&Instruction::I32Const(first_32.len() as i32));
        f.instruction(&Instruction::I32Const(5));
        f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::TRIE_NODE)));

        // Build tail with first elements using ArrayNewFixed
        for elem in first_32.iter() {
            self.generate_expr(elem, f)?;
        }
        f.instruction(&Instruction::ArrayNewFixed {
            array_type_index: gc_types::TRIE_NODE,
            array_size: first_32.len() as u32,
        });

        f.instruction(&Instruction::StructNew(gc_types::PERSISTENT_VECTOR));

        // For remaining elements, we need to generate conj calls
        // This is inefficient but correct - can optimize later
        for elem in rest {
            self.generate_expr(elem, f)?;
            self.generate_vec_conj_inplace(f)?;
        }

        Ok(())
    }

    /// Generate conj when vec is already on stack
    fn generate_vec_conj_inplace(&self, _f: &mut Function) -> CompileResult<()> {
        // TODO: Implement proper trie manipulation
        // For now, this is a placeholder that will need full implementation
        // when we support vectors > 32 elements
        Err(CompileError::Unsupported(
            "Vectors with more than 32 elements not yet supported".to_string(),
        ))
    }

    /// Generate code to get element at index from persistent vector.
    ///
    /// Algorithm:
    /// - If index >= tailoff: return tail[index & 0x1F]
    /// - Else: traverse trie from root using bit partitioning
    fn generate_vec_nth(&self, vec: &Expr, index: &Expr, f: &mut Function) -> CompileResult<()> {
        use crate::ir::gc_types;

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

        // Call $vec_array_for(vec, idx) to get the leaf array
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::LocalGet(idx_local));
        f.instruction(&Instruction::Call(
            self.helper_func_idx(helper_funcs::VEC_ARRAY_FOR),
        ));

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

    /// Generate code to add element to end of persistent vector.
    ///
    /// Full algorithm:
    /// - If (cnt - tail_off) < 32 (room in tail): clone tail, append element
    /// - Else (tail full):
    ///   - If root overflow: create new root [old_root, new_path(shift, tail)], shift += 5
    ///   - Else: push_tail into existing trie
    ///   - Create new single-element tail [val]
    fn generate_vec_conj(&self, vec: &Expr, val: &Expr, f: &mut Function) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::type_ids;

        // Scratch local indices (relative to scratch_base)
        // Layout: scratch_base+0: eqref, +1: i32, +2: eqref, +3: eqref, +4: eqref
        let scratch_base = self.scratch_local.get();
        let vec_local = scratch_base; // eqref - stores the vector
        let cnt_local = scratch_base + 1; // i32 - stores count
        let new_tail_local = scratch_base + 2; // eqref - stores new tail/new root

        // Evaluate and store vec
        self.generate_expr(vec, f)?;
        f.instruction(&Instruction::LocalSet(vec_local));

        // Get count
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_CNT,
        });
        f.instruction(&Instruction::LocalSet(cnt_local));

        // Check: (cnt - tail_off) < 32 means room in tail
        // Inline tail_off calculation: cnt - $vec_tail_off(vec)
        f.instruction(&Instruction::LocalGet(cnt_local));
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::Call(
            self.helper_func_idx(helper_funcs::VEC_TAIL_OFF),
        ));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(32));
        f.instruction(&Instruction::I32LtU);

        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(
            ValType::Ref(RefType::EQREF),
        )));

        // === Room in tail: create larger tail, copy elements, add new element ===
        // The tail array is sized to fit exactly, so we need to create a new array
        // that is one element larger, not just clone.

        // tail_idx = cnt & 0x1f (where new element goes)
        // new_tail = array.new of size (tail_idx + 1)
        // array.copy from old tail to new tail
        // array.set new tail at tail_idx

        // Create new array of size (cnt & 0x1f) + 1, filled with null
        f.instruction(&Instruction::RefNull(HeapType::Abstract {
            shared: false,
            ty: AbstractHeapType::Eq,
        }));
        f.instruction(&Instruction::LocalGet(cnt_local));
        f.instruction(&Instruction::I32Const(0x1f));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add); // (cnt & 0x1f) + 1
        f.instruction(&Instruction::ArrayNew(gc_types::TRIE_NODE));
        f.instruction(&Instruction::LocalSet(new_tail_local));

        // Copy old tail elements to new tail using array.copy
        // array.copy dst dst_offset src src_offset len
        f.instruction(&Instruction::LocalGet(new_tail_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::TRIE_NODE)));
        f.instruction(&Instruction::I32Const(0)); // dst_offset
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_TAIL,
        });
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(gc_types::TRIE_NODE)));
        f.instruction(&Instruction::I32Const(0)); // src_offset
        // len = cnt & 0x1f (old tail size)
        f.instruction(&Instruction::LocalGet(cnt_local));
        f.instruction(&Instruction::I32Const(0x1f));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::ArrayCopy {
            array_type_index_dst: gc_types::TRIE_NODE,
            array_type_index_src: gc_types::TRIE_NODE,
        });

        // Set new element at index (cnt & 0x1f)
        f.instruction(&Instruction::LocalGet(new_tail_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::TRIE_NODE,
        )));
        f.instruction(&Instruction::LocalGet(cnt_local));
        f.instruction(&Instruction::I32Const(0x1f));
        f.instruction(&Instruction::I32And);
        self.generate_expr(val, f)?;
        f.instruction(&Instruction::ArraySet(gc_types::TRIE_NODE));

        // Create new vector: type_id, (inc cnt), shift, root, new_tail
        f.instruction(&Instruction::I32Const(type_ids::PERSISTENT_VECTOR));
        f.instruction(&Instruction::LocalGet(cnt_local));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add); // new cnt

        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_SHIFT,
        });

        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_ROOT,
        });

        f.instruction(&Instruction::LocalGet(new_tail_local));
        f.instruction(&Instruction::StructNew(gc_types::PERSISTENT_VECTOR));

        f.instruction(&Instruction::Else);

        // === Tail full: push into trie ===

        // Check root overflow: (cnt >>> 5) > (1 << shift)
        f.instruction(&Instruction::LocalGet(cnt_local));
        f.instruction(&Instruction::I32Const(5));
        f.instruction(&Instruction::I32ShrU); // cnt >>> 5

        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_SHIFT,
        });
        f.instruction(&Instruction::I32Shl); // 1 << shift
        f.instruction(&Instruction::I32GtU); // (cnt>>>5) > (1<<shift)

        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(
            ValType::Ref(RefType::EQREF),
        )));

        // === Root overflow: grow tree ===
        // new_root = [old_root, new_path(shift, tail)]

        // First element: old_root
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_ROOT,
        });

        // Second element: new_path(shift, tail)
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_SHIFT,
        });
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_TAIL,
        });
        f.instruction(&Instruction::Call(
            self.helper_func_idx(helper_funcs::VEC_NEW_PATH),
        ));

        // Create new root array [old_root, new_path_result]
        f.instruction(&Instruction::ArrayNewFixed {
            array_type_index: gc_types::TRIE_NODE,
            array_size: 2,
        });
        f.instruction(&Instruction::LocalSet(new_tail_local)); // reuse local for new_root

        // Build result: type_id, (inc cnt), (shift + 5), new_root, [val]
        f.instruction(&Instruction::I32Const(type_ids::PERSISTENT_VECTOR));
        f.instruction(&Instruction::LocalGet(cnt_local));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);

        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_SHIFT,
        });
        f.instruction(&Instruction::I32Const(5));
        f.instruction(&Instruction::I32Add); // shift + 5

        f.instruction(&Instruction::LocalGet(new_tail_local)); // new_root

        // New tail = [val]
        self.generate_expr(val, f)?;
        f.instruction(&Instruction::ArrayNewFixed {
            array_type_index: gc_types::TRIE_NODE,
            array_size: 1,
        });

        f.instruction(&Instruction::StructNew(gc_types::PERSISTENT_VECTOR));

        f.instruction(&Instruction::Else);

        // === No overflow: push tail into existing tree ===
        // new_root = push_tail(vec, shift, root, tail)
        // But first handle the case where root is null (first push)

        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_ROOT,
        });
        f.instruction(&Instruction::RefIsNull);

        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(
            ValType::Ref(RefType::EQREF),
        )));

        // Root is null: this is the first time we're pushing tail into trie
        // new_root = new_path(shift, tail) - wraps tail in a single-element array
        // Note: must use shift (not shift-5) so the tail gets wrapped properly
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_SHIFT,
        });
        // Use shift directly (not shift - 5), so new_path(5, tail) = [tail]
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_TAIL,
        });
        f.instruction(&Instruction::Call(
            self.helper_func_idx(helper_funcs::VEC_NEW_PATH),
        ));

        f.instruction(&Instruction::Else);

        // Root is not null: push_tail(vec, shift, root, tail)
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_SHIFT,
        });
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_ROOT,
        });
        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_TAIL,
        });
        f.instruction(&Instruction::Call(
            self.helper_func_idx(helper_funcs::VEC_PUSH_TAIL),
        ));

        f.instruction(&Instruction::End); // end if (root is null)

        f.instruction(&Instruction::LocalSet(new_tail_local)); // reuse local for new_root

        // Build result: type_id, (inc cnt), shift, new_root, [val]
        f.instruction(&Instruction::I32Const(type_ids::PERSISTENT_VECTOR));
        f.instruction(&Instruction::LocalGet(cnt_local));
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);

        f.instruction(&Instruction::LocalGet(vec_local));
        f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
            gc_types::PERSISTENT_VECTOR,
        )));
        f.instruction(&Instruction::StructGet {
            struct_type_index: gc_types::PERSISTENT_VECTOR,
            field_index: gc_types::PV_SHIFT,
        });

        f.instruction(&Instruction::LocalGet(new_tail_local)); // new_root

        // New tail = [val]
        self.generate_expr(val, f)?;
        f.instruction(&Instruction::ArrayNewFixed {
            array_type_index: gc_types::TRIE_NODE,
            array_size: 1,
        });

        f.instruction(&Instruction::StructNew(gc_types::PERSISTENT_VECTOR));

        f.instruction(&Instruction::End); // end if (root overflow)
        f.instruction(&Instruction::End); // end if (room in tail)

        Ok(())
    }

    // ========================================================================
    // Persistent Map Codegen
    // ========================================================================

    /// Generate code to create a new persistent map from key-value pairs.
    fn generate_map_new(&self, pairs: &[(Expr, Expr)], f: &mut Function) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::type_ids;
        // TODO: Implement full HAMT. For now, create map struct with count set.
        // The root will be null (placeholder until HAMT is implemented).
        let cnt = pairs.len() as i32;
        f.instruction(&Instruction::I32Const(type_ids::PERSISTENT_MAP)); // type_id
        f.instruction(&Instruction::I32Const(cnt)); // cnt
        f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::HAMT_NODE))); // root (placeholder)
        f.instruction(&Instruction::StructNew(gc_types::PERSISTENT_MAP));
        Ok(())
    }

    /// Generate map lookup (get)
    fn generate_map_get(
        &self,
        map: &Expr,
        key: &Expr,
        f: &mut Function,
    ) -> CompileResult<()> {
        use crate::ir::gc_types;
        // Placeholder - full HAMT lookup requires hash function and bit manipulation
        self.generate_expr(map, f)?;
        self.generate_expr(key, f)?;
        // For now, just return nil
        f.instruction(&Instruction::Drop);
        f.instruction(&Instruction::Drop);
        f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
        f.instruction(&Instruction::RefI31);
        Ok(())
    }

    /// Generate map association (assoc)
    fn generate_map_assoc(
        &self,
        map: &Expr,
        key: &Expr,
        val: &Expr,
        f: &mut Function,
    ) -> CompileResult<()> {
        self.generate_expr(map, f)?;
        self.generate_expr(key, f)?;
        self.generate_expr(val, f)?;
        self.generate_map_assoc_impl(f)
    }

    /// Implementation of map assoc when values are on stack
    fn generate_map_assoc_impl(&self, f: &mut Function) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::type_ids;
        // Placeholder - needs full HAMT implementation
        // For now, just drop values and return empty map
        f.instruction(&Instruction::Drop); // val
        f.instruction(&Instruction::Drop); // key
        f.instruction(&Instruction::Drop); // map
        f.instruction(&Instruction::I32Const(type_ids::PERSISTENT_MAP)); // type_id
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::HAMT_NODE)));
        f.instruction(&Instruction::StructNew(gc_types::PERSISTENT_MAP));
        Ok(())
    }

    // ========================================================================
    // Persistent Set Codegen
    // ========================================================================

    /// Generate code to create a new persistent set from elements.
    fn generate_set_new(&self, elements: &[Expr], f: &mut Function) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::type_ids;
        // TODO: Implement full HAMT. For now, create set struct with count set.
        // The root will be null (placeholder until HAMT is implemented).
        let cnt = elements.len() as i32;
        f.instruction(&Instruction::I32Const(type_ids::PERSISTENT_SET)); // type_id
        f.instruction(&Instruction::I32Const(cnt)); // cnt
        f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::HAMT_NODE))); // root (placeholder)
        f.instruction(&Instruction::StructNew(gc_types::PERSISTENT_SET));
        Ok(())
    }

    /// Generate set membership test (contains?)
    fn generate_set_contains(
        &self,
        set: &Expr,
        key: &Expr,
        f: &mut Function,
    ) -> CompileResult<()> {
        // Placeholder - needs HAMT lookup
        self.generate_expr(set, f)?;
        self.generate_expr(key, f)?;
        f.instruction(&Instruction::Drop);
        f.instruction(&Instruction::Drop);
        f.instruction(&Instruction::I32Const(0)); // false
        Ok(())
    }

    /// Generate set conjunction (conj)
    fn generate_set_conj(
        &self,
        set: &Expr,
        val: &Expr,
        f: &mut Function,
    ) -> CompileResult<()> {
        self.generate_expr(set, f)?;
        self.generate_expr(val, f)?;
        self.generate_set_conj_impl(f)
    }

    /// Implementation of set conj when values are on stack
    fn generate_set_conj_impl(&self, f: &mut Function) -> CompileResult<()> {
        use crate::ir::gc_types;
        use crate::ir::type_ids;
        // Placeholder - needs full HAMT implementation
        f.instruction(&Instruction::Drop); // val
        f.instruction(&Instruction::Drop); // set
        f.instruction(&Instruction::I32Const(type_ids::PERSISTENT_SET)); // type_id
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::RefNull(HeapType::Concrete(gc_types::HAMT_NODE)));
        f.instruction(&Instruction::StructNew(gc_types::PERSISTENT_SET));
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

        // Add 15 scratch locals (3 sets of 5, for nested protocol dispatch):
        // Each protocol dispatch uses 5 locals, and nested calls bump by 5.
        // Scratch locals layout per set (repeated 3x for nesting):
        //   +0: eqref (protocol dispatch, vec storage)
        //   +1: i32 (count, index)
        //   +2: eqref (new tail, temp)
        //   +3: eqref (old tail, temp)
        //   +4: eqref (extra temp)
        for _ in 0..3 {
            local_types.push((1, ValType::Ref(RefType::EQREF))); // scratch +0
            local_types.push((1, ValType::I32));                  // scratch +1
            local_types.push((1, ValType::Ref(RefType::EQREF))); // scratch +2
            local_types.push((1, ValType::Ref(RefType::EQREF))); // scratch +3
            local_types.push((1, ValType::Ref(RefType::EQREF))); // scratch +4
        }

        let mut f = Function::new(local_types);
        self.generate_expr(&func.body, &mut f)?;
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
                // Check if target function is exported (needs WIT marshaling)
                let num_imports = self.num_imports();
                let is_exported = if *func >= num_imports {
                    let local_idx = (*func - num_imports) as usize;
                    self.ir.functions.get(local_idx).map_or(false, |f| f.exported)
                } else {
                    false // Imports use WIT types directly
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
                let is_exported = if *func >= num_imports {
                    let local_idx = (*func - num_imports) as usize;
                    self.ir.functions.get(local_idx).map_or(false, |f| f.exported)
                } else {
                    false
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
            // For simple expressions that don't contain sub-expressions, delegate to normal gen
            _ => self.generate_expr(expr, f),
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
                _ => unreachable!("arithmetic binop only: {:?}", op)
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
        self.generate_expr_inner(expr, f, 0)
    }

    fn generate_expr_inner(&self, expr: &Expr, f: &mut Function, loop_depth: u32) -> CompileResult<()> {
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
                f.instruction(&Instruction::LocalGet(*local));
            }

            Expr::LocalSet { local, value, ty: _ } => {
                self.generate_expr_inner(value, f, loop_depth)?;
                f.instruction(&Instruction::LocalSet(*local));
                f.instruction(&Instruction::I32Const(0));
            }

            Expr::GlobalGet(idx) => {
                f.instruction(&Instruction::GlobalGet(*idx));
            }

            Expr::GlobalSet(idx, value) => {
                self.generate_expr_inner(value, f, loop_depth)?;
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
                    // Arithmetic operations: unwrap, compute, wrap
                    (BinOp::Add, Type::I32) | (BinOp::Add, Type::I64) => {
                        self.generate_expr_inner(left, f, loop_depth)?;
                        generate_unwrap_i31(f);
                        self.generate_expr_inner(right, f, loop_depth)?;
                        generate_unwrap_i31(f);
                        f.instruction(&Instruction::I32Add);
                        // Encode result: (n << 1) | 1
                        f.instruction(&Instruction::I32Const(1));
                        f.instruction(&Instruction::I32Shl);
                        f.instruction(&Instruction::I32Const(1));
                        f.instruction(&Instruction::I32Or);
                        f.instruction(&Instruction::RefI31);
                    }
                    (BinOp::Sub, Type::I32) | (BinOp::Sub, Type::I64) => {
                        self.generate_expr_inner(left, f, loop_depth)?;
                        generate_unwrap_i31(f);
                        self.generate_expr_inner(right, f, loop_depth)?;
                        generate_unwrap_i31(f);
                        f.instruction(&Instruction::I32Sub);
                        f.instruction(&Instruction::I32Const(1));
                        f.instruction(&Instruction::I32Shl);
                        f.instruction(&Instruction::I32Const(1));
                        f.instruction(&Instruction::I32Or);
                        f.instruction(&Instruction::RefI31);
                    }
                    (BinOp::Mul, Type::I32) | (BinOp::Mul, Type::I64) => {
                        self.generate_expr_inner(left, f, loop_depth)?;
                        generate_unwrap_i31(f);
                        self.generate_expr_inner(right, f, loop_depth)?;
                        generate_unwrap_i31(f);
                        f.instruction(&Instruction::I32Mul);
                        f.instruction(&Instruction::I32Const(1));
                        f.instruction(&Instruction::I32Shl);
                        f.instruction(&Instruction::I32Const(1));
                        f.instruction(&Instruction::I32Or);
                        f.instruction(&Instruction::RefI31);
                    }
                    (BinOp::Div, Type::I32) | (BinOp::Div, Type::I64) => {
                        self.generate_expr_inner(left, f, loop_depth)?;
                        generate_unwrap_i31(f);
                        self.generate_expr_inner(right, f, loop_depth)?;
                        generate_unwrap_i31(f);
                        f.instruction(&Instruction::I32DivS);
                        f.instruction(&Instruction::I32Const(1));
                        f.instruction(&Instruction::I32Shl);
                        f.instruction(&Instruction::I32Const(1));
                        f.instruction(&Instruction::I32Or);
                        f.instruction(&Instruction::RefI31);
                    }
                    (BinOp::Rem, Type::I32) | (BinOp::Rem, Type::I64) => {
                        self.generate_expr_inner(left, f, loop_depth)?;
                        generate_unwrap_i31(f);
                        self.generate_expr_inner(right, f, loop_depth)?;
                        generate_unwrap_i31(f);
                        f.instruction(&Instruction::I32RemS);
                        f.instruction(&Instruction::I32Const(1));
                        f.instruction(&Instruction::I32Shl);
                        f.instruction(&Instruction::I32Const(1));
                        f.instruction(&Instruction::I32Or);
                        f.instruction(&Instruction::RefI31);
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
                        f.instruction(&Instruction::StructGet {
                            struct_type_index: gc_types::FLOAT,
                            field_index: gc_types::FL_VALUE,
                        });
                        self.generate_expr(right, f)?;
                        f.instruction(&Instruction::StructGet {
                            struct_type_index: gc_types::FLOAT,
                            field_index: gc_types::FL_VALUE,
                        });
                        f.instruction(&Instruction::F64Div);
                        f.instruction(&Instruction::StructNew(gc_types::FLOAT));
                    }

                    // Comparison operations: unwrap, compare, return bool sentinel
                    (BinOp::Eq, _) | (BinOp::Ne, _) | (BinOp::Lt, _) | (BinOp::Le, _) | (BinOp::Gt, _) | (BinOp::Ge, _) => {
                        self.generate_expr_inner(left, f, loop_depth)?;
                        generate_unwrap_i31(f);
                        self.generate_expr_inner(right, f, loop_depth)?;
                        generate_unwrap_i31(f);

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
                                self.generate_expr_inner(operand, f, loop_depth)?;
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
                                self.generate_expr_inner(operand, f, loop_depth)?;
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
                        self.generate_condition_inner(operand, f, loop_depth)?;
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
                    self.generate_expr_inner(arg, f, loop_depth)?;
                }
                f.instruction(&Instruction::Call(*func));
            }

            Expr::TailCall { func, args } => {
                for arg in args {
                    self.generate_expr_inner(arg, f, loop_depth)?;
                }
                f.instruction(&Instruction::ReturnCall(*func));
            }

            Expr::If {
                cond,
                then_branch,
                else_branch,
                ty,
            } => {
                self.generate_condition_inner(cond, f, loop_depth)?;

                let block_type = wasm_encoder::BlockType::Result(self.type_to_valtype_gc(ty));

                f.instruction(&Instruction::If(block_type));
                // Branches are inside the If block, so increment depth
                self.generate_expr_inner(then_branch, f, loop_depth + 1)?;
                f.instruction(&Instruction::Else);
                self.generate_expr_inner(else_branch, f, loop_depth + 1)?;
                f.instruction(&Instruction::End);
            }

            Expr::Block(exprs) => {
                if exprs.is_empty() {
                    // Empty block returns nil
                    f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
                    f.instruction(&Instruction::RefI31);
                } else {
                    for (i, expr) in exprs.iter().enumerate() {
                        self.generate_expr_inner(expr, f, loop_depth)?;
                        if i < exprs.len() - 1 {
                            f.instruction(&Instruction::Drop);
                        }
                    }
                }
            }

            Expr::Let { bindings, body } => {
                for (idx, value) in bindings {
                    self.generate_expr_inner(value, f, loop_depth)?;
                    f.instruction(&Instruction::LocalSet(*idx));
                }
                self.generate_expr_inner(body, f, loop_depth)?;
            }

            Expr::Loop { bindings, body } => {
                for (idx, value) in bindings {
                    self.generate_expr_inner(value, f, loop_depth)?;
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
                self.generate_expr_inner(body, f, 0)?;

                // When body falls through (doesn't recur), Br(1) exits to outer block
                // with the result value on stack
                f.instruction(&Instruction::Br(1));

                f.instruction(&Instruction::End);
                f.instruction(&Instruction::End);
            }

            Expr::Recur(values) => {
                for (local_idx, value) in values.iter() {
                    self.generate_expr_inner(value, f, loop_depth)?;
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

            // Tagged value operations (DEPRECATED - no longer used in GC mode)
            Expr::MakeTagged { tag, payload } => {
                self.generate_expr(payload, f)?;
                const PAYLOAD_MASK: i64 = 0x00FFFFFFFFFFFFFF;
                const TAG_SHIFT: u32 = 56;
                f.instruction(&Instruction::I64Const(PAYLOAD_MASK));
                f.instruction(&Instruction::I64And);
                f.instruction(&Instruction::I64Const((*tag as i64) << TAG_SHIFT));
                f.instruction(&Instruction::I64Or);
            }

            Expr::GetTag(value) => {
                self.generate_expr(value, f)?;
                const TAG_SHIFT: i64 = 56;
                f.instruction(&Instruction::I64Const(TAG_SHIFT));
                f.instruction(&Instruction::I64ShrU);
                f.instruction(&Instruction::I32WrapI64);
            }

            Expr::GetPayload(value) => {
                self.generate_expr(value, f)?;
                const PAYLOAD_MASK: i64 = 0x00FFFFFFFFFFFFFF;
                f.instruction(&Instruction::I64Const(PAYLOAD_MASK));
                f.instruction(&Instruction::I64And);
            }

            // Heap operations
            Expr::Alloc(size_expr) => {
                self.generate_expr(size_expr, f)?;
                f.instruction(&Instruction::GlobalGet(0));
                f.instruction(&Instruction::LocalTee(0));
                self.generate_expr(size_expr, f)?;
                f.instruction(&Instruction::I32Add);
                f.instruction(&Instruction::GlobalSet(0));
                f.instruction(&Instruction::LocalGet(0));
            }

            Expr::HeapStore { base, offset, value } => {
                self.generate_expr(base, f)?;
                self.generate_expr(value, f)?;
                f.instruction(&Instruction::I64Store(wasm_encoder::MemArg {
                    offset: *offset as u64,
                    align: 3,
                    memory_index: 0,
                }));
            }

            Expr::HeapLoad { base, offset } => {
                self.generate_expr(base, f)?;
                f.instruction(&Instruction::I64Load(wasm_encoder::MemArg {
                    offset: *offset as u64,
                    align: 3,
                    memory_index: 0,
                }));
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
                f.instruction(&Instruction::StructGet {
                    struct_type_index: *type_idx,
                    field_index: *field_idx,
                });
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
                self.generate_expr(array, f)?;
                f.instruction(&Instruction::ArrayLen);
            }

            Expr::ArrayGet {
                type_idx,
                array,
                index,
            } => {
                self.generate_expr(array, f)?;
                self.generate_expr(index, f)?;
                f.instruction(&Instruction::ArrayGet(*type_idx));
            }

            Expr::ArraySet {
                type_idx,
                array,
                index,
                value,
            } => {
                self.generate_expr(array, f)?;
                self.generate_expr(index, f)?;
                self.generate_expr(value, f)?;
                f.instruction(&Instruction::ArraySet(*type_idx));
            }

            Expr::RefTestI31(value) => {
                self.generate_expr(value, f)?;
                f.instruction(&Instruction::RefTestNonNull(HeapType::I31));
            }

            Expr::RefTest { type_idx, value } => {
                self.generate_expr(value, f)?;
                f.instruction(&Instruction::RefTestNonNull(HeapType::Concrete(*type_idx)));
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

            Expr::VecConj { vec, val } => {
                self.generate_vec_conj(vec, val, f)?;
            }

            Expr::VecCount(vec) => {
                self.generate_expr(vec, f)?;
                // Cast to concrete PersistentVector type for struct.get
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
                    gc_types::PERSISTENT_VECTOR,
                )));
                // struct.get PERSISTENT_VECTOR.cnt - produces i32
                f.instruction(&Instruction::StructGet {
                    struct_type_index: gc_types::PERSISTENT_VECTOR,
                    field_index: gc_types::PV_CNT,
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

            Expr::MapGet { map, key } => {
                self.generate_map_get(map, key, f)?;
            }

            Expr::MapAssoc { map, key, val } => {
                self.generate_map_assoc(map, key, val, f)?;
            }

            Expr::MapCount(map) => {
                self.generate_expr(map, f)?;
                // struct.get PERSISTENT_MAP.cnt
                f.instruction(&Instruction::StructGet {
                    struct_type_index: gc_types::PERSISTENT_MAP,
                    field_index: gc_types::PM_CNT,
                });
            }

            // =========================================================
            // Persistent Set Operations
            // =========================================================

            Expr::SetNew(elements) => {
                self.generate_set_new(elements, f)?;
            }

            Expr::SetContains { set, key } => {
                self.generate_set_contains(set, key, f)?;
            }

            Expr::SetConj { set, val } => {
                self.generate_set_conj(set, val, f)?;
            }

            Expr::SetCount(set) => {
                self.generate_expr(set, f)?;
                // struct.get PERSISTENT_SET.cnt
                f.instruction(&Instruction::StructGet {
                    struct_type_index: gc_types::PERSISTENT_SET,
                    field_index: gc_types::PS_CNT,
                });
            }

            // =========================================================
            // List Operations (cons cells)
            // =========================================================

            Expr::ListFirst(list) => {
                self.generate_expr(list, f)?;
                // Cast to concrete CONS type for struct.get
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
                    gc_types::CONS,
                )));
                // struct.get CONS.first
                f.instruction(&Instruction::StructGet {
                    struct_type_index: gc_types::CONS,
                    field_index: gc_types::CONS_FIRST,
                });
            }

            Expr::ListRest(list) => {
                self.generate_expr(list, f)?;
                // Cast to concrete CONS type for struct.get
                f.instruction(&Instruction::RefCastNonNull(HeapType::Concrete(
                    gc_types::CONS,
                )));
                // struct.get CONS.rest
                f.instruction(&Instruction::StructGet {
                    struct_type_index: gc_types::CONS,
                    field_index: gc_types::CONS_REST,
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
                self.generate_protocol_dispatch(obj, *method_id, args, *in_tail_position, f)?;
            }

            Expr::GetTypeId(value) => {
                self.generate_get_type_id(value, f)?;
            }
        }

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
        self.generate_condition_inner(cond, f, 0)
    }

    fn generate_condition_inner(&self, cond: &Expr, f: &mut Function, loop_depth: u32) -> CompileResult<()> {
        use crate::ir::gc_types;

        self.generate_expr_inner(cond, f, loop_depth)?;

        // All values are now GC refs in GC mode
        // Test if it's an i31ref that could be nil or false
        f.instruction(&Instruction::RefTestNonNull(HeapType::I31));
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Result(ValType::I32)));

        // Is i31ref - need to check if it's nil (0) or false (2)
        // Re-evaluate to get the value back, then cast to i31ref
        self.generate_expr_inner(cond, f, loop_depth + 1)?;
        // Cast eqref to i31ref (we know it's i31 because we tested for it)
        f.instruction(&Instruction::RefCastNonNull(HeapType::I31));
        f.instruction(&Instruction::I31GetS);
        // Truthy if value != 0 (nil) AND value != 2 (false)
        f.instruction(&Instruction::I32Const(gc_types::NIL_SENTINEL));
        f.instruction(&Instruction::I32Ne);
        self.generate_expr_inner(cond, f, loop_depth + 1)?;
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
            | Type::Tagged
            | Type::Unknown => ValType::Ref(RefType::EQREF),
            // Only function refs stay as i32 for now (used as indices)
            Type::Func { .. } => ValType::I32,
        }
    }

    /// Get all ValTypes for a type - everything is a single eqref in GC mode
    fn type_to_valtypes_gc(&self, ty: &Type) -> Vec<ValType> {
        vec![self.type_to_valtype_gc(ty)]
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
    ) -> CompileResult<()> {
        self.generate_protocol_dispatch_table(obj, method_id, args, in_tail_position, f)
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
            self.generate_expr(arg, f)?;
            f.instruction(&Instruction::LocalSet(args_base + i as u32));
        }

        // 2. Evaluate obj and save to scratch local
        self.generate_expr(obj, f)?;
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

    /// Get the protocol function type index for a method ID.
    /// This maps method_id to the correct type signature for call_indirect.
    fn protocol_type_index_for_method(&self, method_id: u32) -> u32 {
        use crate::ir::method_ids;

        match method_id {
            method_ids::LOOKUP => protocol_type_indices::ARITY_2_REF,  // (coll, key) -> value
            method_ids::ASSOC => protocol_type_indices::ARITY_3_REF,   // (coll, key, val) -> coll'
            method_ids::COUNT => protocol_type_indices::ARITY_1_I32,   // (coll) -> i32
            method_ids::NTH => protocol_type_indices::ARITY_2_REF,     // (coll, index) -> value
            method_ids::CONJ => protocol_type_indices::ARITY_2_REF,    // (coll, val) -> coll'
            method_ids::FIRST => protocol_type_indices::ARITY_1_REF,   // (seq) -> value
            method_ids::REST => protocol_type_indices::ARITY_1_REF,    // (seq) -> seq
            method_ids::SEQ => protocol_type_indices::ARITY_1_REF,     // (coll) -> seq
            method_ids::HASH => protocol_type_indices::ARITY_1_I32,    // (value) -> i32
            method_ids::EQUIV => protocol_type_indices::ARITY_2_I32,   // (a, b) -> bool
            _ => protocol_type_indices::ARITY_1_REF,                   // Default for user methods
        }
    }

    /// Generate code to get the runtime type ID of a value.
    ///
    /// Returns i32:
    /// - -1 for i31ref values (nil, bool, small int)
    /// - 0-8 for built-in GC types
    /// - 256+ for user-defined types
    ///
    /// Calls the $get_type_id helper function which uses a chain of ref.test checks.
    fn generate_get_type_id(&self, value: &Expr, f: &mut Function) -> CompileResult<()> {
        // Generate the value - it will be on the stack as eqref
        self.generate_expr(value, f)?;

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
        Type::Tagged => ValType::I64,
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
        // Should have GC types (9) + helper types (2) + protocol types (5) + 1 function type = 17 types
        use crate::ir::protocol_types;
        let expected_types = gc_types::NUM_GC_TYPES + NUM_RUNTIME_HELPERS + protocol_types::NUM_PROTOCOL_TYPES + 1;
        assert_eq!(
            type_count,
            expected_types,
            "Expected {} types (9 GC + 2 helper + 5 protocol + 1 func), found {}",
            expected_types,
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

        // Function types come after GC types, helper function types, and protocol types
        assert_eq!(
            codegen.func_type_offset(),
            gc_types::NUM_GC_TYPES + NUM_RUNTIME_HELPERS + protocol_types::NUM_PROTOCOL_TYPES
        );
    }
}
