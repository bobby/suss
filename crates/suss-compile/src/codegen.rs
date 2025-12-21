//! WASM code generation from IR
//!
//! Generates a WASM component from the IR representation.

use wasm_encoder::{
    Module as WasmModule, CodeSection, FunctionSection, TypeSection, ExportSection,
    MemorySection, DataSection, GlobalSection,
    Function, Instruction, ValType, MemoryType, GlobalType,
    ExportKind, DataSegment, DataSegmentMode,
};
use wit_parser::{Resolve, WorldId};

use crate::error::{CompileError, CompileResult};
use crate::ir::{Module, Function as IrFunc, Expr, Type, BinOp, UnOp};

/// Generate WASM bytes from IR module
pub fn generate(ir: &Module, resolve: &Resolve, world_id: WorldId) -> CompileResult<Vec<u8>> {
    let mut codegen = CodeGen::new(ir, resolve, world_id);
    codegen.generate()
}

struct CodeGen<'a> {
    ir: &'a Module,
    resolve: &'a Resolve,
    world_id: WorldId,
    /// String data offset in memory
    string_offset: u32,
}

impl<'a> CodeGen<'a> {
    fn new(ir: &'a Module, resolve: &'a Resolve, world_id: WorldId) -> Self {
        Self {
            ir,
            resolve,
            world_id,
            string_offset: 0,
        }
    }

    fn generate(&mut self) -> CompileResult<Vec<u8>> {
        let mut module = WasmModule::new();

        // Type section - function signatures
        let mut types = TypeSection::new();
        for func in &self.ir.functions {
            let params: Vec<ValType> = func.params.iter()
                .map(|(_, ty)| self.type_to_valtype(ty))
                .collect();
            let results = vec![self.type_to_valtype(&func.return_type)];
            types.ty().function(params, results);
        }
        module.section(&types);

        // Import section (for now empty - will add WASI imports later)
        // let imports = ImportSection::new();
        // module.section(&imports);

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
                // String concatenation requires runtime support
                // For now, just return the first part if there is one
                if parts.is_empty() {
                    f.instruction(&Instruction::I32Const(0));
                    f.instruction(&Instruction::I32Const(0));
                } else if parts.len() == 1 {
                    self.generate_expr(&parts[0], f)?;
                } else {
                    // TODO: Implement proper string concatenation
                    return Err(CompileError::Unsupported(
                        "String concatenation not yet implemented".into()
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
            Type::String => ValType::I32, // Pointer
            Type::List(_) | Type::Vector(_) => ValType::I32, // Pointer
            Type::Func { .. } => ValType::I32, // Table index
            Type::Unknown => ValType::I32, // Default to i32
        }
    }
}
