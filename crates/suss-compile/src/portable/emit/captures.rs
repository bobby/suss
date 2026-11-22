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
        if !function.blocks.iter().flat_map(|block| &block.instructions).any(|inst|
            matches!(inst.operation, Operation::MakeClosure { .. } | Operation::MakeGeneralClosure { .. } | Operation::MakeFuture { .. }))
        { return; }
        let potential = function.blocks.iter().flat_map(|block| &block.instructions).flat_map(|inst| match &inst.operation {
            Operation::MakeClosure { captures, .. } | Operation::MakeGeneralClosure { captures, .. } => captures.as_slice(),
            _ => &[],
        }).copied().collect::<Vec<_>>();
        let reprs = unboxed::infer(function, resumable, entries, None, &potential);
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
            while storage.iter().any(|repr| *repr != unboxed::Repr::Boxed) {
                let mut rejected = vec![false; storage.len()];
                for method in &methods {
                    let entries = seeds(method, &storage);
                    let inferred = unboxed::infer(&method.function, false, &entries, None, &[]);
                    let locals = unboxed::Locals::new(inferred, &method.function, 0, 0, 0);
                    let observed = object_uses(&method.function, &locals);
                    for (index, repr) in storage.iter().enumerate() {
                        if *repr != unboxed::Repr::Boxed
                            && observed[method.function.blocks[0].parameters[index].0]
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

/// Backward demand over SSA aliases. One worklist handles all captures, so
/// large environments do not rescan the graph once per captured value.
fn object_uses(function: &IrFunction, locals: &unboxed::Locals) -> Vec<bool> {
    let mut incoming = vec![Vec::new(); function.values.len()];
    let mut pending = Vec::new();
    for block in &function.blocks {
        for inst in &block.instructions {
            let numeric = match &inst.operation {
                // Unary arithmetic can preserve the original object.
                Operation::Arithmetic { arguments, .. } => arguments.len() >= 2,
                Operation::Comparison { .. } | Operation::Bitwise { .. } => true,
                _ => false,
            };
            if !numeric || !unboxed::inline(locals, inst) {
                pending.extend_from_slice(ir::operands(&inst.operation));
            }
        }
        match &block.terminator {
            Terminator::Return(value) | Terminator::Throw(value) => pending.push(*value),
            Terminator::Branch { condition, .. } => pending.push(*condition),
            Terminator::Jump { target, arguments } => {
                for (argument, parameter) in arguments.iter().zip(&function.blocks[*target].parameters) {
                    incoming[parameter.0].push(*argument);
                    if locals.of(*parameter) != locals.of(*argument) { pending.push(*argument); }
                }
            }
            _ => {}
        }
    }
    let mut observed = vec![false; function.values.len()];
    while let Some(value) = pending.pop() {
        if observed[value.0] { continue; }
        observed[value.0] = true;
        pending.extend_from_slice(&incoming[value.0]);
    }
    observed
}

/// Captures whose selected fields consume raw locals rather than GC locals.
pub(super) fn raw_uses(function: &IrFunction, layout: &Layout) -> Vec<ir::ValueId> {
    function.blocks.iter().flat_map(|block| &block.instructions).flat_map(|inst| {
        let (definition, captures) = match &inst.operation {
            Operation::MakeClosure { body, captures } => (layout.single(body), captures),
            Operation::MakeGeneralClosure { body, captures } => (layout.general(body), captures),
            _ => return Vec::new(),
        };
        captures.iter().zip(&definition.capture_reprs).filter_map(|(value, repr)|
            (*repr != unboxed::Repr::Boxed).then_some(*value)).collect::<Vec<_>>()
    }).collect()
}
