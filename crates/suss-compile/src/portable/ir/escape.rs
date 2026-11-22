//! Remove closures whose complete use set consists of known, fixed-arity calls.
//! Run after source overload/return/recur checks. SSA substitution changes storage,
//! never the source expression classes which selected those operations (#351).
use super::*;

fn charge(
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
    work: usize,
    span: &Range<usize>,
) -> Result<(), Diagnostic> {
    lower_charge(meter, work, span)
}

/// Count work before copying a graph, including owned literal/closure data.
/// Futures carry a separate continuation plan and are never copied by this pass.
pub(super) fn size(
    function: &Function,
    limit: usize,
    copying: bool,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<Option<usize>, Diagnostic> {
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
        let mut total = 1usize
            .saturating_add(function.values.len())
            .saturating_add(function.blocks.len());
        charge(meter, total, &function.span)?;
        if total > limit {
            return Ok(None);
        }
        for block in &function.blocks {
            let work = block
                .parameters
                .len()
                .saturating_add(match &block.terminator {
                    Terminator::Jump { arguments, .. } => arguments.len(),
                    _ => 1,
                });
            charge(meter, work, &function.span)?;
            total = total.saturating_add(work);
            if total > limit {
                return Ok(None);
            }
            for inst in &block.instructions {
                let work = 1usize
                    .saturating_add(operands(&inst.operation).len())
                    .saturating_add(match &inst.operation {
                        Operation::Literal(Literal::String(units)) => units.len(),
                        Operation::Call {
                            primitive: Some(shape),
                            ..
                        } => shape
                            .0
                            .iter()
                            .map(|method| 1 + method.signature.parameters.len())
                            .sum(),
                        Operation::MakeGeneralClosure { body, .. } => body
                            .capture_types
                            .len()
                            .saturating_add(body.display_name.as_ref().map_or(0, Vec::len))
                            .saturating_add(body.arity_name.as_ref().map_or(0, Vec::len)),
                        _ => 0,
                    });
                charge(meter, work, &inst.span)?;
                total = total.saturating_add(work);
                if total > limit {
                    return Ok(None);
                }
                let mut child_size = |child: &ClosureBody| -> Result<bool, Diagnostic> {
                    let work = child
                        .capture_types
                        .len()
                        .saturating_add(child.display_name.as_ref().map_or(0, Vec::len))
                        .saturating_add(child.arity_name.as_ref().map_or(0, Vec::len))
                        .saturating_add(child.primitive.as_ref().map_or(0, |s| s.parameters.len()));
                    charge(meter, work, &inst.span)?;
                    total = total.saturating_add(work);
                    if total > limit {
                        return Ok(false);
                    }
                    let Some(work) = size(&child.function, limit - total, copying, meter)? else {
                        return Ok(false);
                    };
                    total += work;
                    Ok(true)
                };
                match &inst.operation {
                    Operation::MakeClosure { body, .. } => {
                        if !child_size(body)? {
                            return Ok(None);
                        }
                    }
                    Operation::MakeGeneralClosure { body, .. } => {
                        for method in &body.methods {
                            if !child_size(method)? {
                                return Ok(None);
                            }
                        }
                    }
                    Operation::MakeFuture { .. } if copying => return Ok(None),
                    _ => {}
                }
            }
        }
        Ok(Some(total))
    })
}

pub(super) fn eliminate(
    function: &mut Function,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    // Copying is bounded by the original graph size. Single-use bodies move;
    // repeated calls may copy only within this shared linear growth allowance.
    let mut growth = size(function, usize::MAX, false, meter)?.unwrap_or(0);
    walk(function, &mut growth, meter)
}

fn synchronous(function: &Function) -> bool {
    !function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .any(|inst| {
            matches!(
                inst.operation,
                Operation::Await { .. }
                    | Operation::RegionPush { .. }
                    | Operation::RegionExit { .. }
                    | Operation::RegionValue
                    | Operation::AsyncDynamicEnter { .. }
            )
        })
}

