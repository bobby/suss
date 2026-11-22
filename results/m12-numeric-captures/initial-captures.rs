//! Numeric capture storage is physical; source overload facts stay in HIR.
//!
//! Keep an object field whenever the original value can be observed as an
//! object. In particular, returning a capture on two calls must preserve its
//! shared box. Raw fields are admitted only for numeric operations whose
//! result is a new value, with raw control-flow transfers checked transitively.
use super::{ir, unboxed, ClosureBody, IrFunction, Layout, Operation, Terminator};

pub(super) fn plan(root: &IrFunction, layout: &mut Layout) {
    visit(root, &[], false, layout);
}

fn visit(function: &IrFunction, entries: &[(ir::ValueId, unboxed::Repr)], resumable: bool, layout: &mut Layout) {
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
        let reprs = unboxed::infer(function, resumable, entries, None);
        for inst in function.blocks.iter().flat_map(|block| &block.instructions) {
            let (definition, captures, methods) = match &inst.operation {
                Operation::MakeClosure { body, captures } => (layout.single_index(body), captures, vec![body.as_ref()]),
                Operation::MakeGeneralClosure { body, captures } =>
                    (layout.general_index(body), captures, body.methods.iter().collect()),
                Operation::MakeFuture { body, .. } => {
                    visit(&body.continuation.function, &[], true, layout);
                    continue;
                }
                _ => continue,
            };
            let mut storage = captures.iter().map(|value| match reprs[value.0] {
                repr @ (unboxed::Repr::Long | unboxed::Repr::Double) => repr,
                _ => unboxed::Repr::Boxed,
            }).collect::<Vec<_>>();
            // Removing a candidate can change other expressions' inference.
            // Iterate monotonically until all methods accept the same layout.
            loop {
                let mut rejected = vec![false; storage.len()];
                for method in &methods {
                    let entries = seeds(method, &storage);
                    let inferred = unboxed::infer(&method.function, false, &entries, None);
                    let locals = unboxed::Locals::new(inferred, &method.function, 0, 0, 0);
                    for (index, repr) in storage.iter().enumerate() {
                        if *repr != unboxed::Repr::Boxed && !numeric_uses(&method.function,
                            method.function.blocks[0].parameters[index], &locals)
                        { rejected[index] = true; }
                    }
                }
                if !rejected.iter().any(|value| *value) { break; }
                for (repr, rejected) in storage.iter_mut().zip(rejected) {
                    if rejected { *repr = unboxed::Repr::Boxed; }
                }
            }
            layout.definitions[definition].capture_reprs = storage.clone();
            for method in methods { visit(&method.function, &seeds(method, &storage), false, layout); }
        }
    });
}

fn seeds(body: &ClosureBody, storage: &[unboxed::Repr]) -> Vec<(ir::ValueId, unboxed::Repr)> {
    body.function.blocks[0].parameters.iter().copied().zip(storage.iter().copied())
        .filter(|(_, repr)| *repr != unboxed::Repr::Boxed).collect()
}

fn numeric_uses(function: &IrFunction, capture: ir::ValueId, locals: &unboxed::Locals) -> bool {
    let mut aliases = vec![false; function.values.len()];
    aliases[capture.0] = true;
    loop {
        let mut changed = false;
        for block in &function.blocks {
            if let Terminator::Jump { target, arguments } = &block.terminator {
                for (argument, parameter) in arguments.iter().zip(&function.blocks[*target].parameters) {
                    if aliases[argument.0] && !aliases[parameter.0] {
                        aliases[parameter.0] = true;
                        changed = true;
                    }
                }
            }
        }
        if !changed { break; }
    }
    for block in &function.blocks {
        for inst in &block.instructions {
            if ir::operands(&inst.operation).iter().any(|value| aliases[value.0]) {
                let numeric = match &inst.operation {
                    // Unary arithmetic can preserve the original object.
                    Operation::Arithmetic { arguments, .. } => arguments.len() >= 2,
                    Operation::Comparison { .. } | Operation::Bitwise { .. } => true,
                    _ => false,
                };
                if !numeric || !unboxed::inline(locals, inst) { return false; }
            }
        }
        match &block.terminator {
            Terminator::Return(value) | Terminator::Throw(value) if aliases[value.0] => return false,
            Terminator::Branch { condition, .. } if aliases[condition.0] => return false,
            Terminator::Jump { target, arguments } => {
                for (argument, parameter) in arguments.iter().zip(&function.blocks[*target].parameters) {
                    if aliases[argument.0] && locals.of(*parameter) != locals.of(*argument) { return false; }
                }
            }
            _ => {}
        }
    }
    true
}
