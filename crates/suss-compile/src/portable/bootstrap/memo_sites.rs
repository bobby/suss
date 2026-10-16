//! Canonical source sites eligible for future authenticated memo writes.
//! Source spelling alone grants no authority: this catalog is consumed only by
//! embedded-bootstrap generation and must match its actual lowered field stores.
mod plan;
pub(super) use plan::verified_plan;
pub(in crate::portable) use plan::{Emission, VerifiedMemoPlan};
use super::super::{
    Diagnostic,
    hir::Nominal,
    ir::{Function, Operation, Terminator, ValueId},
    resolve::Phase,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};
use suss_reader::forms::{Form, Kind};

// Reviewed canonical owner/slot/method identities; changes require explicit review.
const EXPECTED: &[(&str, usize, &str)] = &[
    (
        "List",
        4,
        "b5a883d8e19f08ef6e82fa48ac64d623be413e2cd0ec56ea4dde3401d20efd7a",
    ),
    (
        "Cons",
        3,
        "b5a883d8e19f08ef6e82fa48ac64d623be413e2cd0ec56ea4dde3401d20efd7a",
    ),
    (
        "Symbol",
        3,
        "f0a590441ca9a32f8e4b04eb958380c07b5cea155c12249991a4705e2e786aa7",
    ),
    (
        "Keyword",
        3,
        "43ce5179180441ba2be0d4e76b92b72e4ae7f7fa621c692f0b903dac984bde8a",
    ),
    (
        "PersistentVector",
        5,
        "b5a883d8e19f08ef6e82fa48ac64d623be413e2cd0ec56ea4dde3401d20efd7a",
    ),
    (
        "MapEntry",
        2,
        "b5a883d8e19f08ef6e82fa48ac64d623be413e2cd0ec56ea4dde3401d20efd7a",
    ),
    (
        "PersistentArrayMap",
        3,
        "18aa0fbfb8d715c11f3ef34d9653a0d5b233d55968dca684d0efbcf3042f349b",
    ),
    (
        "ChunkedSeq",
        5,
        "b5a883d8e19f08ef6e82fa48ac64d623be413e2cd0ec56ea4dde3401d20efd7a",
    ),
    (
        "NodeSeq",
        4,
        "b5a883d8e19f08ef6e82fa48ac64d623be413e2cd0ec56ea4dde3401d20efd7a",
    ),
    (
        "ArrayNodeSeq",
        4,
        "b5a883d8e19f08ef6e82fa48ac64d623be413e2cd0ec56ea4dde3401d20efd7a",
    ),
    (
        "PersistentHashMap",
        5,
        "18aa0fbfb8d715c11f3ef34d9653a0d5b233d55968dca684d0efbcf3042f349b",
    ),
    (
        "PersistentHashSet",
        2,
        "18aa0fbfb8d715c11f3ef34d9653a0d5b233d55968dca684d0efbcf3042f349b",
    ),
    (
        "LazySeq",
        3,
        "b5a883d8e19f08ef6e82fa48ac64d623be413e2cd0ec56ea4dde3401d20efd7a",
    ),
    (
        "ChunkedCons",
        3,
        "b5a883d8e19f08ef6e82fa48ac64d623be413e2cd0ec56ea4dde3401d20efd7a",
    ),
    (
        "Subvec",
        4,
        "b5a883d8e19f08ef6e82fa48ac64d623be413e2cd0ec56ea4dde3401d20efd7a",
    ),
    (
        "PersistentQueueSeq",
        3,
        "b5a883d8e19f08ef6e82fa48ac64d623be413e2cd0ec56ea4dde3401d20efd7a",
    ),
    (
        "PersistentQueue",
        4,
        "b5a883d8e19f08ef6e82fa48ac64d623be413e2cd0ec56ea4dde3401d20efd7a",
    ),
    (
        "BlackNode",
        4,
        "0e5bfe1fa67edceafcfdbc28cba52331de9a4c57f83990ae58a4bb9a61cf2dca",
    ),
    (
        "RedNode",
        4,
        "0e5bfe1fa67edceafcfdbc28cba52331de9a4c57f83990ae58a4bb9a61cf2dca",
    ),
    (
        "PersistentTreeMapSeq",
        4,
        "49122b135c3b37c4face574129605de7febe21350c86d75b4b390f33a39aeb2e",
    ),
    (
        "PersistentTreeMap",
        4,
        "f810c31b8eec23163fffb59883f673bc158bb8c8faf0d085a9eab5ac47ff6c9d",
    ),
    (
        "PersistentTreeSet",
        2,
        "f810c31b8eec23163fffb59883f673bc158bb8c8faf0d085a9eab5ac47ff6c9d",
    ),
    (
        "Repeat",
        4,
        "b5a883d8e19f08ef6e82fa48ac64d623be413e2cd0ec56ea4dde3401d20efd7a",
    ),
    (
        "IntegerRange",
        5,
        "25c228ba1e06ec272b37193d7cce517436237813dba98969ceadde679d3c7bc8",
    ),
    (
        "Range",
        6,
        "25c228ba1e06ec272b37193d7cce517436237813dba98969ceadde679d3c7bc8",
    ),
];

pub(super) fn owner_slots() -> impl Iterator<Item = (&'static str, usize)> {
    EXPECTED.iter().map(|(owner, slot, _)| (*owner, *slot))
}

