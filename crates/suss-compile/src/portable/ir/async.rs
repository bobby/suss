//! Normalize evaluated awaits into compiled continuation states, not source
//! instructions for a runtime interpreter. The emitter turns these states and
//! genuine analyzed regions into native Wasm resume closures.
use super::{Function, Lowerer, Operation, Terminator, ValueId, liveness};
use crate::portable::{
    Diagnostic,
    hir::{BindingId, Hir, Type},
};
use std::{collections::BTreeSet, ops::Range};

// Root-relayed compiler/runtime contract. The runtime validates the snapshot
// before a single-pointer commit; all reference slots are rooted GC Values.
pub const SNAPSHOT_SIZE: usize = 8;
pub const PC: usize = 0;
pub const LIVE: usize = 1;
pub const UNWIND: usize = 2;
pub const DYNAMIC: usize = 3;
pub const OWNER: usize = 4;
pub const GENERATION: usize = 5;
pub const EVENT_KIND: usize = 6;
pub const EVENT_VALUE: usize = 7;

#[derive(Debug, Clone)]
pub enum Exit {
    /// Original return/throw/branch/jump, retaining simultaneous edge arguments.
    Control,
    Suspend {
        future: ValueId,
        resume: usize,
        result: ValueId,
        /// Values read after resume, excluding the result not produced yet.
        spill: Vec<ValueId>,
    },
}
#[derive(Debug, Clone)]
pub struct State {
    pub block: usize,
    /// Original block parameters are installed only on entry, not after await.
    pub entry: bool,
    /// Exact slice of the original block; operations are never duplicated.
    pub instructions: Range<usize>,
    pub exit: Exit,
    pub span: Range<usize>,
}
#[derive(Debug, Clone)]
pub struct Continuation {
    pub function: Function,
    pub states: Vec<State>,
    /// Translate original CFG edges to the first state of each original block.
    pub block_entries: Vec<usize>,
    /// True CFG backedges: the destination dominates the predecessor. Each
    /// traversed edge ends a turn after parallel parameter transport.
    pub backedges: BTreeSet<(usize, usize)>,
}

/// Lower an analyzed suspendable body with explicit lexical capture types.
/// Ordinary function lowering keeps its separate non-suspendable lowerer.
/// Nested futures retain their own compiled plans; try/dynamic regions are
/// lowered inline with rooted unwind state rather than synchronous closures.
pub fn lower(body: &Hir, captures: &[(BindingId, Type)]) -> Result<Continuation, Diagnostic> {
    let mut lowerer = Lowerer::new(body.span.clone());
    lowerer.suspendable = true;
    for (id, ty) in captures {
        lowerer.parameter(*id, *ty, body.span.clone())?;
    }
    normalize(lowerer.finish(body)?)
}

/// Split each await after evaluating its operand once. Keep the original value
/// ID space so GC slots, closure captures and parallel loop-edge assignments
/// refer to exactly the same evaluated values before and after suspension.
pub fn normalize(function: Function) -> Result<Continuation, Diagnostic> {
    let live = liveness::analyze(&function)?;
    let mut states = Vec::new();
    let mut entries = Vec::with_capacity(function.blocks.len());
    for (block_id, block) in function.blocks.iter().enumerate() {
        entries.push(states.len());
        let mut start = 0;
        let mut entry = true;
        for (index, instruction) in block.instructions.iter().enumerate() {
            if let Operation::Await { future } = &instruction.operation {
                let mut spill = live.blocks[block_id].live_after[index].clone();
                spill.remove(&instruction.result.0);
                // Retain the dependency independently of later source uses: it
                // supplies the event and must survive while the task is pending.
                spill.insert(future.0);
                let resume = states.len() + 1;
                states.push(State {
                    block: block_id,
                    entry,
                    instructions: start..index,
                    exit: Exit::Suspend {
                        future: *future,
                        resume,
                        result: instruction.result,
                        spill: spill.into_iter().map(ValueId).collect(),
                    },
                    span: instruction.span.clone(),
                });
                entry = false;
                start = index + 1;
            }
        }
        states.push(State {
            block: block_id,
            entry,
            instructions: start..block.instructions.len(),
            exit: Exit::Control,
            span: function.span.clone(),
        });
    }
    let backedges = cooperative_backedges(&function)?;
    let result = Continuation {
        function,
        states,
        block_entries: entries,
        backedges,
    };
    verify(&result)?;
    Ok(result)
}

