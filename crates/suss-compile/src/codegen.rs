//! WASM code generation from IR
//!
//! Generates a WASM component from the IR representation.

use wasm_encoder::{
    Module as WasmModule, CodeSection, FunctionSection, TypeSection, ExportSection,
    MemorySection, DataSection, GlobalSection, ImportSection,
    Function, Instruction, ValType, MemoryType, GlobalType, EntityType,
    ExportKind, DataSegment, DataSegmentMode,
};
use wit_parser::{Resolve, WorldId};

use crate::error::{CompileError, CompileResult};
use crate::ir::{Module, Function as IrFunc, Expr, Type, BinOp, UnOp};

/// Generate WASM bytes from IR module with WIT world
pub fn generate(ir: &Module, resolve: &Resolve, world_id: WorldId) -> CompileResult<Vec<u8>> {
    let mut codegen = CodeGen::new(ir, resolve, world_id);
    codegen.generate()
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
    let mut codegen = SimpleCodeGen::new(ir);
    codegen.generate()
}

/// Simplified code generator for expression compilation (no WIT)
struct SimpleCodeGen<'a> {
    ir: &'a Module,
    string_offset: u32,
}

struct CodeGen<'a> {
    ir: &'a Module,
    resolve: &'a Resolve,
    world_id: WorldId,
    /// String data offset in memory
    string_offset: u32,
    /// Number of imported functions
    num_imports: u32,
}

impl<'a> CodeGen<'a> {
    fn new(ir: &'a Module, resolve: &'a Resolve, world_id: WorldId) -> Self {
        Self {
            ir,
            resolve,
            world_id,
            string_offset: 0,
            num_imports: ir.imports.len() as u32,
        }
    }

    fn generate(&mut self) -> CompileResult<Vec<u8>> {
        let mut module = WasmModule::new();

        // Type section - function signatures (imports first, then local functions)
        let mut types = TypeSection::new();

        // Import function types first
        for import in &self.ir.imports {
            let params: Vec<ValType> = import.params.iter()
                .flat_map(|ty| self.type_to_valtypes(ty))
                .collect();
            let results = if import.return_type == Type::Unit {
                vec![]
            } else {
                self.type_to_valtypes(&import.return_type)
            };
            types.ty().function(params, results);
        }

        // Local function types
        for func in &self.ir.functions {
            let params: Vec<ValType> = func.params.iter()
                .flat_map(|(_, ty)| self.type_to_valtypes(ty))
                .collect();
            let results = self.type_to_valtypes(&func.return_type);
            types.ty().function(params, results);
        }
        module.section(&types);

        // Import section
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

        // Function section - declares local functions (type indices start after imports)
        let mut functions = FunctionSection::new();
        for (idx, _) in self.ir.functions.iter().enumerate() {
            functions.function((self.num_imports + idx as u32) as u32);
        }
        module.section(&functions);

        // Memory section
        let mut memory = MemorySection::new();
        memory.memory(MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        module.section(&memory);

        // Global section
        let mut globals = GlobalSection::new();
        // Add heap pointer global
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &wasm_encoder::ConstExpr::i32_const(0),
        );
        module.section(&globals);

        // Export section
        let mut exports = ExportSection::new();
        exports.export("memory", ExportKind::Memory, 0);

        for (idx, func) in self.ir.functions.iter().enumerate() {
            if func.exported {
                let name = func.export_name.as_deref().unwrap_or("unknown");
                // Function indices: imports come first, then local functions
                exports.export(name, ExportKind::Func, self.num_imports + idx as u32);
            }
        }
        module.section(&exports);

        // Code section
        let mut code = CodeSection::new();
        for func in &self.ir.functions {
            let function = self.generate_function(func)?;
            code.function(&function);
        }
        module.section(&code);

        // Data section - string literals
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

            // Update heap pointer to after strings
            self.string_offset = offset;
            module.section(&data);
        }