#[derive(Debug)]
pub(super) struct MemoSite {
    pub owner: String,
    pub field: usize,
    pub span: Range<usize>,
    method_sha256: String,
    receiver_span: Range<usize>,
}
fn symbol(form: &Form, expected: &str) -> bool {
    matches!(&form.kind, Kind::Symbol(name) if name.namespace.is_none() && name.name == expected)
}
fn error(span: Range<usize>, message: &str) -> Diagnostic {
    Diagnostic {
        span,
        message: message.into(),
    }
}
fn catalog(source: &str) -> Result<Vec<MemoSite>, Diagnostic> {
    let forms = suss_reader::forms::read_forms(source)
        .and_then(suss_reader::forms::resolve_conditionals)
        .map_err(|failure| error(failure.span, &failure.message))?;
    let mut sites = Vec::new();
    for form in forms {
        let Kind::List(items) = &form.kind else {
            continue;
        };
        if !items.first().is_some_and(|head| symbol(head, "deftype")) {
            continue;
        }
        let [_, owner, fields, methods @ ..] = items.as_slice() else {
            return Err(error(form.span.clone(), "Malformed canonical type"));
        };
        let (Kind::Symbol(owner), Kind::Vector(fields)) = (&owner.kind, &fields.kind) else {
            return Err(error(form.span.clone(), "Malformed canonical type fields"));
        };
        let mut hash_protocol = false;
        for method in methods {
            if matches!(method.kind, Kind::Symbol(_)) {
                hash_protocol = symbol(method, "IHash");
                continue;
            }
            let Kind::List(parts) = &method.kind else {
                continue;
            };
            if !hash_protocol || !parts.first().is_some_and(|head| symbol(head, "-hash")) {
                continue;
            }
            let Some(Form {
                kind: Kind::Vector(parameters),
                ..
            }) = parts.get(1)
            else {
                return Err(error(
                    method.span.clone(),
                    "Canonical hash method needs one receiver",
                ));
            };
            let [receiver] = parameters.as_slice() else {
                return Err(error(
                    method.span.clone(),
                    "Canonical hash method needs one receiver",
                ));
            };
            let mut pending: Vec<_> = parts.iter().skip(2).collect();
            while let Some(form) = pending.pop() {
                let Kind::List(parts) = &form.kind else {
                    if let Kind::Vector(parts) = &form.kind {
                        pending.extend(parts);
                    }
                    continue;
                };
                let key = match parts.as_slice() {
                    [head, _, _, key] if symbol(head, "caching-hash") => Some(key),
                    [head, key, _]
                        if symbol(head, "set!")
                            && (symbol(key, "__hash") || symbol(key, "_hash")) =>
                    {
                        Some(key)
                    }
                    _ => None,
                };
                if let Some(key) = key {
                    if !(symbol(key, "__hash") || symbol(key, "_hash")) {
                        return Err(error(key.span.clone(), "Unknown canonical hash memo field"));
                    }
                    let field = fields
                        .iter()
                        .position(|field| field.kind == key.kind)
                        .ok_or_else(|| {
                            error(
                                key.span.clone(),
                                "Canonical hash memo is not a declared field",
                            )
                        })?;
                    sites.push(MemoSite {
                        owner: owner.name.clone(),
                        field,
                        span: form.span.clone(),
                        method_sha256: super::sha256(source[method.span.clone()].as_bytes()),
                        receiver_span: receiver.span.clone(),
                    });
                }
                if !parts.first().is_some_and(|head| symbol(head, "quote")) {
                    pending.extend(parts);
                }
            }
        }
    }
    sites.sort_by_key(|site| site.span.start);
    let actual: Vec<_> = sites
        .iter()
        .map(|site| (site.owner.as_str(), site.field, site.method_sha256.as_str()))
        .collect();
    if actual != EXPECTED {
        let mismatch = actual
            .iter()
            .zip(EXPECTED)
            .position(|(actual, expected)| actual != expected)
            .unwrap_or(actual.len().min(EXPECTED.len()));
        return Err(error(
            0..source.len(),
            &format!(
                "Canonical memo owner/slot/method inventory mismatch at {mismatch}: actual {:?}, expected {:?}",
                actual.get(mismatch),
                EXPECTED.get(mismatch)
            ),
        ));
    }
    Ok(sites)
}

fn definition(function: &Function, id: ValueId) -> Option<&Operation> {
    function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .find(|instruction| instruction.result == id)
        .map(|instruction| &instruction.operation)
}
// The generated IFn-key initialization is a forward diamond. Require every
// false-path edge to reach the binder; a return, cycle or bypass is not canonical.
fn bound_region(function: &Function, start: usize, bound: usize) -> Option<BTreeSet<usize>> {
    let mut pending = vec![start];
    let mut seen = BTreeSet::new();
    while let Some(index) = pending.pop() {
        if !seen.insert(index) || index == bound {
            continue;
        }
        let block = function.blocks.get(index)?;
        let successors: Vec<_> = match block.terminator {
            Terminator::Jump { target, .. } => vec![target],
            Terminator::Branch {
                consequent,
                alternative,
                ..
            } => vec![consequent, alternative],
            _ => return None,
        };
        for target in successors {
            if target <= index || target > bound {
                return None;
            }
            pending.push(target);
        }
    }
    Some(seen)
}
fn predecessors(function: &Function, target: usize) -> BTreeSet<usize> {
    function
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| {
            let reaches = match block.terminator {
                Terminator::Jump {
                    target: destination,
                    ..
                } => destination == target,
                Terminator::Branch {
                    consequent,
                    alternative,
                    ..
                } => consequent == target || alternative == target,
                _ => false,
            };
            reaches.then_some(index)
        })
        .collect()
}
fn canonical_dispatch(
    function: &Function,
    raw_block: usize,
    bound_block: usize,
    raw: ValueId,
) -> bool {
    function.blocks.iter().enumerate().any(|(dispatch, block)| {
        let Terminator::Branch { condition, consequent, alternative } = block.terminator else { return false; };
        if consequent != raw_block
            || !matches!(definition(function, condition), Some(Operation::Nominal { operation: Nominal::IsClosure, arguments }) if arguments.as_slice() == [raw])
            || predecessors(function, raw_block) != BTreeSet::from([dispatch]) { return false; }
        let Some(region) = bound_region(function, alternative, bound_block) else { return false; };
        if region.contains(&dispatch) || region.contains(&raw_block) { return false; }
        region.iter().all(|target| predecessors(function, *target).iter().all(|source| {
            region.contains(source) || (*target == alternative && *source == dispatch)
        }))
    })
}

