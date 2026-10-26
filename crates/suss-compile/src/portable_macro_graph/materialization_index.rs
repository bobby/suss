//! Invocation-private scratch addressing. Keys are native recipe IDs, never
//! guest references. Membership is built before callbacks and then read only.

pub(super) trait Index {
    const DENSE: bool;
    fn states<E>(&self, nodes: usize, charge: &mut impl FnMut(u64) -> Result<(), E>)
        -> Result<(Vec<u8>, Vec<bool>), E>;
    fn postorder<E>(&self, nodes: usize, charge: &mut impl FnMut(u64) -> Result<(), E>)
        -> Result<Vec<usize>, E>;
    fn enter<E>(&mut self, id: usize, states: &mut Vec<u8>, repeat: &mut Vec<bool>,
        charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<usize, E>;
    fn slot<E>(&self, id: usize, charge: &mut impl FnMut(u64) -> Result<(), E>)
        -> Result<usize, E>;
    fn push_postorder<E>(&self, values: &mut Vec<usize>, id: usize,
        charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<(), E>;
    fn any_repeat<E>(&self, ids: &[usize], repeat: &[bool],
        charge: &mut impl FnMut(u64) -> Result<(), E>) -> Result<bool, E> {
        for &id in ids {
            if repeat[self.slot(id, charge)?] { return Ok(true); }
        }
        Ok(false)
    }
}

pub(super) struct Dense;
impl Index for Dense {
    const DENSE: bool = true;
    fn states<E>(&self, nodes: usize, charge: &mut impl FnMut(u64) -> Result<(), E>)
        -> Result<(Vec<u8>, Vec<bool>), E> {
        charge(nodes.saturating_mul(3) as u64)?;
        Ok((vec![0; nodes], vec![false; nodes]))
    }
    fn postorder<E>(&self, nodes: usize, charge: &mut impl FnMut(u64) -> Result<(), E>)
        -> Result<Vec<usize>, E> {
        charge(nodes as u64)?;
        Ok(Vec::with_capacity(nodes))
    }
    fn enter<E>(&mut self, id: usize, _: &mut Vec<u8>, _: &mut Vec<bool>,
        _: &mut impl FnMut(u64) -> Result<(), E>) -> Result<usize, E> { Ok(id) }
    fn slot<E>(&self, id: usize, _: &mut impl FnMut(u64) -> Result<(), E>)
        -> Result<usize, E> { Ok(id) }
    fn push_postorder<E>(&self, values: &mut Vec<usize>, id: usize,
        _: &mut impl FnMut(u64) -> Result<(), E>) -> Result<(), E> {
        values.push(id);
        Ok(())
    }
}