        Ok(module.finish())
    }

    fn generate_function(&self, func: &IrFunc) -> CompileResult<Function> {
        // Collect local types (excluding parameters which are already declared)
        let local_types: Vec<(u32, ValType)> = func.locals[func.params.len()..]
            .iter()
            .map(|ty| (1, self.type_to_valtype(ty)))
            .collect();

        let mut f = Function::new(local_types);

        // Generate body
        self.generate_expr(&func.body, &mut f)?;

        f.instruction(&Instruction::End);
        Ok(f)
    }

    fn generate_expr(&self, expr: &Expr, f: &mut Function) -> CompileResult<()> {
        match expr {
            Expr::Unit => {
                // Unit is represented as i32 0
                f.instruction(&Instruction::I32Const(0));
            }

            Expr::Bool(b) => {
                f.instruction(&Instruction::I32Const(if *b { 1 } else { 0 }));
            }

            Expr::Int(i) => {
                if *i >= i32::MIN as i64 && *i <= i32::MAX as i64 {
                    f.instruction(&Instruction::I32Const(*i as i32));
                } else {
                    f.instruction(&Instruction::I64Const(*i));
                }
            }

            Expr::Float(v) => {
                f.instruction(&Instruction::F64Const(*v));
            }

            Expr::String(idx) => {
                // Push string pointer and length
                // For now, strings are stored contiguously
                let mut offset = 0u32;
                for (i, s) in self.ir.strings.iter().enumerate() {
                    if i == *idx as usize {
                        break;
                    }
                    offset += s.len() as u32;
                }
                let len = self.ir.strings[*idx as usize].len() as u32;

                // Return (ptr, len) - for WIT strings this is a pair
                f.instruction(&Instruction::I32Const(offset as i32));
                f.instruction(&Instruction::I32Const(len as i32));
            }

            Expr::LocalGet(idx) => {
                f.instruction(&Instruction::LocalGet(*idx));
            }

            Expr::LocalSet(idx, value) => {
                self.generate_expr(value, f)?;
                f.instruction(&Instruction::LocalSet(*idx));
                f.instruction(&Instruction::I32Const(0)); // Return unit
            }

            Expr::GlobalGet(idx) => {
                f.instruction(&Instruction::GlobalGet(*idx));
            }

            Expr::GlobalSet(idx, value) => {
                self.generate_expr(value, f)?;
                f.instruction(&Instruction::GlobalSet(*idx));
                f.instruction(&Instruction::I32Const(0)); // Return unit
            }

            Expr::BinOp { op, left, right, ty } => {
                self.generate_expr(left, f)?;
                self.generate_expr(right, f)?;

                let instr = match (op, ty) {
                    (BinOp::Add, Type::I32) => Instruction::I32Add,
                    (BinOp::Add, Type::I64) => Instruction::I64Add,
                    (BinOp::Add, Type::F64) => Instruction::F64Add,
                    (BinOp::Sub, Type::I32) => Instruction::I32Sub,
                    (BinOp::Sub, Type::I64) => Instruction::I64Sub,
                    (BinOp::Sub, Type::F64) => Instruction::F64Sub,
                    (BinOp::Mul, Type::I32) => Instruction::I32Mul,
                    (BinOp::Mul, Type::I64) => Instruction::I64Mul,
                    (BinOp::Mul, Type::F64) => Instruction::F64Mul,
                    (BinOp::Div, Type::I32) => Instruction::I32DivS,
                    (BinOp::Div, Type::I64) => Instruction::I64DivS,
                    (BinOp::Div, Type::F64) => Instruction::F64Div,
                    (BinOp::Rem, Type::I32) => Instruction::I32RemS,
                    (BinOp::Rem, Type::I64) => Instruction::I64RemS,

                    (BinOp::Eq, _) => Instruction::I64Eq,
                    (BinOp::Ne, _) => Instruction::I64Ne,
                    (BinOp::Lt, _) => Instruction::I64LtS,
                    (BinOp::Le, _) => Instruction::I64LeS,
                    (BinOp::Gt, _) => Instruction::I64GtS,
                    (BinOp::Ge, _) => Instruction::I64GeS,

                    (BinOp::And, _) => Instruction::I32And,
                    (BinOp::Or, _) => Instruction::I32Or,

                    _ => {
                        return Err(CompileError::Codegen(format!(
                            "Unsupported binop {:?} for type {:?}",
                            op, ty
                        )));
                    }
                };

                f.instruction(&instr);
            }

            Expr::UnOp { op, operand, ty } => {
                self.generate_expr(operand, f)?;

                match op {
                    UnOp::Neg => {
                        match ty {
                            Type::I32 => {
                                f.instruction(&Instruction::I32Const(0));
                                f.instruction(&Instruction::I32Sub);
                            }
                            Type::I64 => {
                                f.instruction(&Instruction::I64Const(0));
                                f.instruction(&Instruction::I64Sub);
                            }
                            Type::F64 => {
                                f.instruction(&Instruction::F64Neg);
                            }
                            _ => {
                                return Err(CompileError::Codegen(
                                    "Neg not supported for this type".into()
                                ));
                            }
                        }
                    }
                    UnOp::Not => {
                        f.instruction(&Instruction::I32Eqz);
                    }
                }
            }

            Expr::Call { func, args } => {
                for arg in args {
                    self.generate_expr(arg, f)?;
                }
                f.instruction(&Instruction::Call(*func));
            }

            Expr::If { cond, then_branch, else_branch, ty } => {
                self.generate_expr(cond, f)?;

                let block_type = wasm_encoder::BlockType::Result(self.type_to_valtype(ty));

                f.instruction(&Instruction::If(block_type));
                self.generate_expr(then_branch, f)?;
                f.instruction(&Instruction::Else);
                self.generate_expr(else_branch, f)?;
                f.instruction(&Instruction::End);
            }

            Expr::Block(exprs) => {
                if exprs.is_empty() {
                    f.instruction(&Instruction::I32Const(0));
                } else {
                    for (i, expr) in exprs.iter().enumerate() {
                        self.generate_expr(expr, f)?;
                        // Drop all but last value
                        if i < exprs.len() - 1 {
                            f.instruction(&Instruction::Drop);
                        }
                    }
                }
            }

            Expr::Let { bindings, body } => {
                // Initialize locals
                for (idx, value) in bindings {
                    self.generate_expr(value, f)?;
                    f.instruction(&Instruction::LocalSet(*idx));
                }

                // Generate body
                self.generate_expr(body, f)?;
            }

            Expr::Loop { bindings, body } => {
                // Initialize loop variables
                for (idx, value) in bindings {
                    self.generate_expr(value, f)?;
                    f.instruction(&Instruction::LocalSet(*idx));
                }

                // Loop block
                f.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
                f.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));

                self.generate_expr(body, f)?;

                // Loop continues with br 0, exits with br 1
                f.instruction(&Instruction::End); // end loop
                f.instruction(&Instruction::End); // end block
            }

            Expr::Recur(values) => {
                // Update loop variables and branch back
                // This assumes we're inside a loop and know the variable indices
                // For now, this is a simplified implementation
                for (i, value) in values.iter().enumerate() {
                    self.generate_expr(value, f)?;
                    f.instruction(&Instruction::LocalSet(i as u32));
                }
                f.instruction(&Instruction::Br(0)); // Branch to loop start
            }

            Expr::StrConcat(parts) => {
                if parts.is_empty() {
                    // Empty string: (ptr=0, len=0)
                    f.instruction(&Instruction::I32Const(0));
                    f.instruction(&Instruction::I32Const(0));
                } else if parts.len() == 1 {
                    // Single part: just return it
                    self.generate_expr(&parts[0], f)?;
                } else {
                    // Multi-part string concatenation
                    // Strategy:
                    // 1. Evaluate all parts and collect (ptr, len) pairs
                    // 2. Calculate total length
                    // 3. Allocate memory from heap
                    // 4. Copy each part into allocated buffer
                    // 5. Return (result_ptr, total_len)

                    // Runtime string concatenation requires heap allocation
                    // This is complex because WASM needs locals declared upfront
                    // and strings are (ptr, len) pairs on the stack
                    //
                    // For now, only compile-time concatenation (all literals) is supported
                    // The lowering phase handles this optimization
                    return Err(CompileError::Unsupported(
                        "Runtime string concatenation not yet implemented. \
                         Use string literals only, e.g., (str \"Hello\" \" World\")".into()
                    ));
                }
            }

            Expr::Coerce { expr, from, to } => {
                self.generate_expr(expr, f)?;

                // Add type conversion if needed
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
                    _ => {
                        // No conversion needed or not supported
                    }
                }
            }
        }

        Ok(())
    }

    fn type_to_valtype(&self, ty: &Type) -> ValType {
        match ty {
            Type::Unit | Type::Bool | Type::I32 => ValType::I32,
            Type::I64 => ValType::I64,
            Type::F64 => ValType::F64,
            Type::String => ValType::I32, // Pointer (first of pair)
            Type::List(_) | Type::Vector(_) => ValType::I32, // Pointer
            Type::Func { .. } => ValType::I32, // Table index
            Type::Unknown => ValType::I32, // Default to i32
        }
    }

    /// Returns all ValTypes needed for a type (strings need ptr+len pair)
    fn type_to_valtypes(&self, ty: &Type) -> Vec<ValType> {
        match ty {
            Type::String => vec![ValType::I32, ValType::I32], // ptr, len
            Type::List(_) => vec![ValType::I32, ValType::I32], // ptr, len
            _ => vec![self.type_to_valtype(ty)],
        }
    }
}