fn canonical_callee(function: &Function, callee: ValueId, expected: &str, phase: Phase) -> bool {
    let is_binding = |value| {
        matches!(definition(function, value), Some(Operation::GlobalRead(global))
        if global.phase() == phase && global.namespace() == "suss.core" && global.name() == expected)
    };
    if is_binding(callee) {
        return true;
    }
    // Ordinary source calls preserve live IFn behavior: the same loaded global
    // either is already a closure or is bound through the canonical IFn key.
    let Some((target, _)) = function
        .blocks
        .iter()
        .enumerate()
        .find(|(_, block)| block.parameters.as_slice() == [callee])
    else {
        return false;
    };
    let mut incoming = Vec::new();
    for (index, block) in function.blocks.iter().enumerate() {
        if let Terminator::Jump {
            target: destination,
            arguments,
        } = &block.terminator
        {
            if *destination == target {
                let [value] = arguments.as_slice() else {
                    return false;
                };
                incoming.push((index, *value));
            }
        }
    }
    let [left, right] = incoming.as_slice() else {
        return false;
    };
    let ((raw_block, raw), (bound_block, bound)) = if is_binding(left.1) {
        (*left, *right)
    } else if is_binding(right.1) {
        (*right, *left)
    } else {
        return false;
    };
    if !function.blocks[raw_block].instructions.is_empty()
        || !canonical_dispatch(function, raw_block, bound_block, raw)
    {
        return false;
    }
    let Some(Operation::Nominal {
        operation: Nominal::BindCallable,
        arguments,
    }) = definition(function, bound)
    else {
        return false;
    };
    let [owner, key] = arguments.as_slice() else {
        return false;
    };
    if *owner != raw {
        return false;
    }
    // Reading the key and binding the object must remain confined to the false
    // arm, in that order. A dominating/hoisted binder changes fast-path effects.
    let instructions = &function.blocks[bound_block].instructions;
    if instructions.len() != 2 || instructions[0].result != *key || instructions[1].result != bound
    {
        return false;
    }
    let Some(Operation::GlobalRead(global)) = definition(function, raw) else {
        return false;
    };
    matches!(definition(function, *key), Some(Operation::GlobalRead(protocol_key))
        if global.phase() == phase && protocol_key.phase() == phase
        && protocol_key.namespace() == "suss.internal.protocol-keys"
        && protocol_key.name() == "key-737573732e636f72652f49466e2f2d696e766f6b652f32")
}

// Prove that the call/store remains confined to the nil-cache miss. The hit
// must return the captured field without evaluating a hash function or receiver.
fn canonical_miss(function: &Function, receiver: ValueId, result: ValueId, field: usize) -> bool {
    let Some((store, block)) = function.blocks.iter().enumerate().find(|(_, block)| {
        block.instructions.iter().any(|inst| {
            matches!(&inst.operation,
            Operation::Nominal { operation: Nominal::FieldSet(slot), arguments }
                if *slot == field && arguments.as_slice() == [receiver, result])
        })
    }) else {
        return false;
    };
    let Some(call_index) = block
        .instructions
        .iter()
        .position(|inst| inst.result == result)
    else {
        return false;
    };
    if !block.instructions.iter().skip(call_index + 1).any(|inst| {
        matches!(&inst.operation,
        Operation::Nominal { operation: Nominal::FieldSet(slot), arguments }
            if *slot == field && arguments.as_slice() == [receiver, result])
    }) {
        return false;
    }
    let Terminator::Jump {
        target: join,
        arguments,
    } = &block.terminator
    else {
        return false;
    };
    if arguments.as_slice() != [result] {
        return false;
    }
    function.blocks.iter().enumerate().any(|(guard, block)| {
        let [read, test] = block.instructions.as_slice() else { return false; };
        if !matches!(&read.operation, Operation::Nominal { operation: Nominal::Field(slot), arguments }
            if *slot == field && arguments.as_slice() == [receiver])
            || !matches!(test.operation, Operation::NilTest(value) if value == read.result) { return false; }
        let Terminator::Branch { condition, consequent, alternative: hit } = block.terminator else { return false; };
        if condition != test.result || guard == 0 || predecessors(function, guard) != BTreeSet::from([0]) { return false; }
        let entry = &function.blocks[0];
        if !entry.instructions.is_empty()
            || !matches!(entry.terminator, Terminator::Jump { target, .. } if target == guard) { return false; }
        let Some(hit_block) = function.blocks.get(hit) else { return false; };
        if !hit_block.instructions.is_empty()
            || predecessors(function, hit) != BTreeSet::from([guard])
            || !matches!(&hit_block.terminator, Terminator::Jump { target, arguments }
                if target == join && arguments.as_slice() == [read.result]) { return false; }
        let Some(join_block) = function.blocks.get(*join) else { return false; };
        if !join_block.instructions.is_empty() || join_block.parameters.len() != 1
            || !matches!(join_block.terminator, Terminator::Return(value) if value == join_block.parameters[0])
            || predecessors(function, *join) != BTreeSet::from([store, hit]) { return false; }
        let Some(region) = bound_region(function, consequent, store) else { return false; };
        if region.contains(&guard) || region.contains(&hit) || region.contains(join) { return false; }
        region.iter().all(|target| predecessors(function, *target).iter().all(|source| {
            region.contains(source) || (*target == consequent && *source == guard)
        }))
    })
}

