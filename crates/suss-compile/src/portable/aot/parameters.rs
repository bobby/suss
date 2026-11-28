//! Canonical indirect parameter records, laid out by the pinned WIT resolver.
//! The adapter first reads flat locals, then uses its ordinary owned-value
//! conversions. Both the parameter record and nested buffers are released
//! before invoking source code.
use super::{Boundary, Diagnostic, Scalar, error};
use wasm_encoder::{Function, Instruction, MemArg};
use wit_parser::{Resolve, SizeAlign};

pub(super) struct Layout {
    offsets: Vec<u32>,
    pub size: u32,
}

impl Layout {
    pub fn new(resolve: &Resolve, function: &wit_parser::Function) -> Result<Self, Diagnostic> {
        let mut sizes = SizeAlign::default();
        sizes.fill(resolve);
        let offsets = sizes
            .field_offsets(function.params.iter().map(|param| &param.ty))
            .into_iter()
            .map(|(offset, _)| u32::try_from(offset.size_wasm32()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| error("WIT parameter record exceeds memory32"))?;
        let size = u32::try_from(
            sizes
                .params(function.params.iter().map(|param| &param.ty))
                .size
                .size_wasm32(),
        )
        .map_err(|_| error("WIT parameter record exceeds memory32"))?;
        Ok(Self { offsets, size })
    }

    /// Local 0 is the incoming canonical record pointer. Flat parameter locals
    /// start at 1; the normal source adapter's scratch locals follow them.
    pub fn read(&self, body: &mut Function, params: &[(String, Boundary)]) {
        let mut local = 1;
        for ((_, ty), offset) in params.iter().zip(&self.offsets) {
            for load in loads(*ty, *offset) {
                body.instruction(&Instruction::LocalGet(0))
                    .instruction(&load)
                    .instruction(&Instruction::LocalSet(local));
                local += 1;
            }
        }
    }
}

fn memory(offset: u32, align: u32) -> MemArg {
    MemArg {
        offset: u64::from(offset),
        align,
        memory_index: 0,
    }
}

fn scalar_load(ty: Scalar, offset: u32) -> Instruction<'static> {
    use Instruction::*;
    match ty {
        Scalar::Bool | Scalar::U8 => I32Load8U(memory(offset, 0)),
        Scalar::S8 => I32Load8S(memory(offset, 0)),
        Scalar::U16 => I32Load16U(memory(offset, 1)),
        Scalar::S16 => I32Load16S(memory(offset, 1)),
        Scalar::Char | Scalar::U32 | Scalar::S32 => I32Load(memory(offset, 2)),
        Scalar::F32 => F32Load(memory(offset, 2)),
        Scalar::F64 => F64Load(memory(offset, 3)),
        Scalar::U64 | Scalar::S64 => I64Load(memory(offset, 3)),
    }
}

fn loads(ty: Boundary, offset: u32) -> Vec<Instruction<'static>> {
    use Instruction::*;
    match ty {
        Boundary::Scalar(ty) => vec![scalar_load(ty, offset)],
        Boundary::Option(ty) => vec![
            I32Load8U(memory(offset, 0)),
            scalar_load(ty, offset + ty.payload_memory().offset as u32),
        ],
        Boundary::String | Boundary::List(_) => {
            vec![I32Load(memory(offset, 2)), I32Load(memory(offset + 4, 2))]
        }
        Boundary::StringOption => vec![
            I32Load8U(memory(offset, 0)),
            I32Load(memory(offset + 4, 2)),
            I32Load(memory(offset + 8, 2)),
        ],
    }
}