// ============================================================================
// SimpleCodeGen - Standalone WASM module generation (no WIT)
// ============================================================================

impl<'a> SimpleCodeGen<'a> {
    fn new(ir: &'a Module) -> Self {
        Self {
            ir,
            string_offset: 0,
        }
    }

    fn generate(&mut self) -> CompileResult<Vec<u8>> {
        let mut module = WasmModule::new();

        // Type section - function signatures
        let mut types = TypeSection::new();
        for func in &self.ir.functions {
            let params: Vec<ValType> = func.params.iter()
                .flat_map(|(_, ty)| self.type_to_valtypes(ty))
                .collect();
            let results = self.type_to_valtypes(&func.return_type);
            types.ty().function(params, results);
        }
        module.section(&types);

        // Function section - declares functions
        let mut functions = FunctionSection::new();
        for (idx, _) in self.ir.functions.iter().enumerate() {
            functions.function(idx as u32);
        }
        module.section(&functions);

        // Memory section
        let mut memory = MemorySection::new();
        memory.memory(MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        module.section(&memory);

        // Global section - heap pointer
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

        // Export section
        let mut exports = ExportSection::new();
        exports.export("memory", ExportKind::Memory, 0);

        for (idx, func) in self.ir.functions.iter().enumerate() {
            if func.exported {
                let name = func.export_name.as_deref().unwrap_or(&func.name);
                exports.export(name, ExportKind::Func, idx as u32);
            }
        }
        module.section(&exports);

        // Code section
        let mut code = CodeSection::new();
        for func in &self.ir.functions {
            let function = self.generate_function(func)?;
            code.function(&function);
        }
        module.section(&code);

        // Data section - string literals
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

            self.string_offset = offset;
            module.section(&data);
        }

        Ok(module.finish())
    }