fn verify_operands(
    site: &MemoSite,
    function: &Function,
    arguments: &[ValueId],
    phase: Phase,
) -> Result<(), Diagnostic> {
    let fail = |message| error(site.span.clone(), message);
    let [receiver, result] = arguments else {
        return Err(fail(
            "Canonical memo store needs receiver and computed result",
        ));
    };
    let parameters = &function
        .blocks
        .first()
        .ok_or_else(|| fail("Canonical memo method has no entry"))?
        .parameters;
    let matches: Vec<_> = parameters
        .iter()
        .filter(|parameter| {
            function
                .values
                .get(parameter.0)
                .is_some_and(|value| value.span == site.receiver_span)
        })
        .collect();
    if matches.as_slice() != [receiver] {
        return Err(fail("Canonical memo store targets the wrong receiver"));
    }
    let Some(Operation::Call { operands }) = definition(function, *result) else {
        return Err(fail(
            "Canonical memo store must use its computed hash result",
        ));
    };
    let [callee, argument] = operands.as_slice() else {
        return Err(fail("Canonical hash computation needs one receiver"));
    };
    if argument != receiver {
        return Err(fail("Canonical hash computation uses the wrong receiver"));
    }
    let expected = match site.owner.as_str() {
        "Symbol" => "hash-symbol",
        "Keyword" => "hash-keyword",
        "PersistentArrayMap" | "PersistentHashMap" | "PersistentHashSet" | "PersistentTreeMap"
        | "PersistentTreeSet" => "hash-unordered-coll",
        _ => "hash-ordered-coll",
    };
    if !canonical_callee(function, *callee, expected, phase) {
        return Err(fail(
            "Canonical hash computation uses the wrong live binding",
        ));
    }
    if !canonical_miss(function, *receiver, *result, site.field) {
        return Err(fail(
            "Canonical memo write is not confined to its nil-cache miss",
        ));
    }
    Ok(())
}

fn required_on_success(function: &Function, required: ValueId) -> bool {
    let Some(required) = function.blocks.iter().position(|block| {
        block
            .instructions
            .iter()
            .any(|inst| inst.result == required)
    }) else {
        return false;
    };
    let mut pending = vec![0];
    let mut visited = BTreeSet::new();
    while let Some(index) = pending.pop() {
        if index == required || !visited.insert(index) {
            continue;
        }
        match function.blocks[index].terminator {
            Terminator::Return(_) => return false,
            Terminator::Jump { target, .. } => pending.push(target),
            Terminator::Branch {
                consequent,
                alternative,
                ..
            } => pending.extend([consequent, alternative]),
            Terminator::Throw(_) => {}
        }
    }
    true
}

fn uses(function: &Function, value: ValueId) -> usize {
    function
        .blocks
        .iter()
        .map(|block| {
            let instructions = block
                .instructions
                .iter()
                .map(|inst| {
                    super::super::ir::operands(&inst.operation)
                        .iter()
                        .filter(|operand| **operand == value)
                        .count()
                })
                .sum::<usize>();
            instructions
                + match &block.terminator {
                    Terminator::Jump { arguments, .. } => arguments
                        .iter()
                        .filter(|operand| **operand == value)
                        .count(),
                    Terminator::Branch { condition, .. } => usize::from(*condition == value),
                    Terminator::Return(operand) | Terminator::Throw(operand) => {
                        usize::from(*operand == value)
                    }
                }
        })
        .sum()
}

fn value_aliases(function: &Function, mut aliases: BTreeSet<usize>) -> BTreeSet<usize> {
    loop {
        let before = aliases.len();
        for block in &function.blocks {
            for inst in &block.instructions {
                if matches!(&inst.operation, Operation::GlobalWrite { value, .. } if aliases.contains(&value.0))
                {
                    aliases.insert(inst.result.0);
                }
            }
            if let Terminator::Jump { target, arguments } = &block.terminator {
                for (value, parameter) in arguments.iter().zip(&function.blocks[*target].parameters)
                {
                    if aliases.contains(&value.0) {
                        aliases.insert(parameter.0);
                    }
                }
            }
        }
        if aliases.len() == before {
            return aliases;
        }
    }
}

fn verify_installations(
    function: &Function,
    sites: &[MemoSite],
    phase: Phase,
    verified: &mut BTreeSet<(usize, usize)>,
    entry: bool,
) -> Result<(), Diagnostic> {
    let instructions: Vec<_> = function
        .blocks
        .iter()
        .flat_map(|block| &block.instructions)
        .collect();
    // Resolve protocol and key aliases once per function, not once per site.
    let protocols = value_aliases(function, instructions.iter().filter_map(|inst| {
        matches!(&inst.operation, Operation::GlobalRead(global)
            if global.phase() == phase && global.namespace() == "suss.core" && global.name() == "IHash").then_some(inst.result.0)
    }).collect());
    let keys = value_aliases(function, instructions.iter().filter_map(|inst| {
        matches!(&inst.operation, Operation::Nominal { operation: Nominal::Key(1), arguments }
            if arguments.len() == 1 && protocols.contains(&arguments[0].0)).then_some(inst.result.0)
    }).collect());
    for instruction in &instructions {
        let Operation::MakeClosure { body, .. } = &instruction.operation else {
            continue;
        };
        for site in sites {
            if !body
                .function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .any(|inst| {
                    inst.span == site.span
                        && matches!(
                            inst.operation,
                            Operation::Nominal {
                                operation: Nominal::FieldSet(_),
                                ..
                            }
                        )
                })
            {
                continue;
            }
            let fail = || {
                error(
                    site.span.clone(),
                    "Canonical memo method has wrong owner/protocol installation",
                )
            };
            let installations: Vec<_> = instructions
                .iter()
                .filter_map(|inst| match &inst.operation {
                    Operation::Nominal {
                        operation: Nominal::Set,
                        arguments,
                    } if arguments.get(2) == Some(&instruction.result) => Some(*inst),
                    _ => None,
                })
                .collect();
            if !entry {
                return Err(fail());
            }
            let [installation] = installations.as_slice() else {
                return Err(fail());
            };
            if uses(function, instruction.result) != 1
                || !required_on_success(function, installation.result)
            {
                return Err(fail());
            }
            let Operation::Nominal { arguments, .. } = &installation.operation else {
                unreachable!()
            };
            let [class, key, _] = arguments.as_slice() else {
                return Err(fail());
            };
            let publications: Vec<_> = instructions
                .iter()
                .filter(|inst| {
                    matches!(&inst.operation,
                Operation::GlobalWrite { global, .. } if global.phase() == phase
                    && global.namespace() == "suss.core" && global.name() == site.owner)
                })
                .collect();
            let [publication] = publications.as_slice() else {
                return Err(fail());
            };
            if !matches!(&publication.operation, Operation::GlobalWrite { value, .. } if value == class)
                || !required_on_success(function, publication.result)
            {
                return Err(fail());
            }
            let mut classes = BTreeSet::from([class.0]);
            for inst in &instructions {
                if matches!(&inst.operation, Operation::GlobalRead(global) if global.phase() == phase && global.namespace() == "suss.core" && global.name() == site.owner)
                {
                    classes.insert(inst.result.0);
                }
            }
            let classes = value_aliases(function, classes);
            if instructions
                .iter()
                .filter(|inst| {
                    matches!(&inst.operation,
                Operation::Nominal { operation: Nominal::Set, arguments }
                    if arguments.first().is_some_and(|value| classes.contains(&value.0)) && arguments.get(1).is_some_and(|value| keys.contains(&value.0)))
                })
                .count()
                != 1
            {
                return Err(fail());
            }
            let Some(Operation::Nominal {
                operation: Nominal::Key(1),
                arguments,
            }) = definition(function, *key)
            else {
                return Err(fail());
            };
            let [protocol] = arguments.as_slice() else {
                return Err(fail());
            };
            if !matches!(definition(function, *protocol), Some(Operation::GlobalRead(global))
                if global.phase() == phase && global.namespace() == "suss.core" && global.name() == "IHash")
            {
                return Err(fail());
            }
            if !verified.insert((site.span.start, site.span.end)) {
                return Err(fail());
            }
        }
        verify_installations(&body.function, sites, phase, verified, false)?;
    }
    Ok(())
}

