//! Original compact accounting for an already cycle-checked recipe DAG.
//!
//! Postorder is child before parent. Repeated reader recipes execute on every
//! arrival; compiler sharing barriers execute once. Duplicate edges are distinct
//! arrivals. Counting does not execute or reorder any guest callback.
pub(super) fn count<E>(
    nodes: usize,
    root: usize,
    postorder: &[usize],
    repeat: &[bool],
    degree: impl Fn(usize) -> usize,
    dependencies: impl Fn(usize) -> Vec<usize>,
    mut charge: impl FnMut(u64) -> Result<(), E>,
) -> Result<Option<u64>, E> {
    count_indexed(nodes, root, postorder, repeat, degree, dependencies,
        |id, _| Ok(id), &mut charge)
}

/// Same occurrence calculation with an invocation-private address mapping.
/// Postorder, degrees and edges continue to contain original recipe IDs.
/// Mapping errors propagate before any corresponding arrival read/write.
pub(super) fn count_indexed<E, C: FnMut(u64) -> Result<(), E>>(
    nodes: usize,
    root: usize,
    postorder: &[usize],
    repeat: &[bool],
    degree: impl Fn(usize) -> usize,
    dependencies: impl Fn(usize) -> Vec<usize>,
    mut slot: impl FnMut(usize, &mut C) -> Result<usize, E>,
    charge: &mut C,
) -> Result<Option<u64>, E> {
    charge(nodes as u64)?;
    let mut arrivals = vec![0u64; nodes];
    arrivals[slot(root, charge)?] = 1;
    let mut total = 0u64;
    for &id in postorder.iter().rev() {
        charge(1)?;
        let local = slot(id, charge)?;
        let executions = if repeat[local] {
            arrivals[local]
        } else {
            u64::from(arrivals[local] != 0)
        };
        let degree = degree(id) as u64;
        // One charge per dependency when copied, one per edge visit, and one
        // when the actual materialization frame finishes. Guest work is extra.
        let Some(work) = degree
            .checked_mul(2)
            .and_then(|n| n.checked_add(1))
            .and_then(|n| n.checked_mul(executions))
            .and_then(|n| total.checked_add(n))
        else {
            return Ok(None);
        };
        total = work;
        charge(degree)?;
        for child in dependencies(id) {
            charge(1)?;
            let child_slot = slot(child, charge)?;
            let Some(count) = arrivals[child_slot].checked_add(executions) else {
                return Ok(None);
            };
            arrivals[child_slot] = count;
        }
    }
    Ok(Some(total))
}

#[cfg(test)]
mod tests {
    use super::count;




    fn compact(edges: &[Vec<usize>], repeat: &[bool]) -> Option<u64> {
        count(
            edges.len(),
            edges.len() - 1,
            &(0..edges.len()).collect::<Vec<_>>(),
            repeat,
            |id| edges[id].len(),
            |id| edges[id].clone(),
            |_| Ok::<_, ()>(()),
        )
        .unwrap()
    }
    fn expanded(edges: &[Vec<usize>], repeat: &[bool]) -> u64 {
        fn visit(id: usize, edges: &[Vec<usize>], repeat: &[bool], cached: &mut [bool]) -> u64 {
            if !repeat[id] && cached[id] {
                return 0;
            }
            let work = (2 * edges[id].len() + 1) as u64
                + edges[id]
                    .iter()
                    .map(|&child| visit(child, edges, repeat, cached))
                    .sum::<u64>();
            cached[id] = true;
            work
        }
        visit(
            edges.len() - 1,
            edges,
            repeat,
            &mut vec![false; edges.len()],
        )
    }
    #[test]
    fn matches_literal_occurrences_for_all_small_dags_and_sharing_barriers() {
        // Every forward DAG with four nodes, every repeat/barrier assignment.
        // Reverse edge order and duplicates independently exercise arrival order.
        for shape in 0..64 {
            let mut edges = vec![vec![]; 4];
            let mut bit = 0;
            for parent in 0..4 {
                for child in 0..parent {
                    if shape & (1 << bit) != 0 {
                        edges[parent].push(child);
                    }
                    bit += 1;
                }
            }
            for mask in 0..16 {
                let repeat = (0..4).map(|i| mask & (1 << i) != 0).collect::<Vec<_>>();
                assert_eq!(compact(&edges, &repeat), Some(expanded(&edges, &repeat)));
                let doubled = edges
                    .iter()
                    .map(|v| v.iter().rev().flat_map(|&id| [id, id]).collect())
                    .collect::<Vec<_>>();
                assert_eq!(
                    compact(&doubled, &repeat),
                    Some(expanded(&doubled, &repeat))
                );
            }
        }
    }
    #[test]
    fn overflow_is_rejection_not_saturation_or_wrapping() {
        let edges = (0..65)
            .map(|i| if i == 0 { vec![] } else { vec![i - 1, i - 1] })
            .collect::<Vec<_>>();
        assert_eq!(compact(&edges, &vec![true; 65]), None);
        assert_eq!(compact(&edges, &vec![false; 65]), Some(321));
    }
    #[test]
    fn exponential_occurrences_require_only_linear_planning_work() {
        let edges = (0..31)
            .map(|i| if i == 0 { vec![] } else { vec![i - 1, i - 1] })
            .collect::<Vec<_>>();
        let mut charged = 0;
        let work = count(
            31,
            30,
            &(0..31).collect::<Vec<_>>(),
            &[true; 31],
            |id| edges[id].len(),
            |id| edges[id].clone(),
            |units| {
                charged += units;
                Ok::<_, ()>(())
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(work, 6 * (1u64 << 30) - 5);
        assert_eq!(charged, 31 + 31 + 2 * 60);
    }
    #[test]
    fn meter_failure_stops_before_dependency_allocation() {
        let result = count(
            1,
            0,
            &[0],
            &[false],
            |_| 3,
            |_| panic!("must charge before copying dependencies"),
            |units| if units == 3 { Err("cancelled") } else { Ok(()) },
        );
        assert_eq!(result, Err("cancelled"));
    }
}