struct Candidate {
    definition: (usize, usize),
    arity: usize,
    escaped: bool,
    sites: Vec<(usize, usize)>,
    function: Option<Function>,
    captures: Vec<ValueId>,
}

fn walk(
    function: &mut Function,
    growth: &mut usize,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
        if !synchronous(function) {
            return Ok(());
        }
        for block in &mut function.blocks {
            for inst in &mut block.instructions {
                match &mut inst.operation {
                    Operation::MakeClosure { body, .. } => walk(&mut body.function, growth, meter)?,
                    Operation::MakeGeneralClosure { body, .. } => {
                        for method in &mut body.methods {
                            walk(&mut method.function, growth, meter)?;
                        }
                    }
                    // Do not change normalized continuation graphs or their spill plans.
                    _ => {}
                }
            }
        }
        let mut candidates = Vec::new();
        let mut by_value = HashMap::new();
        for (b, block) in function.blocks.iter().enumerate() {
            charge(meter, 1 + block.instructions.len(), &function.span)?;
            for (i, inst) in block.instructions.iter().enumerate() {
                if let Operation::MakeClosure { body, .. } = &inst.operation {
                    if body.variadic || !synchronous(&body.function) {
                        continue;
                    }
                    by_value.insert(inst.result, candidates.len());
                    candidates.push(Candidate {
                        definition: (b, i),
                        arity: body.arity,
                        escaped: false,
                        sites: Vec::new(),
                        function: None,
                        captures: Vec::new(),
                    });
                }
            }
        }
        if candidates.is_empty() {
            return Ok(());
        }
        for (b, block) in function.blocks.iter().enumerate() {
            for (i, inst) in block.instructions.iter().enumerate() {
                let inputs = operands(&inst.operation);
                charge(meter, 1 + inputs.len(), &inst.span)?;
                for (position, value) in inputs.iter().enumerate() {
                    let Some(&candidate) = by_value.get(value) else {
                        continue;
                    };
                    let candidate = &mut candidates[candidate];
                    if position == 0
                        && matches!(inst.operation, Operation::Call { .. })
                        && inputs.len() == candidate.arity + 1
                    {
                        candidate.sites.push((b, i));
                    } else {
                        candidate.escaped = true;
                    }
                }
            }
            let inputs = match &block.terminator {
                Terminator::Return(value) | Terminator::Throw(value) => std::slice::from_ref(value),
                Terminator::Jump { arguments, .. } => arguments,
                Terminator::Branch { condition, .. } => std::slice::from_ref(condition),
            };
            charge(meter, inputs.len(), &function.span)?;
            for value in inputs {
                if let Some(&candidate) = by_value.get(value) {
                    candidates[candidate].escaped = true;
                }
            }
        }
        let mut sites = Vec::new();
        for (index, candidate) in candidates.iter_mut().enumerate() {
            if candidate.escaped {
                continue;
            }
            let (b, i) = candidate.definition;
            let Operation::MakeClosure { body, .. } = &function.blocks[b].instructions[i].operation
            else {
                unreachable!()
            };
            let overhead = candidate.sites.len().saturating_mul(8);
            if overhead > *growth {
                continue;
            }
            let copies = candidate.sites.len().saturating_sub(1);
            let copying = if copies == 0 {
                0
            } else {
                let Some(work) = size(&body.function, (*growth - overhead) / copies, true, meter)?
                else {
                    continue;
                };
                work * copies
            };
            *growth -= copying + overhead;
            let inst = &mut function.blocks[b].instructions[i];
            let Operation::MakeClosure { body, captures } =
                std::mem::replace(&mut inst.operation, Operation::Literal(Literal::Nil))
            else {
                unreachable!()
            };
            function.values[inst.result.0].ty = Type::Nil;
            let ClosureBody { function: body, .. } = *body;
            candidate.function = Some(body);
            candidate.captures = captures;
            sites.extend(candidate.sites.iter().map(|&(b, i)| (b, i, index)));
        }
        // Splitting a block leaves earlier instruction positions unchanged.
        sites.sort_unstable_by(|a, b| b.cmp(a));
        let changed = !sites.is_empty();
        for (b, i, candidate) in sites {
            let candidate = &mut candidates[candidate];
            candidate.sites.pop();
            let source = if candidate.sites.is_empty() {
                candidate.function.take().unwrap()
            } else {
                candidate.function.as_ref().unwrap().clone()
            };
            splice(function, b, i, source, &candidate.captures, meter)?;
        }
        if changed {
            prune_unreachable(function, meter)?;
        }
        Ok(())
    })
}

