//! Remove closures whose complete use set consists of known, fixed-arity calls.
//! Run after source overload/return/recur checks. SSA substitution changes storage,
//! never the source expression classes which selected those operations (#351).
use super::*;

fn charge(meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>, work: usize, span: &Range<usize>) -> Result<(), Diagnostic> {
    lower_charge(meter, work, span)
}

/// Count work before copying a graph, including owned literal/closure data.
/// Futures carry a separate continuation plan and are never copied by this pass.
fn size(function: &Function, limit: usize, copying: bool,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>) -> Result<Option<usize>, Diagnostic>
{
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
        let mut total = 1usize.saturating_add(function.values.len()).saturating_add(function.blocks.len());
        charge(meter, total, &function.span)?;
        if total > limit { return Ok(None); }
        for block in &function.blocks {
            let work = block.parameters.len().saturating_add(match &block.terminator {
                Terminator::Jump { arguments, .. } => arguments.len(), _ => 1,
            });
            charge(meter, work, &function.span)?;
            total = total.saturating_add(work);
            if total > limit { return Ok(None); }
            for inst in &block.instructions {
                let work = 1usize.saturating_add(operands(&inst.operation).len()).saturating_add(match &inst.operation {
                    Operation::Literal(Literal::String(units)) => units.len(), _ => 0,
                });
                charge(meter, work, &inst.span)?;
                total = total.saturating_add(work);
                if total > limit { return Ok(None); }
                let children: Vec<&ClosureBody> = match &inst.operation {
                    Operation::MakeClosure { body, .. } => vec![body],
                    Operation::MakeGeneralClosure { body, .. } => body.methods.iter().collect(),
                    Operation::MakeFuture { .. } if copying => return Ok(None),
                    _ => Vec::new(),
                };
                for child in children {
                    let work = child.capture_types.len()
                        .saturating_add(child.display_name.as_ref().map_or(0, Vec::len))
                        .saturating_add(child.arity_name.as_ref().map_or(0, Vec::len))
                        .saturating_add(child.primitive.as_ref().map_or(0, |s| s.parameters.len()));
                    charge(meter, work, &inst.span)?;
                    total = total.saturating_add(work);
                    if total > limit { return Ok(None); }
                    let Some(work) = size(&child.function, limit - total, copying, meter)? else { return Ok(None); };
                    total += work;
                }
            }
        }
        Ok(Some(total))
    })
}

pub(super) fn eliminate(function: &mut Function, meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>) -> Result<(), Diagnostic> {
    // Copying is bounded by the original graph size. Single-use bodies move;
    // repeated calls may copy only within this shared linear growth allowance.
    let mut growth = size(function, usize::MAX, false, meter)?.unwrap_or(0);
    walk(function, &mut growth, meter)
}

fn synchronous(function: &Function) -> bool {
    !function.blocks.iter().flat_map(|block| &block.instructions).any(|inst|
        matches!(inst.operation, Operation::Await { .. } | Operation::RegionPush { .. }
            | Operation::RegionExit { .. } | Operation::RegionValue | Operation::AsyncDynamicEnter { .. }))
}

struct Candidate {
    definition: (usize, usize),
    arity: usize,
    escaped: bool,
    sites: Vec<(usize, usize)>,
    function: Option<Function>,
    captures: Vec<ValueId>,
}

fn walk(function: &mut Function, growth: &mut usize,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>) -> Result<(), Diagnostic>
{
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
        if !synchronous(function) { return Ok(()); }
        for block in &mut function.blocks {
            for inst in &mut block.instructions {
                match &mut inst.operation {
                    Operation::MakeClosure { body, .. } => walk(&mut body.function, growth, meter)?,
                    Operation::MakeGeneralClosure { body, .. } => for method in &mut body.methods { walk(&mut method.function, growth, meter)?; },
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
                    if body.variadic || !synchronous(&body.function)
                        || body.function.blocks.iter().any(|block| match block.terminator {
                            Terminator::Jump { target: 0, .. } => true,
                            Terminator::Branch { consequent, alternative, .. } => consequent == 0 || alternative == 0,
                            _ => false,
                        }) { continue; }
                    by_value.insert(inst.result, candidates.len());
                    candidates.push(Candidate { definition: (b, i), arity: body.arity,
                        escaped: false, sites: Vec::new(), function: None, captures: Vec::new() });
                }
            }
        }
        if candidates.is_empty() { return Ok(()); }
        for (b, block) in function.blocks.iter().enumerate() {
            for (i, inst) in block.instructions.iter().enumerate() {
                let inputs = operands(&inst.operation);
                charge(meter, 1 + inputs.len(), &inst.span)?;
                for (position, value) in inputs.iter().enumerate() {
                    let Some(&candidate) = by_value.get(value) else { continue; };
                    let candidate = &mut candidates[candidate];
                    if position == 0 && matches!(inst.operation, Operation::Call { .. })
                        && inputs.len() == candidate.arity + 1
                    { candidate.sites.push((b, i)); }
                    else { candidate.escaped = true; }
                }
            }
            let inputs = match &block.terminator {
                Terminator::Return(value) | Terminator::Throw(value) => std::slice::from_ref(value),
                Terminator::Jump { arguments, .. } => arguments,
                Terminator::Branch { condition, .. } => std::slice::from_ref(condition),
            };
            charge(meter, inputs.len(), &function.span)?;
            for value in inputs {
                if let Some(&candidate) = by_value.get(value) { candidates[candidate].escaped = true; }
            }
        }
        let mut sites = Vec::new();
        for (index, candidate) in candidates.iter_mut().enumerate() {
            if candidate.escaped { continue; }
            let (b, i) = candidate.definition;
            let Operation::MakeClosure { body, .. } = &function.blocks[b].instructions[i].operation else { unreachable!() };
            let overhead = candidate.sites.len().saturating_mul(8);
            if overhead > *growth { continue; }
            let copies = candidate.sites.len().saturating_sub(1);
            let copying = if copies == 0 { 0 } else {
                let Some(work) = size(&body.function, (*growth - overhead) / copies, true, meter)? else { continue; };
                work * copies
            };
            *growth -= copying + overhead;
            let inst = &mut function.blocks[b].instructions[i];
            let Operation::MakeClosure { body, captures } = std::mem::replace(&mut inst.operation, Operation::Literal(Literal::Nil)) else { unreachable!() };
            function.values[inst.result.0].ty = Type::Value;
            let ClosureBody { function: body, .. } = *body;
            candidate.function = Some(body);
            candidate.captures = captures;
            sites.extend(candidate.sites.iter().map(|&(b, i)| (b, i, index)));
        }
        // Splitting a block leaves earlier instruction positions unchanged.
        sites.sort_unstable_by(|a, b| b.cmp(a));
        for (b, i, candidate) in sites {
            let candidate = &mut candidates[candidate];
            candidate.sites.pop();
            let source = if candidate.sites.is_empty() { candidate.function.take().unwrap() }
                else { candidate.function.as_ref().unwrap().clone() };
            splice(function, b, i, source, &candidate.captures, meter)?;
        }
        Ok(())
    })
}

