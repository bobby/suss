//! Invocation-local integer SSA scratch; never change IR or postpone coercion.
use super::{ir, Diagnostic, Function, Instruction, IrFunction};
use crate::portable::hir::Bitwise;

type Meter<'a> = dyn FnMut(u64) -> Result<(), Diagnostic> + 'a;

pub(super) fn integer(operation: Bitwise) -> bool {
    matches!(operation, Bitwise::Int | Bitwise::And | Bitwise::Or | Bitwise::Xor
        | Bitwise::AndNot | Bitwise::Not | Bitwise::Clear | Bitwise::Flip
        | Bitwise::Set | Bitwise::Left | Bitwise::Right | Bitwise::Unsigned | Bitwise::Imul)
}

#[derive(Clone, Copy)]
enum Use { Unused, Once { block: usize, instruction: usize }, Escapes }
#[derive(Default)]
pub(super) struct Plan { slots: Vec<Option<u32>>, pub count: u32 }
impl Plan {
    pub fn slot(&self, id: ir::ValueId) -> Option<u32> {
        self.slots.get(id.0).copied().flatten()
    }
    pub fn new(ir: &IrFunction, enabled: bool, meter: &mut Meter<'_>) -> Result<Self, Diagnostic> {
        if !enabled { return Ok(Self::default()); }
        let fail = |message: &str| Diagnostic { span: ir.span.clone(), message: message.into() };
        // Avoid arrays unless a literal-separated integer pair could qualify.
        let mut present = false;
        'find: for block in &ir.blocks {
            meter(1)?;
            let mut previous_integer = false;
            for inst in &block.instructions {
                meter(1)?;
                match &inst.operation {
                    ir::Operation::Bitwise { operation, .. } if integer(*operation) => {
                        if previous_integer { present = true; break 'find; }
                        previous_integer = true;
                    }
                    ir::Operation::Literal(_) => {},
                    _ => previous_integer = false,
                }
            }
        }
        if !present { return Ok(Self::default()); }
        // Pay both requested backing stores, per-entry initialization, and the
        // stack-owned Vec/Plan metadata. Four units cover reserve/drop controls.
        let metadata = std::mem::size_of::<Vec<Use>>()
            .checked_add(std::mem::size_of::<Self>())
            .ok_or_else(|| fail("Integer SSA plan metadata overflow"))?;
        let work = ir.values.len().checked_mul(std::mem::size_of::<Use>() + std::mem::size_of::<Option<u32>>() + 2)
            .and_then(|n| n.checked_add(metadata)).and_then(|n| n.checked_add(4)).and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| fail("Integer SSA plan storage overflow"))?;
        meter(work)?;
        let mut uses = Vec::new();
        uses.try_reserve_exact(ir.values.len()).map_err(|_| fail("Unable to reserve integer SSA uses"))?;
        uses.resize(ir.values.len(), Use::Unused);
        let mut slots = Vec::new();
        slots.try_reserve_exact(ir.values.len()).map_err(|_| fail("Unable to reserve integer SSA locals"))?;
        slots.resize(ir.values.len(), None);
        for (block_id, block) in ir.blocks.iter().enumerate() {
            meter(1)?;
            for (position, inst) in block.instructions.iter().enumerate() {
                meter(1)?;
                let site = if matches!(&inst.operation, ir::Operation::Bitwise { operation, .. } if integer(*operation)) {
                    Use::Once { block: block_id, instruction: position }
                } else { Use::Escapes };
                for arg in ir::operands(&inst.operation) {
                    meter(1)?;
                    uses[arg.0] = match uses[arg.0] { Use::Unused => site, _ => Use::Escapes };
                }
            }
            let exits = match &block.terminator {
                ir::Terminator::Return(value) | ir::Terminator::Throw(value) => std::slice::from_ref(value),
                ir::Terminator::Jump { arguments, .. } => arguments,
                ir::Terminator::Branch { condition, .. } => std::slice::from_ref(condition),
            };
            for arg in exits { meter(1)?; uses[arg.0] = Use::Escapes; }
        }
        let mut count = 0u32;
        for (block_id, block) in ir.blocks.iter().enumerate() {
            meter(1)?;
            for (position, inst) in block.instructions.iter().enumerate() {
                meter(1)?;
                if !matches!(&inst.operation, ir::Operation::Bitwise { operation, .. } if integer(*operation)) { continue; }
                let Use::Once { block: consumer_block, instruction: consumer } = uses[inst.result.0] else { continue; };
                if consumer_block != block_id || consumer <= position { continue; }
                let mut clear = true;
                // Each range stops at the next nonliteral (including another
                // producer). Thus literal runs are scanned at most once: this
                // remains linear rather than a producer-by-block quadratic scan.
                for between in &block.instructions[position + 1..consumer] {
                    meter(1)?;
                    // Explicit whitelist. Other bitwise ops can coerce objects;
                    // mutable reads, calls, captures and unknown ops are barriers.
                    if !matches!(&between.operation, ir::Operation::Literal(_)) { clear = false; break; }
                }
                if clear {
                    meter(1)?;
                    slots[inst.result.0] = Some(count);
                    count = count.checked_add(1).ok_or_else(|| fail("Too many integer SSA locals"))?;
                }
            }
        }
        Ok(Self { slots, count })
    }
}