pub fn verify(continuation: &Continuation) -> Result<(), Diagnostic> {
    let fail = |message: &str| Diagnostic {
        span: continuation.function.span.clone(),
        message: message.into(),
    };
    let live = liveness::analyze(&continuation.function)?;
    if continuation.block_entries.len() != continuation.function.blocks.len()
        || continuation.states.is_empty()
    {
        return Err(fail("Invalid continuation state catalog"));
    }
    if continuation.backedges != cooperative_backedges(&continuation.function)? {
        return Err(fail("Invalid continuation cooperative checkpoints"));
    }
    let mut next = 0;
    for (block_id, block) in continuation.function.blocks.iter().enumerate() {
        if continuation.block_entries[block_id] != next {
            return Err(fail("Invalid continuation block entry"));
        }
        let mut original_start = 0;
        for (index, instruction) in block.instructions.iter().enumerate() {
            let Operation::Await { future } = &instruction.operation else {
                continue;
            };
            let state = continuation
                .states
                .get(next)
                .ok_or_else(|| fail("Missing suspension state"))?;
            let Exit::Suspend {
                future: actual,
                resume,
                result,
                spill,
            } = &state.exit
            else {
                return Err(fail("Missing suspension edge"));
            };
            let mut expected = live.blocks[block_id].live_after[index].clone();
            expected.remove(&instruction.result.0);
            expected.insert(future.0);
            if *actual != *future
                || *result != instruction.result
                || *resume != next + 1
                || spill.iter().map(|v| v.0).collect::<BTreeSet<_>>() != expected
                || spill.len() != expected.len()
            {
                return Err(fail("Invalid continuation spill or resume result"));
            }
            check_instructions(
                state,
                block_id,
                original_start == 0,
                original_start..index,
                &fail,
            )?;
            original_start = index + 1;
            next += 1;
        }
        let state = continuation
            .states
            .get(next)
            .ok_or_else(|| fail("Missing continuation exit state"))?;
        let Exit::Control = &state.exit else {
            return Err(fail("Invalid continuation exit"));
        };
        check_instructions(
            state,
            block_id,
            original_start == 0,
            original_start..block.instructions.len(),
            &fail,
        )?;
        next += 1;
    }
    if next != continuation.states.len() {
        return Err(fail("Extra continuation states"));
    }
    Ok(())
}

