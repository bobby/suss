//! Virtual vector/MapEntry storage after source typing (#351).
//! A guarded continuation keeps scalar elements on the fast path; the original
//! construction and lookup paths remain available when a precondition misses.
use super::*;

#[derive(Clone, Copy)]
enum Kind {
    Vector { array: ValueId },
    MapEntry,
}
impl Kind {
    fn array(self) -> Option<ValueId> {
        match self {
            Self::Vector { array } => Some(array),
            Self::MapEntry => None,
        }
    }
    fn guard(self) -> Nominal {
        match self {
            Self::Vector { .. } => Nominal::ScalarVectorGuard,
            Self::MapEntry => Nominal::ScalarMapEntryGuard,
        }
    }
}

struct Plan {
    block: usize,
    instruction: usize,
    kind: Kind,
    checked: Option<ValueId>,
    elements: Vec<ValueId>,
    reads: HashMap<ValueId, ValueId>,
    conditions: HashSet<ValueId>,
    region: Vec<usize>,
}

fn charge(
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
    n: usize,
    span: &Range<usize>,
) -> Result<(), Diagnostic> {
    lower_charge(meter, n, span)
}

pub(super) fn eliminate(
    function: &mut Function,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    let mut budget = escape::size(function, usize::MAX, false, meter)?.unwrap_or(0);
    walk(function, &mut budget, meter)
}

fn synchronous(function: &Function) -> bool {
    !function
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .any(|i| {
            matches!(
                i.operation,
                Operation::Await { .. }
                    | Operation::RegionPush { .. }
                    | Operation::RegionExit { .. }
                    | Operation::RegionValue
                    | Operation::AsyncDynamicEnter { .. }
            )
        })
}

fn walk(
    function: &mut Function,
    budget: &mut usize,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
        if !synchronous(function) {
            return Ok(());
        }
        for block in &mut function.blocks {
            for inst in &mut block.instructions {
                match &mut inst.operation {
                    Operation::MakeClosure { body, .. } => walk(&mut body.function, budget, meter)?,
                    Operation::MakeGeneralClosure { body, .. } => {
                        for method in &mut body.methods {
                            walk(&mut method.function, budget, meter)?;
                        }
                    }
                    _ => {}
                }
            }
        }
        let Some(plan) = analyze(function, meter)? else {
            return Ok(());
        };
        // The complete parent graph is a conservative upper bound for this
        // continuation copy, including nested owned bodies and literal data.
        let Some(work) = escape::size(function, *budget, true, meter)? else {
            return Ok(());
        };
        *budget -= work;
        transform(function, plan, meter)?;
        escape::prune_unreachable(function, meter)
    })
}

