//! Continuation analysis prerequisites only: no future or scheduler acceptance.
use suss_compile::portable::{
    self,
    hir::{Literal, Type},
    ir::{
        self, Block, Function, Instruction, Operation, Terminator, Value, ValueId,
        liveness::{LiveSet, analyze},
    },
};

fn lowered(source: &str) -> Function {
    let function = ir::lower(&portable::analyze(source).unwrap()).unwrap();
    ir::verify(&function).unwrap();
    function
}

fn literal_id(function: &Function, expected: f64) -> usize {
    function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find_map(|instruction| match instruction.operation {
            Operation::Literal(Literal::Number(value)) if value == expected => {
                Some(instruction.result.0)
            }
            _ => None,
        })
        .unwrap()
}

#[test]
fn lowered_branch_captures_are_live_but_discarded_locals_are_not() {
    let function = lowered("(let [x 11 y 22 unused 99] (if true (fn [] (+ x 333)) (fn [] y)))");
    let result = analyze(&function).unwrap();
    let x = literal_id(&function, 11.0);
    let y = literal_id(&function, 22.0);
    let unused = literal_id(&function, 99.0);
    let (branch_id, condition) = function
        .blocks
        .iter()
        .enumerate()
        .find_map(|(id, block)| match block.terminator {
            Terminator::Branch { condition, .. } => Some((id, condition.0)),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        result.blocks[branch_id].live_out,
        LiveSet::from([x, y, condition])
    );
    assert!(
        result
            .blocks
            .iter()
            .all(|block| !block.live_out.contains(&unused))
    );
    let mut closures = 0;
    for (block_id, block) in function.blocks.iter().enumerate() {
        for (index, instruction) in block.instructions.iter().enumerate() {
            if let Operation::MakeClosure { captures, body } = &instruction.operation {
                closures += 1;
                assert_eq!(
                    result.blocks[block_id].live_in,
                    captures.iter().map(|id| id.0).collect()
                );
                assert_eq!(
                    result.blocks[block_id].live_after[index],
                    LiveSet::from([instruction.result.0])
                );
                // Captured closure entries can be analyzed independently; their
                // parameter IDs must never leak into the enclosing function.
                let nested = analyze(&body.function).unwrap();
                assert!(nested.blocks[0].live_in.contains(&0));
            }
        }
    }
    assert_eq!(closures, 2);
    assert_eq!(analyze(&function).unwrap(), result);
}

#[test]
fn lowered_phi_edges_use_predecessor_arguments_not_destination_ids() {
    let function = lowered("(if true 11 22)");
    let result = analyze(&function).unwrap();
    let mut edges = 0;
    for (block_id, block) in function.blocks.iter().enumerate() {
        if let Terminator::Jump { target, arguments } = &block.terminator {
            edges += 1;
            assert_eq!(
                result.blocks[*target].live_in,
                function.blocks[*target]
                    .parameters
                    .iter()
                    .map(|id| id.0)
                    .collect()
            );
            assert_eq!(
                result.blocks[block_id].live_out,
                arguments.iter().map(|id| id.0).collect()
            );
            for parameter in &function.blocks[*target].parameters {
                assert!(!result.blocks[block_id].live_out.contains(&parameter.0));
            }
        }
    }
    assert_eq!(edges, 2);
}

#[test]
fn lowered_loop_backedge_preserves_outer_values_and_substitutes_parameters() {
    let function = lowered(
        "(let [saved 77 unused 99] (loop [flag true x 1] (if flag (recur false 2) (+ saved x))))",
    );
    let result = analyze(&function).unwrap();
    let saved = literal_id(&function, 77.0);
    let unused = literal_id(&function, 99.0);
    let (header, parameters) = function
        .blocks
        .iter()
        .enumerate()
        .find(|(_, block)| block.parameters.len() == 2)
        .unwrap();
    assert!(result.blocks[header].live_in.contains(&saved));
    let mut incoming = 0;
    for (block_id, block) in function.blocks.iter().enumerate() {
        if let Terminator::Jump { target, arguments } = &block.terminator {
            if *target == header {
                incoming += 1;
                let expected = arguments.iter().map(|id| id.0).chain([saved]).collect();
                assert_eq!(result.blocks[block_id].live_out, expected);
                for parameter in &parameters.parameters {
                    assert!(!result.blocks[block_id].live_out.contains(&parameter.0));
                }
            }
        }
    }
    assert_eq!(incoming, 2);
    assert!(
        result
            .blocks
            .iter()
            .all(|block| !block.live_out.contains(&unused))
    );
}

fn graph(types: &[Type], blocks: Vec<Block>) -> Function {
    Function {
        values: types
            .iter()
            .map(|ty| Value {
                ty: *ty,
                span: 0..1,
            })
            .collect(),
        blocks,
        span: 0..1,
    }
}
fn literal(result: usize, value: Literal) -> Instruction {
    Instruction {
        result: ValueId(result),
        operation: Operation::Literal(value),
        span: 0..1,
    }
}

#[test]
fn self_edge_substitution_is_parallel_and_keeps_current_ir_transport_operands() {
    let function = graph(
        &[Type::Number; 4],
        vec![
            Block {
                parameters: vec![],
                instructions: vec![
                    literal(0, Literal::Number(11.0)),
                    literal(1, Literal::Number(22.0)),
                ],
                terminator: Terminator::Jump {
                    target: 1,
                    arguments: vec![ValueId(0), ValueId(1)],
                },
            },
            Block {
                parameters: vec![ValueId(2), ValueId(3)],
                instructions: vec![],
                terminator: Terminator::Jump {
                    target: 1,
                    arguments: vec![ValueId(3), ValueId(2)],
                },
            },
        ],
    );
    ir::verify(&function).unwrap();
    let result = analyze(&function).unwrap();
    assert_eq!(result.blocks[0].live_out, LiveSet::from([0, 1]));
    assert_eq!(result.blocks[0].live_in, LiveSet::new());
    assert_eq!(result.blocks[1].live_in, LiveSet::from([2, 3]));
    assert_eq!(result.blocks[1].live_out, LiveSet::from([2, 3]));
}

#[test]
fn return_throw_and_instruction_boundaries_keep_only_remaining_uses() {
    for throwing in [false, true] {
        let function = graph(
            &[Type::Number; 2],
            vec![Block {
                parameters: vec![],
                instructions: vec![
                    literal(0, Literal::Number(11.0)),
                    literal(1, Literal::Number(99.0)),
                ],
                terminator: if throwing {
                    Terminator::Throw(ValueId(0))
                } else {
                    Terminator::Return(ValueId(0))
                },
            }],
        );
        let result = analyze(&function).unwrap();
        assert_eq!(result.blocks[0].live_in, LiveSet::new());
        assert_eq!(result.blocks[0].live_out, LiveSet::from([0]));
        assert_eq!(
            result.blocks[0].live_after,
            vec![LiveSet::from([0]), LiveSet::from([0])]
        );
    }
}

#[test]
fn invalid_graph_is_rejected_before_liveness() {
    let mut function = lowered("42");
    function.blocks[0].terminator = Terminator::Jump {
        target: usize::MAX,
        arguments: vec![],
    };
    assert!(analyze(&function).is_err());
}