fn destinations(terminator: &Terminator) -> Vec<usize> {
    match terminator {
        Terminator::Return(_) | Terminator::Throw(_) => Vec::new(),
        Terminator::Jump { target, .. } => vec![*target],
        Terminator::Branch {
            consequent,
            alternative,
            ..
        } => vec![*consequent, *alternative],
    }
}
/// Verified structured task graphs yield at every natural loop backedge. Reject
/// an irreducible cycle rather than silently permitting a non-yielding cycle.
fn cooperative_backedges(function: &Function) -> Result<BTreeSet<(usize, usize)>, Diagnostic> {
    let count = function.blocks.len();
    let all: BTreeSet<_> = (0..count).collect();
    let mut predecessors = vec![Vec::new(); count];
    for (from, block) in function.blocks.iter().enumerate() {
        for to in destinations(&block.terminator) {
            predecessors[to].push(from);
        }
    }
    let mut dominators = vec![all; count];
    dominators[0] = BTreeSet::from([0]);
    loop {
        let mut changed = false;
        for block in 1..count {
            let mut next: BTreeSet<_> = (0..count).collect();
            for from in &predecessors[block] {
                next.retain(|id| dominators[*from].contains(id));
            }
            next.insert(block);
            if next != dominators[block] {
                dominators[block] = next;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut backedges = BTreeSet::new();
    let mut indegree = vec![0; count];
    for (from, block) in function.blocks.iter().enumerate() {
        for to in destinations(&block.terminator) {
            if dominators[from].contains(&to) {
                backedges.insert((from, to));
            } else {
                indegree[to] += 1;
            }
        }
    }
    let mut ready: Vec<_> = (0..count).filter(|id| indegree[*id] == 0).collect();
    let mut visited = 0;
    while let Some(from) = ready.pop() {
        visited += 1;
        for to in destinations(&function.blocks[from].terminator) {
            if !backedges.contains(&(from, to)) {
                indegree[to] -= 1;
                if indegree[to] == 0 {
                    ready.push(to);
                }
            }
        }
    }
    if visited != count {
        return Err(Diagnostic {
            span: function.span.clone(),
            message:
                "Irreducible async control-flow cycle requires cooperative checkpoint lowering"
                    .into(),
        });
    }
    Ok(backedges)
}

fn check_instructions(
    state: &State,
    block: usize,
    entry: bool,
    original: Range<usize>,
    fail: &impl Fn(&str) -> Diagnostic,
) -> Result<(), Diagnostic> {
    if state.block != block || state.entry != entry || state.instructions != original {
        return Err(fail("Changed continuation instruction order"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::ir::{Block, Instruction, Terminator, Value};

    fn two_awaits() -> Function {
        Function {
            values: (0..3)
                .map(|_| Value {
                    ty: Type::Value,
                    span: 0..1,
                })
                .collect(),
            blocks: vec![Block {
                parameters: vec![ValueId(0)],
                instructions: vec![
                    Instruction {
                        result: ValueId(1),
                        operation: Operation::Await { future: ValueId(0) },
                        span: 0..1,
                    },
                    Instruction {
                        result: ValueId(2),
                        operation: Operation::Await { future: ValueId(1) },
                        span: 0..1,
                    },
                ],
                terminator: Terminator::Return(ValueId(2)),
            }],
            span: 0..1,
        }
    }

    #[test]
    fn consecutive_awaits_keep_result_dependency_and_entry_once() {
        let plan = normalize(two_awaits()).unwrap();
        assert_eq!(plan.block_entries, vec![0]);
        assert_eq!(plan.states.len(), 3);
        assert!(plan.states[0].entry);
        assert!(!plan.states[1].entry);
        assert!(!plan.states[2].entry);
        for (i, state) in plan.states[..2].iter().enumerate() {
            let Exit::Suspend {
                future,
                result,
                resume,
                spill,
            } = &state.exit
            else {
                panic!("suspend")
            };
            assert_eq!(*future, ValueId(i));
            assert_eq!(*result, ValueId(i + 1));
            assert_eq!(*resume, i + 1);
            assert_eq!(spill, &vec![ValueId(i)]);
            assert!(state.instructions.is_empty());
        }
    }

    #[test]
    fn rejects_replayed_entry_and_missing_pending_dependency() {
        let mut plan = normalize(two_awaits()).unwrap();
        plan.states[1].entry = true;
        assert!(verify(&plan).is_err());
        plan.states[1].entry = false;
        let Exit::Suspend { spill, .. } = &mut plan.states[0].exit else {
            panic!("suspend")
        };
        spill.clear();
        assert!(verify(&plan).is_err());
    }
    #[test]
    fn checkpoints_true_backedges_and_verifies_the_catalog() {
        let function = Function {
            values: vec![
                Value {
                    ty: Type::Number,
                    span: 0..1,
                },
                Value {
                    ty: Type::Bool,
                    span: 0..1,
                },
                Value {
                    ty: Type::Number,
                    span: 0..1,
                },
                Value {
                    ty: Type::Number,
                    span: 0..1,
                },
            ],
            blocks: vec![
                Block {
                    parameters: vec![ValueId(0), ValueId(1)],
                    instructions: vec![],
                    terminator: Terminator::Jump {
                        target: 1,
                        arguments: vec![ValueId(0)],
                    },
                },
                Block {
                    parameters: vec![ValueId(2)],
                    instructions: vec![],
                    terminator: Terminator::Branch {
                        condition: ValueId(1),
                        consequent: 2,
                        alternative: 3,
                    },
                },
                Block {
                    parameters: vec![],
                    instructions: vec![Instruction {
                        result: ValueId(3),
                        operation: Operation::Arithmetic {
                            operator: crate::portable::hir::Arithmetic::Add,
                            arguments: vec![ValueId(2), ValueId(0)],
                        },
                        span: 0..1,
                    }],
                    terminator: Terminator::Jump {
                        target: 1,
                        arguments: vec![ValueId(3)],
                    },
                },
                Block {
                    parameters: vec![],
                    instructions: vec![],
                    terminator: Terminator::Return(ValueId(2)),
                },
            ],
            span: 0..1,
        };
        let mut plan = normalize(function).unwrap();
        assert_eq!(plan.backedges, BTreeSet::from([(2, 1)]));
        plan.backedges.clear();
        assert!(verify(&plan).is_err());
    }

    #[test]
    fn a_lower_numbered_join_is_not_a_loop_checkpoint() {
        let function = Function {
            values: vec![
                Value {
                    ty: Type::Bool,
                    span: 0..1,
                },
                Value {
                    ty: Type::Number,
                    span: 0..1,
                },
                Value {
                    ty: Type::Number,
                    span: 0..1,
                },
                Value {
                    ty: Type::Number,
                    span: 0..1,
                },
            ],
            blocks: vec![
                Block {
                    parameters: vec![ValueId(0)],
                    instructions: vec![],
                    terminator: Terminator::Branch {
                        condition: ValueId(0),
                        consequent: 2,
                        alternative: 3,
                    },
                },
                Block {
                    parameters: vec![ValueId(1)],
                    instructions: vec![],
                    terminator: Terminator::Return(ValueId(1)),
                },
                Block {
                    parameters: vec![],
                    instructions: vec![Instruction {
                        result: ValueId(2),
                        operation: Operation::Literal(crate::portable::hir::Literal::Number(1.0)),
                        span: 0..1,
                    }],
                    terminator: Terminator::Jump {
                        target: 1,
                        arguments: vec![ValueId(2)],
                    },
                },
                Block {
                    parameters: vec![],
                    instructions: vec![Instruction {
                        result: ValueId(3),
                        operation: Operation::Literal(crate::portable::hir::Literal::Number(2.0)),
                        span: 0..1,
                    }],
                    terminator: Terminator::Jump {
                        target: 1,
                        arguments: vec![ValueId(3)],
                    },
                },
            ],
            span: 0..1,
        };
        assert!(normalize(function).unwrap().backedges.is_empty());
    }

    #[test]
    fn rejects_irreducible_cycles_without_a_cooperative_cut() {
        let function = Function {
            values: vec![Value {
                ty: Type::Bool,
                span: 0..1,
            }],
            blocks: vec![
                Block {
                    parameters: vec![ValueId(0)],
                    instructions: vec![],
                    terminator: Terminator::Branch {
                        condition: ValueId(0),
                        consequent: 1,
                        alternative: 2,
                    },
                },
                Block {
                    parameters: vec![],
                    instructions: vec![],
                    terminator: Terminator::Branch {
                        condition: ValueId(0),
                        consequent: 2,
                        alternative: 3,
                    },
                },
                Block {
                    parameters: vec![],
                    instructions: vec![],
                    terminator: Terminator::Jump {
                        target: 1,
                        arguments: vec![],
                    },
                },
                Block {
                    parameters: vec![],
                    instructions: vec![],
                    terminator: Terminator::Return(ValueId(0)),
                },
            ],
            span: 0..1,
        };
        assert!(
            normalize(function)
                .unwrap_err()
                .message
                .contains("Irreducible async control-flow cycle")
        );
    }
}

impl Lowerer {
    /// Inline genuine analyzed regions into the task CFG. Latent exceptional
    /// branches make handler/cleanup locals and reachability visible to normal
    /// IR verification; native emission bypasses them only during unwind.
    pub(super) fn async_region(
        &mut self,
        hir: &Hir,
        body: &Hir,
        handler: Option<&Hir>,
        cleanup: Option<&Hir>,
        payload: Option<&crate::portable::hir::Parameter>,
        dynamic: Vec<ValueId>,
    ) -> Result<Option<ValueId>, Diagnostic> {
        use crate::portable::{hir::Literal, ir::Terminator};
        let bindings = self.bindings.clone();
        let body_block = self.block(Vec::new());
        let handler_block = handler.map(|_| self.block(Vec::new()));
        let cleanup_block = cleanup.map(|_| self.block(Vec::new()));
        let end = self.block(Vec::new());
        let flag = self.emit(
            Operation::RegionPush {
                handler: handler_block,
                cleanup: cleanup_block,
                end,
                dynamic,
            },
            Type::Bool,
            hir.span.clone(),
        );
        let exceptional: Vec<_> = handler_block.into_iter().chain(cleanup_block).collect();
        let selectors: Vec<_> = exceptional.iter().map(|_| self.block(Vec::new())).collect();
        self.function.blocks[self.current].terminator = Terminator::Branch {
            condition: flag,
            consequent: end,
            alternative: selectors.first().copied().unwrap_or(body_block),
        };
        for (index, selector) in selectors.iter().enumerate() {
            self.current = *selector;
            let flag = self.literal(Literal::Bool(false), hir.span.clone());
            self.function.blocks[*selector].terminator = Terminator::Branch {
                condition: flag,
                consequent: exceptional[index],
                alternative: selectors.get(index + 1).copied().unwrap_or(body_block),
            };
        }
        self.current = body_block;
        if let Some(value) = self.expression(body)? {
            self.emit(
                Operation::RegionExit {
                    value,
                    cleanup: false,
                },
                Type::Value,
                body.span.clone(),
            );
            self.function.blocks[self.current].terminator = Terminator::Jump {
                target: cleanup_block.unwrap_or(end),
                arguments: Vec::new(),
            };
        }
        if let (Some(handler), Some(block)) = (handler, handler_block) {
            self.bindings = bindings.clone();
            self.current = block;
            if let Some(payload) = payload {
                let value = self.emit(Operation::RegionValue, Type::Value, payload.span.clone());
                self.bindings.insert(payload.id, value);
            }
            if let Some(value) = self.expression(handler)? {
                self.emit(
                    Operation::RegionExit {
                        value,
                        cleanup: false,
                    },
                    Type::Value,
                    handler.span.clone(),
                );
                self.function.blocks[self.current].terminator = Terminator::Jump {
                    target: cleanup_block.unwrap_or(end),
                    arguments: Vec::new(),
                };
            }
        }
        if let (Some(cleanup), Some(block)) = (cleanup, cleanup_block) {
            self.bindings = bindings.clone();
            self.current = block;
            if let Some(value) = self.expression(cleanup)? {
                self.emit(
                    Operation::RegionExit {
                        value,
                        cleanup: true,
                    },
                    Type::Value,
                    cleanup.span.clone(),
                );
                self.function.blocks[self.current].terminator = Terminator::Jump {
                    target: end,
                    arguments: Vec::new(),
                };
            }
        }
        self.bindings = bindings;
        self.current = end;
        Ok(Some(self.emit(
            Operation::RegionValue,
            Type::Value,
            hir.span.clone(),
        )))
    }
}