fn map_operation(operation: &mut Operation, ids: &[ValueId]) {
    let inputs: &mut [ValueId] = match operation {
        Operation::Bitwise { arguments, .. } | Operation::Comparison { arguments, .. }
        | Operation::Array { arguments, .. } | Operation::Nominal { arguments, .. } => arguments,
        Operation::Call { operands, .. } | Operation::DynamicScope { operands, .. }
        | Operation::AsyncDynamicEnter { operands, .. } => operands,
        Operation::MakeClosure { captures, .. } | Operation::MakeGeneralClosure { captures, .. }
        | Operation::MakeFuture { captures, .. } => captures,
        Operation::Try { regions } => regions,
        Operation::Await { future } => std::slice::from_mut(future),
        Operation::GlobalWrite { value, .. } | Operation::NilTest(value)
        | Operation::RegionExit { value, .. } => std::slice::from_mut(value),
        Operation::RegionPush { dynamic, .. } => dynamic,
        Operation::Literal(_) | Operation::GlobalRead(_) | Operation::GlobalCell(_)
        | Operation::GlobalBound(_) | Operation::RegionValue => return,
    };
    for value in inputs { *value = ids[value.0]; }
}

fn splice(parent: &mut Function, block: usize, index: usize, mut body: Function,
    captures: &[ValueId], meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>) -> Result<(), Diagnostic>
{
    charge(meter, body.values.len().saturating_add(body.blocks.len()).saturating_add(captures.len()), &body.span)?;
    let suffix = parent.blocks[block].instructions.split_off(index + 1);
    let call = parent.blocks[block].instructions.pop().unwrap();
    let Operation::Call { operands: arguments, .. } = call.operation else { unreachable!() };
    let mut actuals = captures.to_vec();
    actuals.extend_from_slice(&arguments[1..]);
    assert_eq!(actuals.len(), body.blocks[0].parameters.len());
    let mut ids = vec![ValueId(usize::MAX); body.values.len()];
    for (parameter, actual) in body.blocks[0].parameters.iter().zip(actuals) { ids[parameter.0] = actual; }
    for (index, value) in std::mem::take(&mut body.values).into_iter().enumerate() {
        if ids[index].0 != usize::MAX { continue; }
        ids[index] = ValueId(parent.values.len());
        parent.values.push(value);
    }
    let start = parent.blocks.len();
    let join = start + body.blocks.len();
    let terminator = std::mem::replace(&mut parent.blocks[block].terminator, Terminator::Jump { target: start, arguments: Vec::new() });
    for (index, mut inner) in std::mem::take(&mut body.blocks).into_iter().enumerate() {
        charge(meter, inner.instructions.len() + inner.parameters.len(), &body.span)?;
        if index == 0 { inner.parameters.clear(); }
        else { for value in &mut inner.parameters { *value = ids[value.0]; } }
        for inst in &mut inner.instructions {
            charge(meter, operands(&inst.operation).len(), &inst.span)?;
            inst.result = ids[inst.result.0];
            map_operation(&mut inst.operation, &ids);
        }
        inner.terminator = match inner.terminator {
            Terminator::Return(value) => Terminator::Jump { target: join, arguments: vec![ids[value.0]] },
            Terminator::Throw(value) => Terminator::Throw(ids[value.0]),
            Terminator::Jump { target, arguments } => Terminator::Jump { target: start + target, arguments: arguments.into_iter().map(|value| ids[value.0]).collect() },
            Terminator::Branch { condition, consequent, alternative } => Terminator::Branch { condition: ids[condition.0], consequent: start + consequent, alternative: start + alternative },
        };
        parent.blocks.push(inner);
    }
    parent.blocks.push(Block { parameters: vec![call.result], instructions: suffix, terminator });
    Ok(())
}
