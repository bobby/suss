//! Backward SSA liveness for continuation spill decisions, without suspension lowering.
use super::{Function, Terminator, ValueId, operands};
use crate::portable::Diagnostic;
use std::collections::BTreeSet;

/// Sorted, duplicate-free value IDs in the enclosing function's ID space.
pub type LiveSet = BTreeSet<usize>;

pub(crate) type Meter<'a> = dyn FnMut(u64) -> Result<(), Diagnostic> + 'a;

fn charge(
    meter: &mut Meter<'_>,
    amount: usize,
    span: &std::ops::Range<usize>,
) -> Result<(), Diagnostic> {
    if amount == 0 {
        return Ok(());
    }
    meter(u64::try_from(amount).unwrap_or(u64::MAX)).map_err(|mut error| {
        error.span = span.clone();
        error
    })
}

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
    let span = function.span.clone();
    let mut remaining = crate::portable::DEFAULT_OPERATION_FUEL;
    let mut meter = |work: u64| {
        remaining = remaining.checked_sub(work).ok_or_else(|| Diagnostic {
            span: span.clone(),
            message: "IR liveness fuel exhausted".into(),
        })?;
        Ok(())
    };
    analyze_metered(function, &mut meter)
}

/// Validate and analyze a function using one caller-owned computation budget.
pub(crate) fn analyze_metered(
    function: &Function,
    meter: &mut Meter<'_>,
) -> Result<Liveness, Diagnostic> {
    let entry_types = function
        .blocks
        .first()
        .map(|block| {
            charge(meter, block.parameters.len(), &function.span)?;
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
    super::verify_function(function, &entry_types, meter)?;
    analyze_verified_metered(function, meter)
}

/// Compute liveness after the exact function graph has already passed IR
/// verification. The caller supplies the same meter used for that verification.
pub(crate) fn analyze_verified_metered(
    function: &Function,
    meter: &mut Meter<'_>,
) -> Result<Liveness, Diagnostic> {
    charge(meter, function.blocks.len(), &function.span)?;
    let mut entries = vec![LiveSet::new(); function.blocks.len()];
    loop {
        let mut changed = false;
        for block_id in (0..function.blocks.len()).rev() {
            charge(meter, 1, &function.span)?;
            let mut live = terminator_live(function, block_id, &entries, meter)?;
            for instruction in function.blocks[block_id].instructions.iter().rev() {
                charge(
                    meter,
                    1usize.saturating_add(operands(&instruction.operation).len()),
                    &instruction.span,
                )?;
                live.remove(&instruction.result.0);
                live.extend(operands(&instruction.operation).iter().map(|id| id.0));
            }
            charge(
                meter,
                live.len().max(entries[block_id].len()),
                &function.span,
            )?;
            if live != entries[block_id] {
                entries[block_id] = live;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    charge(meter, function.blocks.len(), &function.span)?;
    let blocks = function
        .blocks
        .iter()
        .enumerate()
        .map(|(block_id, block)| -> Result<BlockLiveness, Diagnostic> {
            charge(meter, 1, &function.span)?;
            let live_out = terminator_live(function, block_id, &entries, meter)?;
            charge(meter, live_out.len(), &function.span)?;
            let mut live = live_out.clone();
            charge(meter, block.instructions.len(), &function.span)?;
            let mut live_after = vec![LiveSet::new(); block.instructions.len()];
            for (index, instruction) in block.instructions.iter().enumerate().rev() {
                charge(meter, live.len(), &instruction.span)?;
                live_after[index] = live.clone();
                charge(
                    meter,
                    1usize.saturating_add(operands(&instruction.operation).len()),
                    &instruction.span,
                )?;
                live.remove(&instruction.result.0);
                live.extend(operands(&instruction.operation).iter().map(|id| id.0));
            }
            charge(meter, entries[block_id].len(), &function.span)?;
            Ok(BlockLiveness {
                live_in: entries[block_id].clone(),
                live_out,
                live_after,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Liveness { blocks })
}

fn terminator_live(
    function: &Function,
    block_id: usize,
    entries: &[LiveSet],
    meter: &mut Meter<'_>,
) -> Result<LiveSet, Diagnostic> {
    let span = &function.span;
    Ok(match &function.blocks[block_id].terminator {
        Terminator::Return(value) | Terminator::Throw(value) => {
            charge(meter, 1, span)?;
            LiveSet::from([value.0])
        }
        Terminator::Branch {
            condition,
            consequent,
            alternative,
        } => {
            charge(
                meter,
                entries[*consequent]
                    .len()
                    .saturating_add(entries[*alternative].len())
                    .saturating_add(2),
                span,
            )?;
            let mut live = entries[*consequent].clone();
            live.extend(&entries[*alternative]);
            live.insert(condition.0);
            live
        }
        Terminator::Jump { target, arguments } => {
            // Simultaneous substitution: remove every destination parameter
            // before adding any incoming value, including on self/back edges.
            let parameters = &function.blocks[*target].parameters;
            charge(
                meter,
                entries[*target]
                    .len()
                    .saturating_add(parameters.len())
                    .saturating_add(arguments.len()),
                span,
            )?;
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
    })
}
