//! Backward SSA liveness for continuation spill decisions, without suspension lowering.
use super::{Function, Terminator, ValueId, operands};
use crate::portable::Diagnostic;
use std::collections::BTreeSet;

/// Sorted, duplicate-free value IDs in the enclosing function's ID space.
pub type LiveSet = BTreeSet<usize>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockLiveness {
    /// Immediately after binding this block's parameters, before its instructions.
    /// Live parameters are expressed using the destination block's IDs.
    pub live_in: LiveSet,
    /// Immediately before the terminator, including its own operands.
    /// Successor parameters have been substituted with predecessor arguments.
    pub live_out: LiveSet,
    /// One set per instruction, immediately after that instruction executes.
    /// This includes its result if subsequently used. At a suspending operation,
    /// the result is produced on resumption; it is not a pre-suspension spill.
    pub live_after: Vec<LiveSet>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Liveness {
    /// Indexed by the original block ID; IDs are never renumbered.
    pub blocks: Vec<BlockLiveness>,
}

/// Analyze an IR function, validating it first, including nested closure graphs.
/// Entry parameters (closure captures/arguments) are accepted with their declared
/// types. Closure creation uses captures only; nested bodies have independent IDs
/// and must be analyzed separately when lowering their own continuations.
pub fn analyze(function: &Function) -> Result<Liveness, Diagnostic> {
    let entry_types = function
        .blocks
        .first()
        .map(|block| {
            block
                .parameters
                .iter()
                .map(|id| {
                    function
                        .values
                        .get(id.0)
                        .map(|value| value.ty)
                        .ok_or_else(|| Diagnostic {
                            span: function.span.clone(),
                            message: "IR entry parameter is out of range".into(),
                        })
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    super::verify_function(function, &entry_types, 0)?;
    let mut entries = vec![LiveSet::new(); function.blocks.len()];
    loop {
        let mut changed = false;
        for block_id in (0..function.blocks.len()).rev() {
            let mut live = terminator_live(function, block_id, &entries);
            for instruction in function.blocks[block_id].instructions.iter().rev() {
                live.remove(&instruction.result.0);
                live.extend(operands(&instruction.operation).iter().map(|id| id.0));
            }
            if live != entries[block_id] {
                entries[block_id] = live;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let blocks = function
        .blocks
        .iter()
        .enumerate()
        .map(|(block_id, block)| {
            let live_out = terminator_live(function, block_id, &entries);
            let mut live = live_out.clone();
            let mut live_after = vec![LiveSet::new(); block.instructions.len()];
            for (index, instruction) in block.instructions.iter().enumerate().rev() {
                live_after[index] = live.clone();
                live.remove(&instruction.result.0);
                live.extend(operands(&instruction.operation).iter().map(|id| id.0));
            }
            BlockLiveness {
                live_in: entries[block_id].clone(),
                live_out,
                live_after,
            }
        })
        .collect();
    Ok(Liveness { blocks })
}

fn terminator_live(function: &Function, block_id: usize, entries: &[LiveSet]) -> LiveSet {
    match &function.blocks[block_id].terminator {
        Terminator::Return(value) | Terminator::Throw(value) => LiveSet::from([value.0]),
        Terminator::Branch {
            condition,
            consequent,
            alternative,
        } => {
            let mut live = entries[*consequent].clone();
            live.extend(&entries[*alternative]);
            live.insert(condition.0);
            live
        }
        Terminator::Jump { target, arguments } => {
            // Simultaneous substitution: remove every destination parameter
            // before adding any incoming value, including on self/back edges.
            let parameters = &function.blocks[*target].parameters;
            let mut live = entries[*target].clone();
            for parameter in parameters {
                live.remove(&parameter.0);
            }
            // Every edge operand is read by the current IR's parameter transport,
            // even if its destination parameter is unused. Dead-edge pruning is
            // a separate transformation; spill analysis must not assume it ran.
            live.extend(arguments.iter().map(|ValueId(id)| *id));
            live
        }
    }
}