fn analyze(
    function: &Function,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<Option<Plan>, Diagnostic> {
    charge(
        meter,
        function.values.len().saturating_mul(2),
        &function.span,
    )?;
    let mut definitions = vec![None; function.values.len()];
    for (b, block) in function.blocks.iter().enumerate() {
        for (i, inst) in block.instructions.iter().enumerate() {
            definitions[inst.result.0] = Some((b, i));
        }
    }
    let operation =
        |v: ValueId| definitions[v.0].map(|(b, i)| &function.blocks[b].instructions[i].operation);
    let literal = |v: ValueId, n: i64| matches!(operation(v), Some(Operation::Literal(Literal::Long(value))) if *value == n);
    let nil = |v: ValueId| matches!(operation(v), Some(Operation::Literal(Literal::Nil)));
    for (b, block) in function.blocks.iter().enumerate() {
        for (i, inst) in block.instructions.iter().enumerate() {
            charge(meter, 1, &inst.span)?;
            let Operation::Nominal {
                operation: Nominal::Construct,
                arguments,
            } = &inst.operation
            else {
                continue;
            };
            let Some(ctor) = arguments.first() else {
                continue;
            };
            let Some(Operation::GlobalRead(global)) = operation(*ctor) else {
                continue;
            };
            let global = global.core_var_of_link().unwrap_or_else(|| global.clone());
            if global.namespace() != "suss.core" {
                continue;
            }
            let (kind, elements) = match global.name() {
                "PersistentVector" => {
                    if arguments.len() != 7
                        || !nil(arguments[1])
                        || !nil(arguments[6])
                        || !literal(arguments[3], 5)
                    {
                        continue;
                    }
                    let array = arguments[5];
                    let Some(Operation::Array {
                        operation: ArrayOperation::Objects,
                        arguments: elements,
                    }) = operation(array)
                    else {
                        continue;
                    };
                    if elements.is_empty()
                        || elements.len() >= 32
                        || !literal(arguments[2], elements.len() as i64)
                    {
                        continue;
                    }
                    (Kind::Vector { array }, elements.clone())
                }
                "MapEntry" if arguments.len() == 4 => (Kind::MapEntry, arguments[1..3].to_vec()),
                _ => continue,
            };
            let value = inst.result;
            // MapEntry's public construction guard is an identity on the
            // canonical admitted type. Private vector construction must still
            // keep its guard/error, so only the public pair can virtualize it.
            let checked = if matches!(kind, Kind::MapEntry) {
                block
                    .instructions
                    .get(i + 1)
                    .and_then(|next| match &next.operation {
                        Operation::Nominal {
                            operation: Nominal::BuiltinConstructGuard,
                            arguments,
                        } if arguments == &[value] => Some(next.result),
                        _ => None,
                    })
            } else {
                None
            };
            let virtuals = |v: ValueId| v == value || Some(v) == checked;
            let observed = checked.unwrap_or(value);
            let mut reads = HashMap::new();
            let mut array_uses = 0;
            for other in function.blocks.iter().flat_map(|b| &b.instructions) {
                charge(meter, 1 + operands(&other.operation).len(), &other.span)?;
                if let Some(array) = kind.array() {
                    array_uses += operands(&other.operation)
                        .iter()
                        .filter(|v| **v == array)
                        .count();
                }
                if let Operation::Nominal {
                    operation: Nominal::VectorNth,
                    arguments: args,
                } = &other.operation
                {
                    if args[0] != observed {
                        continue;
                    }
                    if let Some(Operation::Literal(Literal::Long(index))) = operation(args[1]) {
                        if let Ok(index) = usize::try_from(*index) {
                            if let Some(element) = elements.get(index) {
                                reads.insert(other.result, *element);
                            }
                        }
                    }
                }
            }
            for block in &function.blocks {
                charge(
                    meter,
                    1 + match &block.terminator {
                        Terminator::Jump { arguments, .. } => arguments.len(),
                        _ => 1,
                    },
                    &inst.span,
                )?;
                let Some(array) = kind.array() else {
                    continue;
                };
                array_uses += match &block.terminator {
                    Terminator::Jump { arguments, .. } => {
                        arguments.iter().filter(|v| **v == array).count()
                    }
                    Terminator::Branch { condition, .. } => usize::from(*condition == array),
                    Terminator::Return(value) | Terminator::Throw(value) => {
                        usize::from(*value == array)
                    }
                };
            }
            if (kind.array().is_some() && array_uses != 1) || reads.is_empty() {
                continue;
            }
            let mut conditions = HashSet::new();
            for other in function.blocks.iter().flat_map(|b| &b.instructions) {
                if let Operation::Nominal {
                    operation: Nominal::IsUndefined,
                    arguments: args,
                } = &other.operation
                {
                    if reads.contains_key(&args[0]) {
                        conditions.insert(other.result);
                    }
                }
            }
            let mut seen = vec![false; function.blocks.len()];
            let mut pending = vec![b];
            let mut valid = true;
            let mut boundaries = HashSet::new();
            while let Some(current) = pending.pop() {
                charge(meter, 1, &inst.span)?;
                if seen[current] {
                    continue;
                }
                seen[current] = true;
                let block = &function.blocks[current];
                let start = if current == b { i + 1 } else { 0 };
                for other in &block.instructions[start..] {
                    charge(meter, 1 + operands(&other.operation).len(), &other.span)?;
                    for input in operands(&other.operation) {
                        if virtuals(*input)
                            && !reads.contains_key(&other.result)
                            && Some(other.result) != checked
                        {
                            valid = false;
                        }
                        if reads.contains_key(input)
                            && !matches!(
                                &other.operation,
                                Operation::Nominal {
                                    operation: Nominal::IsUndefined,
                                    ..
                                }
                            )
                        {
                            valid = false;
                        }
                    }
                }
                let mut edge = |target: usize| {
                    if target <= b {
                        boundaries.insert(target);
                    } else if !seen[target] {
                        pending.push(target);
                    }
                };
                match &block.terminator {
                    Terminator::Jump { target, arguments } => {
                        if arguments.iter().any(|v| virtuals(*v)) {
                            valid = false;
                        }
                        edge(*target);
                    }
                    Terminator::Branch {
                        condition,
                        consequent,
                        alternative,
                    } => {
                        if virtuals(*condition) || reads.contains_key(condition) {
                            valid = false;
                        }
                        if !conditions.contains(condition) {
                            edge(*consequent);
                        }
                        edge(*alternative);
                    }
                    Terminator::Return(v) | Terminator::Throw(v) => {
                        if virtuals(*v) || reads.contains_key(v) {
                            valid = false;
                        }
                    }
                }
            }
            if !valid {
                continue;
            }
            // A lower-numbered continuation can follow an inlined closure.
            // Stop only at actual loop headers dominating this constructor;
            // otherwise source SSA definitions could leak into an uncloned join.
            for boundary in boundaries {
                let mut reached = vec![false; function.blocks.len()];
                let mut work = vec![0];
                while let Some(current) = work.pop() {
                    charge(meter, 1, &inst.span)?;
                    if current == boundary || reached[current] {
                        continue;
                    }
                    reached[current] = true;
                    match function.blocks[current].terminator {
                        Terminator::Jump { target, .. } => work.push(target),
                        Terminator::Branch {
                            consequent,
                            alternative,
                            ..
                        } => work.extend([consequent, alternative]),
                        _ => {}
                    }
                }
                if reached[b] {
                    valid = false;
                    break;
                }
            }
            if !valid {
                continue;
            }
            return Ok(Some(Plan {
                block: b,
                instruction: i,
                kind,
                checked,
                elements,
                reads,
                conditions,
                region: seen
                    .iter()
                    .enumerate()
                    .filter_map(|(i, v)| v.then_some(i))
                    .collect(),
            }));
        }
    }
    Ok(None)
}

fn transform(
    function: &mut Function,
    plan: Plan,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    let root = plan.block;
    let original_blocks = function.blocks.len();
    let mut block_ids = HashMap::new();
    for (offset, block) in plan.region.iter().enumerate() {
        block_ids.insert(*block, original_blocks + offset);
    }
    let mut ids: Vec<_> = (0..function.values.len()).map(ValueId).collect();
    for &block in &plan.region {
        let source = &function.blocks[block];
        let start = if block == root {
            plan.instruction + 1
        } else {
            0
        };
        let definitions = source
            .parameters
            .iter()
            .copied()
            .filter(|_| block != root)
            .chain(source.instructions[start..].iter().map(|inst| inst.result));
        for old in definitions {
            charge(meter, 1, &function.span)?;
            ids[old.0] = ValueId(function.values.len());
            function.values.push(function.values[old.0].clone());
        }
    }
    // Definitions retain their declared types. Only the uses of private
    // indexed projections substitute the already evaluated scalar elements.
    let mut uses = ids.clone();
    for (&read, &element) in &plan.reads {
        uses[read.0] = element;
    }
    for &block in &plan.region {
        let source = &function.blocks[block];
        let start = if block == root {
            plan.instruction + 1
        } else {
            0
        };
        let parameters = if block == root {
            vec![]
        } else {
            source.parameters.iter().map(|v| ids[v.0]).collect()
        };
        let mut instructions = Vec::new();
        for inst in &source.instructions[start..] {
            charge(meter, 1 + operands(&inst.operation).len(), &inst.span)?;
            let mut inst = inst.clone();
            let old = inst.result;
            inst.result = ids[old.0];
            if plan.reads.contains_key(&old) || Some(old) == plan.checked {
                inst.operation = Operation::Literal(Literal::Nil);
                function.values[inst.result.0].ty = Type::Nil;
            } else if plan.conditions.contains(&old) {
                inst.operation = Operation::Literal(Literal::Bool(false));
            } else {
                escape::map_operation(&mut inst.operation, &uses);
            }
            instructions.push(inst);
        }
        // Earlier blocks are admitted only as loop headers dominating root.
        // A back edge recomputes the original prefix and guards next iteration.
        let target = |old: usize| if old <= root { old } else { block_ids[&old] };
        let terminator = match &source.terminator {
            Terminator::Return(value) => Terminator::Return(uses[value.0]),
            Terminator::Throw(value) => Terminator::Throw(uses[value.0]),
            Terminator::Jump {
                target: old,
                arguments,
            } => Terminator::Jump {
                target: target(*old),
                arguments: arguments.iter().map(|v| uses[v.0]).collect(),
            },
            Terminator::Branch {
                condition,
                consequent,
                alternative,
            } => {
                if plan.conditions.contains(condition) {
                    Terminator::Jump {
                        target: target(*alternative),
                        arguments: vec![],
                    }
                } else {
                    Terminator::Branch {
                        condition: uses[condition.0],
                        consequent: target(*consequent),
                        alternative: target(*alternative),
                    }
                }
            }
        };
        function.blocks.push(Block {
            parameters,
            instructions,
            terminator,
        });
    }
    let mut suffix = function.blocks[root]
        .instructions
        .split_off(plan.instruction + 1);
    let mut constructor = function.blocks[root].instructions.pop().unwrap();
    let Operation::Nominal { arguments, .. } = &mut constructor.operation else {
        unreachable!()
    };
    let ctor = arguments[0];
    let span = constructor.span.clone();
    let mut instructions = Vec::new();
    if plan.kind.array().is_some() {
        let array = ValueId(function.values.len());
        function.values.push(Value {
            ty: Type::Value,
            span: span.clone(),
        });
        arguments[5] = array;
        instructions.push(Instruction {
            result: array,
            span: span.clone(),
            operation: Operation::Array {
                operation: ArrayOperation::Objects,
                arguments: plan.elements,
            },
        });
    }
    let constructor_result = constructor.result;
    instructions.push(constructor);
    instructions.append(&mut suffix);
    let slow = function.blocks.len();
    let terminator = std::mem::replace(
        &mut function.blocks[root].terminator,
        Terminator::Return(constructor_result),
    );
    function.blocks.push(Block {
        parameters: vec![],
        instructions,
        terminator,
    });
    for block in &mut function.blocks[..original_blocks] {
        for inst in &mut block.instructions {
            if Some(inst.result) == plan.kind.array() {
                inst.operation = Operation::Literal(Literal::Nil);
                function.values[inst.result.0].ty = Type::Nil;
            }
        }
    }
    charge(meter, plan.reads.len(), &function.span)?;
    let projected: HashSet<_> = plan.reads.values().copied().collect();
    // At most 31 distinct elements: sorting never scales with read-site count.
    let mut projected: Vec<_> = projected.into_iter().collect();
    projected.sort_unstable_by_key(|v| v.0);
    let mut current = root;
    for element in projected {
        let result = ValueId(function.values.len());
        function.values.push(Value {
            ty: Type::Bool,
            span: span.clone(),
        });
        function.blocks[current].instructions.push(Instruction {
            result,
            span: span.clone(),
            operation: Operation::Nominal {
                operation: Nominal::IsUndefined,
                arguments: vec![element],
            },
        });
        let next = function.blocks.len();
        function.blocks[current].terminator = Terminator::Branch {
            condition: result,
            consequent: slow,
            alternative: next,
        };
        function.blocks.push(Block {
            parameters: vec![],
            instructions: vec![],
            terminator: Terminator::Return(constructor_result),
        });
        current = next;
    }
    let result = ValueId(function.values.len());
    function.values.push(Value {
        ty: Type::Bool,
        span: span.clone(),
    });
    function.blocks[current].instructions.push(Instruction {
        result,
        span,
        operation: Operation::Nominal {
            operation: plan.kind.guard(),
            arguments: vec![ctor],
        },
    });
    function.blocks[current].terminator = Terminator::Branch {
        condition: result,
        consequent: block_ids[&root],
        alternative: slow,
    };
    Ok(())
}