pub(super) fn map_operation(operation: &mut Operation, ids: &[ValueId]) {
    let inputs: &mut [ValueId] = match operation {
        Operation::Arithmetic { arguments, .. }
        | Operation::Bitwise { arguments, .. }
        | Operation::Comparison { arguments, .. }
        | Operation::Array { arguments, .. }
        | Operation::Nominal { arguments, .. } => arguments,
        Operation::Call { operands, .. }
        | Operation::DynamicScope { operands, .. }
        | Operation::AsyncDynamicEnter { operands, .. } => operands,
        Operation::MakeClosure { captures, .. }
        | Operation::MakeGeneralClosure { captures, .. }
        | Operation::MakeFuture { captures, .. } => captures,
        Operation::Try { regions } => regions,
        Operation::Await { future } => std::slice::from_mut(future),
        Operation::GlobalWrite { value, .. }
        | Operation::NilTest(value)
        | Operation::RegionExit { value, .. } => std::slice::from_mut(value),
        Operation::RegionPush { dynamic, .. } => dynamic,
        Operation::Literal(_)
        | Operation::GlobalRead(_)
        | Operation::GlobalCell(_)
        | Operation::GlobalBound(_)
        | Operation::RegionValue => return,
    };
    for value in inputs {
        *value = ids[value.0];
    }
}

fn splice(
    parent: &mut Function,
    block: usize,
    index: usize,
    mut body: Function,
    captures: &[ValueId],
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    charge(
        meter,
        body.values
            .len()
            .saturating_add(body.blocks.len())
            .saturating_add(captures.len()),
        &body.span,
    )?;
    charge(
        meter,
        parent.blocks[block].instructions.len() - index - 1,
        &body.span,
    )?;
    let suffix = parent.blocks[block].instructions.split_off(index + 1);
    let call = parent.blocks[block].instructions.pop().unwrap();
    let Operation::Call {
        operands: arguments,
        ..
    } = call.operation
    else {
        unreachable!()
    };
    let mut actuals = captures.to_vec();
    actuals.extend_from_slice(&arguments[1..]);
    assert_eq!(actuals.len(), body.blocks[0].parameters.len());
    // Keep declared entry types, including Value parameters captured by nested
    // thunks. Their physical representations propagate through these CFG edges.
    let mut ids = Vec::with_capacity(body.values.len());
    for value in std::mem::take(&mut body.values) {
        ids.push(ValueId(parent.values.len()));
        parent.values.push(value);
    }
    let start = parent.blocks.len();
    let join = start + body.blocks.len();
    let terminator = std::mem::replace(
        &mut parent.blocks[block].terminator,
        Terminator::Jump {
            target: start,
            arguments: actuals,
        },
    );
    for mut inner in std::mem::take(&mut body.blocks) {
        charge(
            meter,
            inner.instructions.len() + inner.parameters.len(),
            &body.span,
        )?;
        for value in &mut inner.parameters {
            *value = ids[value.0];
        }
        for inst in &mut inner.instructions {
            charge(meter, operands(&inst.operation).len(), &inst.span)?;
            inst.result = ids[inst.result.0];
            map_operation(&mut inst.operation, &ids);
        }
        inner.terminator = match inner.terminator {
            Terminator::Return(value) => Terminator::Jump {
                target: join,
                arguments: vec![ids[value.0]],
            },
            Terminator::Throw(value) => Terminator::Throw(ids[value.0]),
            Terminator::Jump { target, arguments } => Terminator::Jump {
                target: start + target,
                arguments: arguments.into_iter().map(|value| ids[value.0]).collect(),
            },
            Terminator::Branch {
                condition,
                consequent,
                alternative,
            } => Terminator::Branch {
                condition: ids[condition.0],
                consequent: start + consequent,
                alternative: start + alternative,
            },
        };
        parent.blocks.push(inner);
    }
    parent.blocks.push(Block {
        parameters: vec![call.result],
        instructions: suffix,
        terminator,
    });
    Ok(())
}