/// The producer is emitted at its original instruction. Unknown inputs still
/// call the original coercion left-to-right; only known integer results skip it.
pub(super) fn emit(
    function: &mut Function, plan: &Plan, operation: Bitwise, arguments: &[ir::ValueId],
    result: ir::ValueId, offset: u32, raw_base: u32, temporary: u32,
    index: &impl Fn(&str) -> u32,
) {
    use Instruction::*;
    for (position, argument) in arguments.iter().enumerate() {
        if let Some(slot) = plan.slot(*argument) {
            function.instruction(&LocalGet(raw_base + slot));
        } else {
            function.instruction(&LocalGet(argument.0 as u32 + offset))
                .instruction(&Call(index("coerce-int32")));
        }
        function.instruction(&LocalSet(temporary + position as u32));
    }
    function.instruction(&LocalGet(temporary));
    match operation {
        Bitwise::Int => {},
        Bitwise::Not => { function.instruction(&I32Const(-1)).instruction(&I32Xor); },
        Bitwise::Clear | Bitwise::Flip | Bitwise::Set => {
            function.instruction(&I32Const(1)).instruction(&LocalGet(temporary + 1)).instruction(&I32Shl);
            match operation {
                Bitwise::Clear => { function.instruction(&I32Const(-1)).instruction(&I32Xor).instruction(&I32And); },
                Bitwise::Flip => { function.instruction(&I32Xor); },
                Bitwise::Set => { function.instruction(&I32Or); },
                _ => unreachable!(),
            }
        },
        _ => {
            function.instruction(&LocalGet(temporary + 1));
            let instruction = match operation {
                Bitwise::And => I32And, Bitwise::Or => I32Or, Bitwise::Xor => I32Xor,
                Bitwise::AndNot => {
                    function.instruction(&I32Const(-1)).instruction(&I32Xor);
                    I32And
                },
                Bitwise::Left => I32Shl, Bitwise::Right => I32ShrS,
                Bitwise::Unsigned => I32ShrU, Bitwise::Imul => I32Mul,
                _ => unreachable!("only integer primitive chains"),
            };
            function.instruction(&instruction);
        },
    }
    if let Some(slot) = plan.slot(result) {
        function.instruction(&LocalSet(raw_base + slot));
    } else {
        function.instruction(&if operation == Bitwise::Unsigned { F64ConvertI32U } else { F64ConvertI32S })
            .instruction(&Call(index("number-box")))
            .instruction(&LocalSet(result.0 as u32 + offset));
    }
}