pub(super) fn verify(
    source: &str,
    function: &Function,
    phase: Phase,
) -> Result<Vec<MemoSite>, Diagnostic> {
    let sites = catalog(source)?;
    let mut expected = BTreeMap::new();
    for site in &sites {
        if expected
            .insert((site.span.start, site.span.end), (site, 0usize))
            .is_some()
        {
            return Err(error(site.span.clone(), "Duplicate canonical memo site"));
        }
    }
    let mut pending = vec![function];
    while let Some(function) = pending.pop() {
        for block in &function.blocks {
            for inst in &block.instructions {
                match &inst.operation {
                    Operation::MakeClosure { body, .. } => pending.push(&body.function),
                    Operation::MakeGeneralClosure { body, .. } => {
                        pending.extend(body.methods.iter().map(|method| &method.function))
                    }
                    Operation::MakeFuture { body, .. } => pending.push(&body.continuation.function),
                    Operation::Nominal {
                        operation: Nominal::FieldSet(field),
                        arguments,
                    } => {
                        if let Some((site, count)) =
                            expected.get_mut(&(inst.span.start, inst.span.end))
                        {
                            if *field != site.field {
                                return Err(error(
                                    inst.span.clone(),
                                    "Canonical memo store targets the wrong field",
                                ));
                            }
                            verify_operands(site, function, arguments, phase)?;
                            *count += 1;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    for (site, count) in expected.values() {
        if *count != 1 {
            return Err(error(
                site.span.clone(),
                &format!(
                    "Canonical {} memo site must lower to exactly one field store, found {count}",
                    site.owner
                ),
            ));
        }
    }
    let mut installed = BTreeSet::new();
    verify_installations(function, &sites, phase, &mut installed, true)?;
    for site in &sites {
        if !installed.contains(&(site.span.start, site.span.end)) {
            return Err(error(
                site.span.clone(),
                "Canonical memo method has no checked owner/protocol installation",
            ));
        }
    }
    Ok(sites)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_memo_store_verification_rejects_changed_and_missing_lowered_writes() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let other_phase = match phase {
                Phase::Runtime => Phase::Macro,
                Phase::Macro => Phase::Runtime,
            };
            let analyzed = super::super::catalog(phase).unwrap();
            let mut ir = super::super::super::ir::lower(&analyzed.hir).unwrap();
            let sites = verify(super::super::SOURCE, &ir, phase).unwrap();
            let site = &sites[0];
            {
                use super::super::super::hir::{Literal, Type};
                use super::super::super::ir::{Block, ClosureBody, Instruction, Value};
                assert!(ir.blocks[0].parameters.is_empty());
                let span = ir.span.clone();
                let wrapped = Function {
                    values: vec![
                        Value {
                            ty: Type::Closure(0),
                            span: span.clone(),
                        },
                        Value {
                            ty: Type::Nil,
                            span: span.clone(),
                        },
                    ],
                    blocks: vec![Block {
                        parameters: vec![],
                        instructions: vec![
                            Instruction {
                                result: ValueId(0),
                                operation: Operation::MakeClosure {
                                    body: Box::new(ClosureBody {
                                        display_name: None,
                                        variadic: false,
                                        capture_types: vec![],
                                        arity: 0,
                                        function: ir.clone(),
                                    }),
                                    captures: vec![],
                                },
                                span: span.clone(),
                            },
                            Instruction {
                                result: ValueId(1),
                                operation: Operation::Literal(Literal::Nil),
                                span: span.clone(),
                            },
                        ],
                        terminator: Terminator::Return(ValueId(1)),
                    }],
                    span,
                };
                super::super::super::ir::verify(&wrapped).unwrap();
                assert!(
                    verify(super::super::SOURCE, &wrapped, phase)
                        .unwrap_err()
                        .message
                        .contains("owner/protocol installation")
                );
            }
            assert!(
                verify(super::super::SOURCE, &ir, other_phase)
                    .unwrap_err()
                    .message
                    .contains("wrong live binding")
            );
            fn edit_method(
                function: &mut Function,
                span: &Range<usize>,
                edit: &mut dyn FnMut(&mut Function),
            ) -> bool {
                if function
                    .blocks
                    .iter()
                    .flat_map(|block| &block.instructions)
                    .any(|inst| {
                        inst.span == *span
                            && matches!(
                                inst.operation,
                                Operation::Nominal {
                                    operation: Nominal::FieldSet(_),
                                    ..
                                }
                            )
                    })
                {
                    edit(function);
                    return true;
                }
                for block in &mut function.blocks {
                    for inst in &mut block.instructions {
                        match &mut inst.operation {
                            Operation::MakeClosure { body, .. } => {
                                if edit_method(&mut body.function, span, edit) {
                                    return true;
                                }
                            }
                            Operation::MakeGeneralClosure { body, .. } => {
                                for method in &mut body.methods {
                                    if edit_method(&mut method.function, span, edit) {
                                        return true;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                false
            }
            let tree = sites
                .iter()
                .find(|site| site.owner == "PersistentTreeSet")
                .unwrap();
            fn corrupt_installation(
                function: &mut Function,
                span: &Range<usize>,
                operand: usize,
            ) -> bool {
                let implementation = function
                    .blocks
                    .iter()
                    .flat_map(|block| &block.instructions)
                    .find_map(|inst| {
                        let Operation::MakeClosure { body, .. } = &inst.operation else {
                            return None;
                        };
                        body.function
                            .blocks
                            .iter()
                            .flat_map(|block| &block.instructions)
                            .any(|inner| {
                                inner.span == *span
                                    && matches!(
                                        inner.operation,
                                        Operation::Nominal {
                                            operation: Nominal::FieldSet(_),
                                            ..
                                        }
                                    )
                            })
                            .then_some(inst.result)
                    });
                if let Some(implementation) = implementation {
                    if operand >= 3 {
                        let (block_index, position) = function.blocks.iter().enumerate().find_map(|(index, block)| {
                            block.instructions.iter().position(|inst| matches!(&inst.operation,
                                Operation::Nominal { operation: Nominal::Set, arguments } if arguments.get(2) == Some(&implementation))).map(|position| (index, position))
                        }).unwrap();
                        if operand == 6 {
                            let result = function.blocks[block_index].instructions[position].result;
                            assert_eq!(
                                uses(function, result),
                                0,
                                "installation result is discarded"
                            );
                            let after = function.blocks[block_index]
                                .instructions
                                .split_off(position + 1);
                            let installation =
                                function.blocks[block_index].instructions.pop().unwrap();
                            let condition = ValueId(function.values.len());
                            function.values.push(super::super::super::ir::Value {
                                ty: super::super::super::hir::Type::Bool,
                                span: span.clone(),
                            });
                            function.blocks[block_index].instructions.push(
                                super::super::super::ir::Instruction {
                                    result: condition,
                                    operation: Operation::Literal(
                                        super::super::super::hir::Literal::Bool(true),
                                    ),
                                    span: span.clone(),
                                },
                            );
                            let installed = function.blocks.len();
                            let continuation = installed + 1;
                            let old = std::mem::replace(
                                &mut function.blocks[block_index].terminator,
                                Terminator::Branch {
                                    condition,
                                    consequent: installed,
                                    alternative: continuation,
                                },
                            );
                            function.blocks.push(super::super::super::ir::Block {
                                parameters: vec![],
                                instructions: vec![installation],
                                terminator: Terminator::Jump {
                                    target: continuation,
                                    arguments: vec![],
                                },
                            });
                            function.blocks.push(super::super::super::ir::Block {
                                parameters: vec![],
                                instructions: after,
                                terminator: old,
                            });
                            return true;
                        }
                        let mut extra = function.blocks[block_index].instructions[position].clone();
                        let Operation::Nominal { arguments, .. } = &mut extra.operation else {
                            unreachable!()
                        };
                        if operand == 3 {
                            let class = arguments[0];
                            extra = function
                                .blocks
                                .iter()
                                .flat_map(|block| &block.instructions)
                                .find(|inst| {
                                    matches!(&inst.operation,
                                Operation::GlobalWrite { value, .. } if *value == class)
                                })
                                .unwrap()
                                .clone();
                        } else if operand >= 7 {
                            let key = arguments[1];
                            arguments[2] = arguments[0];
                            let fresh = ValueId(function.values.len());
                            function.values.push(function.values[key.0].clone());
                            let mut operation = definition(function, key).unwrap().clone();
                            if operand == 8 {
                                let Operation::Nominal {
                                    arguments: key_arguments,
                                    ..
                                } = &mut operation
                                else {
                                    unreachable!()
                                };
                                let protocol = key_arguments[0];
                                let Operation::GlobalRead(global) =
                                    definition(function, protocol).unwrap()
                                else {
                                    unreachable!()
                                };
                                let global = global.clone();
                                let alias = ValueId(function.values.len());
                                function.values.push(function.values[protocol.0].clone());
                                key_arguments[0] = alias;
                                function.blocks[block_index].instructions.insert(
                                    position + 1,
                                    super::super::super::ir::Instruction {
                                        result: alias,
                                        operation: Operation::GlobalWrite {
                                            global,
                                            value: protocol,
                                        },
                                        span: span.clone(),
                                    },
                                );
                            }
                            arguments[1] = fresh;
                            function.blocks[block_index].instructions.insert(
                                position + if operand == 8 { 2 } else { 1 },
                                super::super::super::ir::Instruction {
                                    result: fresh,
                                    operation,
                                    span: span.clone(),
                                },
                            );
                        } else if operand == 4 {
                            arguments[2] = arguments[0];
                        } else {
                            extra.operation = Operation::Nominal {
                                operation: Nominal::Array,
                                arguments: vec![implementation],
                            };
                        }
                        let mut value = function.values[extra.result.0].clone();
                        if operand == 5 {
                            value.ty = super::super::super::hir::Type::Value;
                        }
                        extra.result = ValueId(function.values.len());
                        function.values.push(value);
                        // Place the extra effect after the original installation;
                        // every referenced value already dominates this point.
                        function.blocks[block_index].instructions.insert(
                            position
                                + if operand == 8 {
                                    3
                                } else if operand == 7 {
                                    2
                                } else {
                                    1
                                },
                            extra,
                        );
                        return true;
                    }
                    for inst in function
                        .blocks
                        .iter_mut()
                        .flat_map(|block| &mut block.instructions)
                    {
                        if let Operation::Nominal {
                            operation: Nominal::Set,
                            arguments,
                        } = &mut inst.operation
                        {
                            if arguments.get(2) == Some(&implementation) {
                                arguments[operand] = arguments[if operand == 0 { 1 } else { 0 }];
                                return true;
                            }
                        }
                    }
                }
                for inst in function
                    .blocks
                    .iter_mut()
                    .flat_map(|block| &mut block.instructions)
                {
                    if let Operation::MakeClosure { body, .. } = &mut inst.operation {
                        if corrupt_installation(&mut body.function, span, operand) {
                            return true;
                        }
                    }
                }
                false
            }
            for selected in [site, tree] {
                for operand in 0..9 {
                    let mut changed = ir.clone();
                    assert!(corrupt_installation(&mut changed, &selected.span, operand));
                    super::super::super::ir::verify(&changed).unwrap();
                    assert!(
                        verify(super::super::SOURCE, &changed, phase)
                            .unwrap_err()
                            .message
                            .contains("owner/protocol installation"),
                        "{} operand {operand}",
                        selected.owner
                    );
                }
            }
            for selected in [site, tree] {
                for mutation in 0..4 {
                    let mut changed = ir.clone();
                    assert!(edit_method(&mut changed, &selected.span, &mut |function| {
                        let guard = function
                            .blocks
                            .iter()
                            .position(|block| {
                                block
                                    .instructions
                                    .iter()
                                    .any(|inst| matches!(inst.operation, Operation::NilTest(_)))
                            })
                            .unwrap();
                        match mutation {
                            0 => {
                                let test = function.blocks[guard]
                                    .instructions
                                    .iter_mut()
                                    .find(|inst| matches!(inst.operation, Operation::NilTest(_)))
                                    .unwrap();
                                test.operation = Operation::Literal(
                                    super::super::super::hir::Literal::Bool(true),
                                );
                            }
                            1 => {
                                let Terminator::Branch {
                                    consequent,
                                    alternative,
                                    ..
                                } = &mut function.blocks[guard].terminator
                                else {
                                    unreachable!()
                                };
                                std::mem::swap(consequent, alternative);
                            }
                            2 => {
                                let read = &mut function.blocks[guard].instructions[0];
                                let Operation::Nominal {
                                    operation: Nominal::Field(slot),
                                    ..
                                } = &mut read.operation
                                else {
                                    unreachable!()
                                };
                                *slot = 0;
                            }
                            _ => {
                                let Terminator::Branch {
                                    consequent,
                                    alternative,
                                    ..
                                } = function.blocks[guard].terminator
                                else {
                                    unreachable!()
                                };
                                function.blocks[alternative].terminator = Terminator::Jump {
                                    target: consequent,
                                    arguments: vec![],
                                };
                            }
                        }
                    }));
                    super::super::super::ir::verify(&changed).unwrap();
                    assert!(
                        verify(super::super::SOURCE, &changed, phase)
                            .unwrap_err()
                            .message
                            .contains("nil-cache miss"),
                        "{} mutation {mutation}",
                        selected.owner
                    );
                }
            }
            for mutation in 0..4 {
                let mut changed = ir.clone();
                assert!(edit_method(&mut changed, &tree.span, &mut |function| {
                    let condition = function
                        .blocks
                        .iter()
                        .flat_map(|block| &block.instructions)
                        .find(|inst| {
                            matches!(
                                inst.operation,
                                Operation::Nominal {
                                    operation: Nominal::IsClosure,
                                    ..
                                }
                            )
                        })
                        .unwrap()
                        .result;
                    if mutation >= 2 {
                        let dispatch = function
                            .blocks
                            .iter()
                            .position(|block| {
                                matches!(block.terminator,
                        Terminator::Branch { condition: test, .. } if test == condition)
                            })
                            .unwrap();
                        if mutation == 2 {
                            let binder = function
                                .blocks
                                .iter()
                                .position(|block| {
                                    block.instructions.iter().any(|inst| {
                                        matches!(
                                            inst.operation,
                                            Operation::Nominal {
                                                operation: Nominal::BindCallable,
                                                ..
                                            }
                                        )
                                    })
                                })
                                .unwrap();
                            let hoisted = std::mem::take(&mut function.blocks[binder].instructions);
                            assert_eq!(hoisted.len(), 2);
                            let at = function.blocks[dispatch]
                                .instructions
                                .iter()
                                .position(|inst| inst.result == condition)
                                .unwrap();
                            function.blocks[dispatch]
                                .instructions
                                .splice(at..at, hoisted);
                        } else {
                            let raw_block = match function.blocks[dispatch].terminator {
                                Terminator::Branch { consequent, .. } => consequent,
                                _ => unreachable!(),
                            };
                            let raw = match definition(function, condition).unwrap() {
                                Operation::Nominal { arguments, .. } => arguments[0],
                                _ => unreachable!(),
                            };
                            let at = function.blocks[dispatch]
                                .instructions
                                .iter()
                                .position(|inst| inst.result == raw)
                                .unwrap();
                            let read = function.blocks[dispatch].instructions.remove(at);
                            function.blocks[0].instructions.push(read);
                            let test = ValueId(function.values.len());
                            function.values.push(super::super::super::ir::Value {
                                ty: super::super::super::hir::Type::Bool,
                                span: 0..0,
                            });
                            function.blocks[0].instructions.push(
                                super::super::super::ir::Instruction {
                                    result: test,
                                    operation: Operation::Literal(
                                        super::super::super::hir::Literal::Bool(false),
                                    ),
                                    span: 0..0,
                                },
                            );
                            let old = function.blocks[0].terminator.clone();
                            assert!(matches!(old, Terminator::Jump { .. }));
                            let trampoline = function.blocks.len();
                            function.blocks.push(super::super::super::ir::Block {
                                parameters: Vec::new(),
                                instructions: Vec::new(),
                                terminator: old,
                            });
                            function.blocks[0].terminator = Terminator::Branch {
                                condition: test,
                                consequent: trampoline,
                                alternative: raw_block,
                            };
                        }
                        return;
                    }
                    if mutation == 1 {
                        let branch = function.blocks.iter_mut().find(|block| matches!(block.terminator, Terminator::Branch { condition: test, .. } if test == condition)).unwrap();
                        let Terminator::Branch {
                            consequent,
                            alternative,
                            ..
                        } = &mut branch.terminator
                        else {
                            unreachable!()
                        };
                        std::mem::swap(consequent, alternative);
                    } else {
                        let instruction = function
                            .blocks
                            .iter_mut()
                            .flat_map(|block| &mut block.instructions)
                            .find(|inst| inst.result == condition)
                            .unwrap();
                        instruction.operation =
                            Operation::Literal(super::super::super::hir::Literal::Bool(false));
                    }
                }));
                super::super::super::ir::verify(&changed).unwrap();
                assert!(
                    verify(super::super::SOURCE, &changed, phase)
                        .unwrap_err()
                        .message
                        .contains("wrong live binding")
                );
            }

            fn replace(function: &mut Function, span: &Range<usize>, operation: Operation) -> bool {
                for block in &mut function.blocks {
                    for inst in &mut block.instructions {
                        if inst.span == *span
                            && matches!(
                                inst.operation,
                                Operation::Nominal {
                                    operation: Nominal::FieldSet(_),
                                    ..
                                }
                            )
                        {
                            inst.operation = operation;
                            return true;
                        }
                        match &mut inst.operation {
                            Operation::MakeClosure { body, .. } => {
                                if replace(&mut body.function, span, operation.clone()) {
                                    return true;
                                }
                            }
                            Operation::MakeGeneralClosure { body, .. } => {
                                for method in &mut body.methods {
                                    if replace(&mut method.function, span, operation.clone()) {
                                        return true;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                false
            }
            // Preserve the genuine operands while changing only the destination.
            fn change_field(function: &mut Function, span: &Range<usize>) -> bool {
                for block in &mut function.blocks {
                    for inst in &mut block.instructions {
                        if inst.span == *span {
                            if let Operation::Nominal {
                                operation: Nominal::FieldSet(field),
                                ..
                            } = &mut inst.operation
                            {
                                *field += 1;
                                return true;
                            }
                        }
                        match &mut inst.operation {
                            Operation::MakeClosure { body, .. } => {
                                if change_field(&mut body.function, span) {
                                    return true;
                                }
                            }
                            Operation::MakeGeneralClosure { body, .. } => {
                                for method in &mut body.methods {
                                    if change_field(&mut method.function, span) {
                                        return true;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                false
            }
            fn corrupt_operand(
                function: &mut Function,
                span: &Range<usize>,
                result: bool,
            ) -> Option<Operation> {
                for block in &mut function.blocks {
                    for inst in &mut block.instructions {
                        if inst.span == *span {
                            let original = inst.operation.clone();
                            if let Operation::Nominal {
                                operation: Nominal::FieldSet(_),
                                arguments,
                            } = &mut inst.operation
                            {
                                arguments[usize::from(result)] = arguments[usize::from(!result)];
                                return Some(original);
                            }
                        }
                        match &mut inst.operation {
                            Operation::MakeClosure { body, .. } => {
                                if let Some(original) =
                                    corrupt_operand(&mut body.function, span, result)
                                {
                                    return Some(original);
                                }
                            }
                            Operation::MakeGeneralClosure { body, .. } => {
                                for method in &mut body.methods {
                                    if let Some(original) =
                                        corrupt_operand(&mut method.function, span, result)
                                    {
                                        return Some(original);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                None
            }
            for (result, message) in [(false, "wrong receiver"), (true, "computed hash result")] {
                let original = corrupt_operand(&mut ir, &site.span, result).unwrap();
                // Both edits remain valid typed SSA; ordinary IR verification alone
                // does not establish the canonical memo site's semantic operands.
                super::super::super::ir::verify(&ir).unwrap();
                assert!(
                    verify(super::super::SOURCE, &ir, phase)
                        .unwrap_err()
                        .message
                        .contains(message)
                );
                assert!(replace(&mut ir, &site.span, original));
            }
            assert!(change_field(&mut ir, &site.span));
            assert!(
                verify(super::super::SOURCE, &ir, phase)
                    .unwrap_err()
                    .message
                    .contains("wrong field")
            );
            assert!(replace(
                &mut ir,
                &site.span,
                Operation::Literal(super::super::super::hir::Literal::Nil)
            ));
            assert!(
                verify(super::super::SOURCE, &ir, phase)
                    .unwrap_err()
                    .message
                    .contains("found 0")
            );
        }
    }

    #[test]
    fn canonical_memo_catalog_rejects_same_count_method_substitution() {
        let source = super::super::SOURCE;
        let site = &catalog(source).unwrap()[0];
        let mut changed = source.to_owned();
        changed.replace_range(
            site.span.clone(),
            "(caching-hash coll hash-unordered-coll __hash)",
        );
        assert!(
            catalog(&changed)
                .unwrap_err()
                .message
                .contains("inventory mismatch")
        );
    }

    #[test]
    fn canonical_hash_memo_sites_include_expanded_tree_methods() {
        let sites = catalog(super::super::SOURCE).unwrap();
        assert_eq!(sites.len(), 25);
        assert!(sites.iter().any(|site| site.owner == "Symbol"));
        assert!(sites.iter().any(|site| site.owner == "Keyword"));
        let direct = sites
            .iter()
            .filter(|site| super::super::SOURCE[site.span.clone()].starts_with("(set!"))
            .count();
        assert_eq!(direct, 5);
    }
}
