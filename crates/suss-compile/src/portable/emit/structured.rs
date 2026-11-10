//! Structured control flow for a reducible CFG (#358), after Ramsey's "Beyond
//! Relooper" (ICFP 2022). A loop header's dominator subtree sits in a `loop`;
//! a block with several forward predecessors follows the `block` that wraps
//! its dominator parent, so a branch to it is a `br` out of that `block`.
//! Every other target is the only forward successor of its dominator parent
//! and is emitted in place. Irreducible graphs return `None` and keep the
//! block-index dispatcher.
use super::super::ir::{Function, Terminator};

/// One emission step, in order. `Code` emits a block's instructions and its
/// terminator's prefix: the parameter assignment of a jump, or the falsey
/// test and `if` of a branch, whose arms follow as `Else`-separated steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    Code(usize),
    Loop,
    Block,
    Else,
    End,
    Br(u32),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Label {
    Loop(usize),
    Follow(usize),
    If,
}

fn successors(terminator: &Terminator) -> Vec<usize> {
    match terminator {
        Terminator::Jump { target, .. } => vec![*target],
        Terminator::Branch { consequent, alternative, .. } => vec![*alternative, *consequent],
        Terminator::Return(_) | Terminator::Throw(_) => Vec::new(),
    }
}

struct Plan {
    rpo: Vec<Option<usize>>,
    children: Vec<Vec<usize>>,
    header: Vec<bool>,
    merge: Vec<bool>,
    steps: Vec<Step>,
}

pub(super) fn plan(function: &Function) -> Option<Vec<Step>> {
    let count = function.blocks.len();
    if count == 0 { return None; }
    let successors: Vec<Vec<usize>> = function.blocks.iter()
        .map(|block| successors(&block.terminator)).collect();
    // Iterative postorder from the entry; unreachable blocks are never emitted.
    let mut postorder = Vec::with_capacity(count);
    let mut seen = vec![false; count];
    let mut stack = vec![(0, 0)];
    seen[0] = true;
    while let Some((block, next)) = stack.pop() {
        if let Some(&target) = successors[block].get(next) {
            stack.push((block, next + 1));
            if !seen[target] {
                seen[target] = true;
                stack.push((target, 0));
            }
        } else {
            postorder.push(block);
        }
    }
    let order: Vec<usize> = postorder.into_iter().rev().collect();
    let mut rpo = vec![None; count];
    for (position, block) in order.iter().enumerate() { rpo[*block] = Some(position); }
    let mut predecessors = vec![Vec::new(); count];
    for &block in &order {
        for &target in &successors[block] { predecessors[target].push(block); }
    }
    // Cooper, Harvey and Kennedy's iterative dominators, over RPO positions.
    let mut idom: Vec<Option<usize>> = vec![None; order.len()];
    idom[0] = Some(0);
    let intersect = |idom: &[Option<usize>], mut a: usize, mut b: usize| {
        while a != b {
            while a > b { a = idom[a].expect("processed"); }
            while b > a { b = idom[b].expect("processed"); }
        }
        a
    };
    let mut changed = true;
    while changed {
        changed = false;
        for position in 1..order.len() {
            let mut new = None;
            for &predecessor in &predecessors[order[position]] {
                let predecessor = rpo[predecessor].expect("reachable");
                if idom[predecessor].is_none() { continue; }
                new = Some(new.map_or(predecessor, |current| intersect(&idom, predecessor, current)));
            }
            if new != idom[position] {
                idom[position] = new;
                changed = true;
            }
        }
    }
    let dominates = |a: usize, mut b: usize| loop {
        if a == b { return true; }
        if b == 0 { return false; }
        b = idom[b].expect("reachable");
    };
    let mut header = vec![false; count];
    let mut forward = vec![0usize; count];
    for &block in &order {
        let source = rpo[block].expect("reachable");
        for &target in &successors[block] {
            let position = rpo[target].expect("reachable");
            if position <= source {
                // A retreating edge to a non-dominator is irreducible.
                if !dominates(position, source) { return None; }
                header[target] = true;
            } else {
                forward[target] += 1;
            }
        }
    }
    let mut children = vec![Vec::new(); count];
    for position in 1..order.len() {
        children[order[idom[position].expect("reachable")]].push(order[position]);
    }
    let mut plan = Plan {
        rpo, children, header,
        merge: forward.iter().map(|edges| *edges > 1).collect(),
        steps: Vec::new(),
    };
    let mut context = Vec::new();
    plan.tree(function, 0, &mut context, &successors);
    debug_assert!(context.is_empty());
    Some(plan.steps)
}