/// A body that only throws or recurs has no return edge to the call's suffix.
/// Remove that suffix (and any blocks it alone reached) after all sites split.
pub(super) fn prune_unreachable(
    function: &mut Function,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    charge(
        meter,
        function.blocks.len().saturating_mul(3),
        &function.span,
    )?;
    let mut reachable = vec![false; function.blocks.len()];
    let mut pending = vec![0];
    while let Some(block) = pending.pop() {
        if reachable[block] {
            continue;
        }
        reachable[block] = true;
        match function.blocks[block].terminator {
            Terminator::Jump { target, .. } => pending.push(target),
            Terminator::Branch {
                consequent,
                alternative,
                ..
            } => pending.extend([consequent, alternative]),
            _ => {}
        }
    }
    if reachable.iter().all(|value| *value) {
        return Ok(());
    }
    let mut ids = vec![0; reachable.len()];
    let mut next = 0;
    for (index, live) in reachable.iter().enumerate() {
        if *live {
            ids[index] = next;
            next += 1;
        }
    }
    let mut index = 0;
    function.blocks.retain(|_| {
        let live = reachable[index];
        index += 1;
        live
    });
    for block in &mut function.blocks {
        match &mut block.terminator {
            Terminator::Jump { target, .. } => *target = ids[*target],
            Terminator::Branch {
                consequent,
                alternative,
                ..
            } => {
                *consequent = ids[*consequent];
                *alternative = ids[*alternative];
            }
            _ => {}
        }
    }
    // Verification requires a definition for every SSA record, including
    // records no longer used after an always-throwing call removes its suffix.
    charge(
        meter,
        function.values.len().saturating_mul(3),
        &function.span,
    )?;
    let mut defined = vec![false; function.values.len()];
    for block in &function.blocks {
        for value in &block.parameters {
            defined[value.0] = true;
        }
        for inst in &block.instructions {
            defined[inst.result.0] = true;
        }
    }
    let mut values = vec![ValueId(usize::MAX); defined.len()];
    let mut next = 0;
    for (index, live) in defined.iter().enumerate() {
        if *live {
            values[index] = ValueId(next);
            next += 1;
        }
    }
    let mut index = 0;
    function.values.retain(|_| {
        let live = defined[index];
        index += 1;
        live
    });
    for block in &mut function.blocks {
        charge(
            meter,
            block.parameters.len() + block.instructions.len(),
            &function.span,
        )?;
        for value in &mut block.parameters {
            *value = values[value.0];
        }
        for inst in &mut block.instructions {
            charge(meter, operands(&inst.operation).len(), &inst.span)?;
            inst.result = values[inst.result.0];
            map_operation(&mut inst.operation, &values);
        }
        let inputs = match &mut block.terminator {
            Terminator::Return(value) | Terminator::Throw(value) => std::slice::from_mut(value),
            Terminator::Jump { arguments, .. } => arguments,
            Terminator::Branch { condition, .. } => std::slice::from_mut(condition),
        };
        charge(meter, inputs.len(), &function.span)?;
        for value in inputs {
            *value = values[value.0];
        }
    }
    Ok(())
}