    fn generate_function(&self, func: &IrFunc) -> CompileResult<Function> {
        let local_types: Vec<(u32, ValType)> = func.locals[func.params.len()..]
            .iter()
            .map(|ty| (1, self.type_to_valtype(ty)))
            .collect();

        let mut f = Function::new(local_types);
        self.generate_expr(&func.body, &mut f)?;
        f.instruction(&Instruction::End);
        Ok(f)
    }

    fn generate_expr(&self, expr: &Expr, f: &mut Function) -> CompileResult<()> {
        match expr {
            Expr::Unit => {
                f.instruction(&Instruction::I32Const(0));
            }

            Expr::Bool(b) => {
                f.instruction(&Instruction::I32Const(if *b { 1 } else { 0 }));
            }

            Expr::Int(i) => {
                if *i >= i32::MIN as i64 && *i <= i32::MAX as i64 {
                    f.instruction(&Instruction::I32Const(*i as i32));
                } else {
                    f.instruction(&Instruction::I64Const(*i));
                }
            }

            Expr::Float(v) => {
                f.instruction(&Instruction::F64Const(*v));
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

            Expr::LocalGet(idx) => {
                f.instruction(&Instruction::LocalGet(*idx));
            }

            Expr::LocalSet(idx, value) => {
                self.generate_expr(value, f)?;
                f.instruction(&Instruction::LocalSet(*idx));
                f.instruction(&Instruction::I32Const(0));
            }

            Expr::GlobalGet(idx) => {
                f.instruction(&Instruction::GlobalGet(*idx));
            }

            Expr::GlobalSet(idx, value) => {
                self.generate_expr(value, f)?;
                f.instruction(&Instruction::GlobalSet(*idx));
                f.instruction(&Instruction::I32Const(0));
            }

            Expr::BinOp { op, left, right, ty } => {
                self.generate_expr(left, f)?;
                self.generate_expr(right, f)?;

                let instr = match (op, ty) {
                    (BinOp::Add, Type::I32) => Instruction::I32Add,
                    (BinOp::Add, Type::I64) => Instruction::I64Add,
                    (BinOp::Add, Type::F64) => Instruction::F64Add,
                    (BinOp::Sub, Type::I32) => Instruction::I32Sub,
                    (BinOp::Sub, Type::I64) => Instruction::I64Sub,
                    (BinOp::Sub, Type::F64) => Instruction::F64Sub,
                    (BinOp::Mul, Type::I32) => Instruction::I32Mul,
                    (BinOp::Mul, Type::I64) => Instruction::I64Mul,
                    (BinOp::Mul, Type::F64) => Instruction::F64Mul,
                    (BinOp::Div, Type::I32) => Instruction::I32DivS,
                    (BinOp::Div, Type::I64) => Instruction::I64DivS,
                    (BinOp::Div, Type::F64) => Instruction::F64Div,
                    (BinOp::Rem, Type::I32) => Instruction::I32RemS,
                    (BinOp::Rem, Type::I64) => Instruction::I64RemS,

                    (BinOp::Eq, Type::I32) => Instruction::I32Eq,
                    (BinOp::Eq, Type::I64) => Instruction::I64Eq,
                    (BinOp::Eq, _) => Instruction::I32Eq,
                    (BinOp::Ne, Type::I32) => Instruction::I32Ne,
                    (BinOp::Ne, Type::I64) => Instruction::I64Ne,
                    (BinOp::Ne, _) => Instruction::I32Ne,
                    (BinOp::Lt, Type::I32) => Instruction::I32LtS,
                    (BinOp::Lt, Type::I64) => Instruction::I64LtS,
                    (BinOp::Lt, _) => Instruction::I32LtS,
                    (BinOp::Le, Type::I32) => Instruction::I32LeS,
                    (BinOp::Le, Type::I64) => Instruction::I64LeS,
                    (BinOp::Le, _) => Instruction::I32LeS,
                    (BinOp::Gt, Type::I32) => Instruction::I32GtS,
                    (BinOp::Gt, Type::I64) => Instruction::I64GtS,
                    (BinOp::Gt, _) => Instruction::I32GtS,
                    (BinOp::Ge, Type::I32) => Instruction::I32GeS,
                    (BinOp::Ge, Type::I64) => Instruction::I64GeS,
                    (BinOp::Ge, _) => Instruction::I32GeS,

                    (BinOp::And, _) => Instruction::I32And,
                    (BinOp::Or, _) => Instruction::I32Or,

                    _ => {
                        return Err(CompileError::Codegen(format!(
                            "Unsupported binop {:?} for type {:?}",
                            op, ty
                        )));
                    }
                };

                f.instruction(&instr);
            }

            Expr::UnOp { op, operand, ty } => {
                self.generate_expr(operand, f)?;

                match op {
                    UnOp::Neg => {
                        match ty {
                            Type::I32 => {
                                f.instruction(&Instruction::I32Const(0));
                                f.instruction(&Instruction::I32Sub);
                            }
                            Type::I64 => {
                                f.instruction(&Instruction::I64Const(0));
                                f.instruction(&Instruction::I64Sub);
                            }
                            Type::F64 => {
                                f.instruction(&Instruction::F64Neg);
                            }
                            _ => {
                                return Err(CompileError::Codegen(
                                    "Neg not supported for this type".into()
                                ));
                            }
                        }
                    }
                    UnOp::Not => {
                        f.instruction(&Instruction::I32Eqz);
                    }
                }
            }

            Expr::Call { func, args } => {
                for arg in args {
                    self.generate_expr(arg, f)?;
                }
                f.instruction(&Instruction::Call(*func));
            }

            Expr::If { cond, then_branch, else_branch, ty } => {
                self.generate_expr(cond, f)?;

                let block_type = wasm_encoder::BlockType::Result(self.type_to_valtype(ty));

                f.instruction(&Instruction::If(block_type));
                self.generate_expr(then_branch, f)?;
                f.instruction(&Instruction::Else);
                self.generate_expr(else_branch, f)?;
                f.instruction(&Instruction::End);
            }

            Expr::Block(exprs) => {
                if exprs.is_empty() {
                    f.instruction(&Instruction::I32Const(0));
                } else {
                    for (i, expr) in exprs.iter().enumerate() {
                        self.generate_expr(expr, f)?;
                        if i < exprs.len() - 1 {
                            f.instruction(&Instruction::Drop);
                        }
                    }
                }
            }

            Expr::Let { bindings, body } => {
                for (idx, value) in bindings {
                    self.generate_expr(value, f)?;
                    f.instruction(&Instruction::LocalSet(*idx));
                }
                self.generate_expr(body, f)?;
            }

            Expr::Loop { bindings, body } => {
                for (idx, value) in bindings {
                    self.generate_expr(value, f)?;
                    f.instruction(&Instruction::LocalSet(*idx));
                }

                f.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
                f.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));

                self.generate_expr(body, f)?;

                f.instruction(&Instruction::End);
                f.instruction(&Instruction::End);
            }

            Expr::Recur(values) => {
                for (i, value) in values.iter().enumerate() {
                    self.generate_expr(value, f)?;
                    f.instruction(&Instruction::LocalSet(i as u32));
                }
                f.instruction(&Instruction::Br(0));
            }

            Expr::StrConcat(parts) => {
                if parts.is_empty() {
                    f.instruction(&Instruction::I32Const(0));
                    f.instruction(&Instruction::I32Const(0));
                } else if parts.len() == 1 {
                    self.generate_expr(&parts[0], f)?;
                } else {
                    return Err(CompileError::Unsupported(
                        "Runtime string concatenation not yet implemented".into()
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
        }

        Ok(())
    }

    fn type_to_valtype(&self, ty: &Type) -> ValType {
        match ty {
            Type::Unit | Type::Bool | Type::I32 => ValType::I32,
            Type::I64 => ValType::I64,
            Type::F64 => ValType::F64,
            Type::String => ValType::I32,
            Type::List(_) | Type::Vector(_) => ValType::I32,
            Type::Func { .. } => ValType::I32,
            Type::Unknown => ValType::I32,
        }
    }

    fn type_to_valtypes(&self, ty: &Type) -> Vec<ValType> {
        match ty {
            Type::String => vec![ValType::I32, ValType::I32],
            Type::List(_) => vec![ValType::I32, ValType::I32],
            _ => vec![self.type_to_valtype(ty)],
        }
    }
}

// ============================================================================
// Component generation with WASI imports (for expression compilation)
// ============================================================================

/// Generate a WASM Component with imports for standalone expression compilation
///
/// This creates a WASM Component that imports WASI functions and exports an `eval` function.
/// Used for expressions like `(wasi.random/get-random-u64)`.
pub fn generate_component_with_imports(ir: &Module) -> CompileResult<Vec<u8>> {
    use wit_component::{ComponentEncoder, StringEncoding, metadata};
    use wit_parser::Resolve;

    // Generate core module with imports
    let core_wasm = generate_core_with_imports(ir)?;

    // Build a synthetic WIT world for the expression
    let wit_source = build_synthetic_wit_world(ir)?;

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
    let pkg_id = resolve.push_str("expr.wit", &wit_source)
        .map_err(|e| CompileError::Component(format!("Failed to parse synthetic WIT: {}", e)))?;

    let pkg = &resolve.packages[pkg_id];
    let world_id = pkg.worlds.values().next()
        .ok_or_else(|| CompileError::Component("No world in synthetic WIT".to_string()))?;

    // Encode WIT metadata and append to module
    let encoded_metadata = metadata::encode(&resolve, *world_id, StringEncoding::UTF8, None)
        .map_err(|e| CompileError::Component(format!("Failed to encode metadata: {}", e)))?;

    // Append metadata as custom section
    let module_with_meta = append_metadata_section(&core_wasm, &encoded_metadata)?;

    // Create component
    let mut encoder = ComponentEncoder::default()
        .validate(true)
        .module(&module_with_meta)
        .map_err(|e| CompileError::Component(format!("Failed to encode component: {}", e)))?;

    encoder.encode()
        .map_err(|e| CompileError::Component(format!("Failed to finalize component: {}", e)))
}

/// Build a synthetic WIT world definition for an expression with WASI imports
fn build_synthetic_wit_world(ir: &Module) -> CompileResult<String> {
    let mut wit = String::new();

    wit.push_str("package suss:expr;\n\n");
    wit.push_str("world expr {\n");

    // Add WASI imports
    for import in &ir.imports {
        // Parse wit_interface like "wasi:random/random@0.2.0"
        // We need to write: import wasi:random/random@0.2.0;
        wit.push_str(&format!("    import {};\n", import.wit_interface));
    }

    // Add eval export based on return type
    if let Some(func) = ir.functions.first() {
        let return_wit = type_to_wit_string(&func.return_type);
        wit.push_str(&format!("    export eval: func() -> {};\n", return_wit));
    }

    wit.push_str("}\n");

    Ok(wit)
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

/// Append metadata as a custom section to a WASM module
fn append_metadata_section(wasm: &[u8], metadata: &[u8]) -> CompileResult<Vec<u8>> {
    use wasm_encoder::{Module as WasmModule, CustomSection, RawSection};

    // Parse the original module sections and re-encode with custom section
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

/// Generate a core WASM module with import section (not a component)
fn generate_core_with_imports(ir: &Module) -> CompileResult<Vec<u8>> {
    let mut module = WasmModule::new();

    let num_imports = ir.imports.len() as u32;

    // Type section - import function types first, then local function types
    let mut types = TypeSection::new();

    // Import function types
    for import in &ir.imports {
        let params: Vec<ValType> = import.params.iter()
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
    for func in &ir.functions {
        let params: Vec<ValType> = func.params.iter()
            .flat_map(|(_, ty)| type_to_valtypes(ty))
            .collect();
        let results = type_to_valtypes(&func.return_type);
        types.ty().function(params, results);
    }
    module.section(&types);

    // Import section - WASI functions
    if !ir.imports.is_empty() {
        let mut imports = ImportSection::new();
        for (idx, import) in ir.imports.iter().enumerate() {
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
    for (idx, _) in ir.functions.iter().enumerate() {
        functions.function(num_imports + idx as u32);
    }
    module.section(&functions);

    // Memory section
    let mut memory = MemorySection::new();
    memory.memory(MemoryType {
        minimum: 1,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memory);

    // Global section - heap pointer
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

    // Export section
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);

    for (idx, func) in ir.functions.iter().enumerate() {
        if func.exported {
            let name = func.export_name.as_deref().unwrap_or(&func.name);
            exports.export(name, ExportKind::Func, num_imports + idx as u32);
        }
    }
    module.section(&exports);

    // Code section
    let mut code = CodeSection::new();
    for func in &ir.functions {
        let function = generate_function_with_imports(func, &ir.strings, num_imports)?;
        code.function(&function);
    }
    module.section(&code);

    // Data section - string literals
    if !ir.strings.is_empty() {
        let mut data = DataSection::new();
        let mut offset = 0u32;

        for s in &ir.strings {
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

    Ok(module.finish())
}

/// Generate a function body (with import awareness)
fn generate_function_with_imports(
    func: &IrFunc,
    strings: &[String],
    _num_imports: u32,
) -> CompileResult<Function> {
    let local_types: Vec<(u32, ValType)> = func.locals[func.params.len()..]
        .iter()
        .map(|ty| (1, type_to_valtype(ty)))
        .collect();

    let mut f = Function::new(local_types);
    generate_expr_with_imports(&func.body, &mut f, strings)?;
    f.instruction(&Instruction::End);
    Ok(f)
}

/// Generate expression code (with import awareness)
fn generate_expr_with_imports(
    expr: &Expr,
    f: &mut Function,
    strings: &[String],
) -> CompileResult<()> {
    match expr {
        Expr::Unit => {
            f.instruction(&Instruction::I32Const(0));
        }

        Expr::Bool(b) => {
            f.instruction(&Instruction::I32Const(if *b { 1 } else { 0 }));
        }

        Expr::Int(i) => {
            if *i >= i32::MIN as i64 && *i <= i32::MAX as i64 {
                f.instruction(&Instruction::I32Const(*i as i32));
            } else {
                f.instruction(&Instruction::I64Const(*i));
            }
        }

        Expr::Float(v) => {
            f.instruction(&Instruction::F64Const(*v));
        }

        Expr::String(idx) => {
            let mut offset = 0u32;
            for (i, s) in strings.iter().enumerate() {
                if i == *idx as usize {
                    break;
                }
                offset += s.len() as u32;
            }
            let len = strings[*idx as usize].len() as u32;

            f.instruction(&Instruction::I32Const(offset as i32));
            f.instruction(&Instruction::I32Const(len as i32));
        }

        Expr::LocalGet(idx) => {
            f.instruction(&Instruction::LocalGet(*idx));
        }

        Expr::LocalSet(idx, value) => {
            generate_expr_with_imports(value, f, strings)?;
            f.instruction(&Instruction::LocalSet(*idx));
            f.instruction(&Instruction::I32Const(0));
        }

        Expr::GlobalGet(idx) => {
            f.instruction(&Instruction::GlobalGet(*idx));
        }

        Expr::GlobalSet(idx, value) => {
            generate_expr_with_imports(value, f, strings)?;
            f.instruction(&Instruction::GlobalSet(*idx));
            f.instruction(&Instruction::I32Const(0));
        }

        Expr::BinOp { op, left, right, ty } => {
            generate_expr_with_imports(left, f, strings)?;
            generate_expr_with_imports(right, f, strings)?;

            let instr = match (op, ty) {
                (BinOp::Add, Type::I32) => Instruction::I32Add,
                (BinOp::Add, Type::I64) => Instruction::I64Add,
                (BinOp::Add, Type::F64) => Instruction::F64Add,
                (BinOp::Sub, Type::I32) => Instruction::I32Sub,
                (BinOp::Sub, Type::I64) => Instruction::I64Sub,
                (BinOp::Sub, Type::F64) => Instruction::F64Sub,
                (BinOp::Mul, Type::I32) => Instruction::I32Mul,
                (BinOp::Mul, Type::I64) => Instruction::I64Mul,
                (BinOp::Mul, Type::F64) => Instruction::F64Mul,
                (BinOp::Div, Type::I32) => Instruction::I32DivS,
                (BinOp::Div, Type::I64) => Instruction::I64DivS,
                (BinOp::Div, Type::F64) => Instruction::F64Div,
                (BinOp::Rem, Type::I32) => Instruction::I32RemS,
                (BinOp::Rem, Type::I64) => Instruction::I64RemS,

                (BinOp::Eq, Type::I32) => Instruction::I32Eq,
                (BinOp::Eq, Type::I64) => Instruction::I64Eq,
                (BinOp::Eq, _) => Instruction::I32Eq,
                (BinOp::Ne, Type::I32) => Instruction::I32Ne,
                (BinOp::Ne, Type::I64) => Instruction::I64Ne,
                (BinOp::Ne, _) => Instruction::I32Ne,
                (BinOp::Lt, Type::I32) => Instruction::I32LtS,
                (BinOp::Lt, Type::I64) => Instruction::I64LtS,
                (BinOp::Lt, _) => Instruction::I32LtS,
                (BinOp::Le, Type::I32) => Instruction::I32LeS,
                (BinOp::Le, Type::I64) => Instruction::I64LeS,
                (BinOp::Le, _) => Instruction::I32LeS,
                (BinOp::Gt, Type::I32) => Instruction::I32GtS,
                (BinOp::Gt, Type::I64) => Instruction::I64GtS,
                (BinOp::Gt, _) => Instruction::I32GtS,
                (BinOp::Ge, Type::I32) => Instruction::I32GeS,
                (BinOp::Ge, Type::I64) => Instruction::I64GeS,
                (BinOp::Ge, _) => Instruction::I32GeS,

                (BinOp::And, _) => Instruction::I32And,
                (BinOp::Or, _) => Instruction::I32Or,

                _ => {
                    return Err(CompileError::Codegen(format!(
                        "Unsupported binop {:?} for type {:?}",
                        op, ty
                    )));
                }
            };

            f.instruction(&instr);
        }

        Expr::UnOp { op, operand, ty } => {
            generate_expr_with_imports(operand, f, strings)?;

            match op {
                UnOp::Neg => {
                    match ty {
                        Type::I32 => {
                            f.instruction(&Instruction::I32Const(0));
                            f.instruction(&Instruction::I32Sub);
                        }
                        Type::I64 => {
                            f.instruction(&Instruction::I64Const(0));
                            f.instruction(&Instruction::I64Sub);
                        }
                        Type::F64 => {
                            f.instruction(&Instruction::F64Neg);
                        }
                        _ => {
                            return Err(CompileError::Codegen(
                                "Neg not supported for this type".into()
                            ));
                        }
                    }
                }
                UnOp::Not => {
                    f.instruction(&Instruction::I32Eqz);
                }
            }
        }

        Expr::Call { func, args } => {
            for arg in args {
                generate_expr_with_imports(arg, f, strings)?;
            }
            f.instruction(&Instruction::Call(*func));
        }

        Expr::If { cond, then_branch, else_branch, ty } => {
            generate_expr_with_imports(cond, f, strings)?;

            let block_type = wasm_encoder::BlockType::Result(type_to_valtype(ty));

            f.instruction(&Instruction::If(block_type));
            generate_expr_with_imports(then_branch, f, strings)?;
            f.instruction(&Instruction::Else);
            generate_expr_with_imports(else_branch, f, strings)?;
            f.instruction(&Instruction::End);
        }

        Expr::Block(exprs) => {
            if exprs.is_empty() {
                f.instruction(&Instruction::I32Const(0));
            } else {
                for (i, expr) in exprs.iter().enumerate() {
                    generate_expr_with_imports(expr, f, strings)?;
                    if i < exprs.len() - 1 {
                        f.instruction(&Instruction::Drop);
                    }
                }
            }
        }

        Expr::Let { bindings, body } => {
            for (idx, value) in bindings {
                generate_expr_with_imports(value, f, strings)?;
                f.instruction(&Instruction::LocalSet(*idx));
            }
            generate_expr_with_imports(body, f, strings)?;
        }

        Expr::Loop { bindings, body } => {
            for (idx, value) in bindings {
                generate_expr_with_imports(value, f, strings)?;
                f.instruction(&Instruction::LocalSet(*idx));
            }

            f.instruction(&Instruction::Block(wasm_encoder::BlockType::Empty));
            f.instruction(&Instruction::Loop(wasm_encoder::BlockType::Empty));

            generate_expr_with_imports(body, f, strings)?;

            f.instruction(&Instruction::End);
            f.instruction(&Instruction::End);
        }

        Expr::Recur(values) => {
            for (i, value) in values.iter().enumerate() {
                generate_expr_with_imports(value, f, strings)?;
                f.instruction(&Instruction::LocalSet(i as u32));
            }
            f.instruction(&Instruction::Br(0));
        }

        Expr::StrConcat(parts) => {
            if parts.is_empty() {
                f.instruction(&Instruction::I32Const(0));
                f.instruction(&Instruction::I32Const(0));
            } else if parts.len() == 1 {
                generate_expr_with_imports(&parts[0], f, strings)?;
            } else {
                return Err(CompileError::Unsupported(
                    "Runtime string concatenation not yet implemented".into()
                ));
            }
        }

        Expr::Coerce { expr, from, to } => {
            generate_expr_with_imports(expr, f, strings)?;

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
    }

    Ok(())
}

/// Helper: Convert IR type to WASM ValType
fn type_to_valtype(ty: &Type) -> ValType {
    match ty {
        Type::Unit | Type::Bool | Type::I32 => ValType::I32,
        Type::I64 => ValType::I64,
        Type::F64 => ValType::F64,
        Type::String => ValType::I32,
        Type::List(_) | Type::Vector(_) => ValType::I32,
        Type::Func { .. } => ValType::I32,
        Type::Unknown => ValType::I32,
    }
}

/// Helper: Get all ValTypes for a type (strings need ptr+len pair)
fn type_to_valtypes(ty: &Type) -> Vec<ValType> {
    match ty {
        Type::String => vec![ValType::I32, ValType::I32],
        Type::List(_) => vec![ValType::I32, ValType::I32],
        _ => vec![type_to_valtype(ty)],
    }
}