impl Plan {
    fn tree(&mut self, function: &Function, block: usize, context: &mut Vec<Label>, successors: &[Vec<usize>]) {
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
            // Later merge children enclose earlier ones, so each `block` ends
            // just before the subtree that follows it.
            let mut merges: Vec<usize> = self.children[block].iter().copied()
                .filter(|child| self.merge[*child]).collect();
            merges.sort_by_key(|child| std::cmp::Reverse(self.rpo[*child]));
            if self.header[block] {
                self.steps.push(Step::Loop);
                context.push(Label::Loop(block));
            }
            self.within(function, block, &merges, context, successors);
            if self.header[block] {
                context.pop();
                self.steps.push(Step::End);
            }
        });
    }

    fn within(&mut self, function: &Function, block: usize, merges: &[usize],
        context: &mut Vec<Label>, successors: &[Vec<usize>]) {
        if let Some((&follow, rest)) = merges.split_first() {
            self.steps.push(Step::Block);
            context.push(Label::Follow(follow));
            self.within(function, block, rest, context, successors);
            context.pop();
            self.steps.push(Step::End);
            self.tree(function, follow, context, successors);
            return;
        }
        self.steps.push(Step::Code(block));
        match &function.blocks[block].terminator {
            Terminator::Jump { target, .. } => self.branch(function, block, *target, context, successors),
            Terminator::Branch { consequent, alternative, .. } => {
                // The test leaves 1 for a falsey condition.
                context.push(Label::If);
                self.branch(function, block, *alternative, context, successors);
                self.steps.push(Step::Else);
                self.branch(function, block, *consequent, context, successors);
                context.pop();
                self.steps.push(Step::End);
            }
            Terminator::Return(_) | Terminator::Throw(_) => {}
        }
    }

    fn branch(&mut self, function: &Function, source: usize, target: usize,
        context: &mut Vec<Label>, successors: &[Vec<usize>]) {
        let label = if self.rpo[target] <= self.rpo[source] {
            Label::Loop(target)
        } else if self.merge[target] {
            Label::Follow(target)
        } else {
            return self.tree(function, target, context, successors);
        };
        let depth = context.iter().rev().position(|item| *item == label)
            .expect("reducible branch target is in scope");
        self.steps.push(Step::Br(depth as u32));
    }
}

#[cfg(test)]
mod tests {
    use super::{plan, Step::{self, *}};
    use crate::portable::ir::{Block, Function, Terminator, ValueId};

    fn function(terminators: Vec<Terminator>) -> Function {
        Function {
            values: Vec::new(),
            span: 0..0,
            blocks: terminators.into_iter()
                .map(|terminator| Block { parameters: Vec::new(), instructions: Vec::new(), terminator })
                .collect(),
        }
    }
    fn jump(target: usize) -> Terminator { Terminator::Jump { target, arguments: Vec::new() } }
    fn branch(consequent: usize, alternative: usize) -> Terminator {
        Terminator::Branch { condition: ValueId(0), consequent, alternative }
    }
    fn ret() -> Terminator { Terminator::Return(ValueId(0)) }

    #[test]
    fn a_loop_recurs_to_its_header_and_returns_in_place() {
        // (loop [i 0] (if (< i n) (recur (inc i)) i))
        let steps = plan(&function(vec![jump(1), branch(2, 3), jump(1), ret()]));
        let expected: Vec<Step> = vec![Code(0), Loop, Code(1), Code(3), Else, Code(2), Br(1), End, End];
        assert_eq!(steps, Some(expected));
    }

    #[test]
    fn an_if_joins_by_leaving_the_block_before_its_merge() {
        // (let [x (if c 1 2)] x)
        let steps = plan(&function(vec![branch(1, 2), jump(3), jump(3), ret()]));
        let expected: Vec<Step> = vec![Block, Code(0), Code(2), Br(1), Else, Code(1), Br(1), End, End, Code(3)];
        assert_eq!(steps, Some(expected));
    }

    #[test]
    fn an_irreducible_graph_keeps_the_dispatcher_and_unreachable_blocks_are_dropped() {
        // Two loop entries: 0 branches to 1 and 2, which jump to each other.
        assert_eq!(plan(&function(vec![branch(1, 2), jump(2), jump(1)])), None);
        assert_eq!(plan(&function(vec![ret(), jump(0)])), Some(vec![Code(0)]));
    }
}
