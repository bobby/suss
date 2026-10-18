//! The original mutable Plan path and a native-plan overlay use one recipe walk.
//! IDs keep native LOCAL rows separate from generated-metadata scratch rows.
use super::{
    native_field_runtime::NativePlan,
    raw_keys::{KeyId, Plan, compiled_prefix::GraphInterner},
};
use crate::portable_session::{Session, SessionError};
use suss_reader::forms::Form;
type Result<T> = std::result::Result<T, SessionError>;
fn invalid() -> SessionError {
    wasmtime::Error::msg("Invalid native/generated reader-plan index").into()
}
pub(super) trait Plans {
    type Id: Copy;
    fn root(
        &mut self,
        form: &Form,
        raw: &mut GraphInterner,
        session: &mut Session,
    ) -> Result<Self::Id>;
    fn generated(
        &mut self,
        form: &Form,
        raw: &mut GraphInterner,
        session: &mut Session,
    ) -> Result<Self::Id>;
    fn key(&self, id: Self::Id, session: &mut Session) -> Result<KeyId>;
    fn metadata(&self, id: Self::Id, session: &mut Session) -> Result<usize>;
    fn selected_key(
        &self,
        id: Self::Id,
        index: usize,
        session: &mut Session,
    ) -> Result<Option<KeyId>>;
    fn child(&self, id: Self::Id, ordinal: usize, session: &mut Session) -> Result<Self::Id>;
}
pub(super) struct Legacy<'a> {
    pub(super) plan: &'a mut Plan,
}
impl Plans for Legacy<'_> {
    type Id = usize;
    fn root(
        &mut self,
        form: &Form,
        raw: &mut GraphInterner,
        session: &mut Session,
    ) -> Result<usize> {
        raw.plan(form, self.plan, &mut |u| session.charge_host_work(u))
    }
    fn generated(
        &mut self,
        form: &Form,
        raw: &mut GraphInterner,
        session: &mut Session,
    ) -> Result<usize> {
        self.root(form, raw, session)
    }
    fn key(&self, id: usize, _: &mut Session) -> Result<KeyId> {
        Ok(self.plan.nodes.get(id).ok_or_else(invalid)?.key)
    }
    fn metadata(&self, id: usize, _: &mut Session) -> Result<usize> {
        Ok(self.plan.nodes.get(id).ok_or_else(invalid)?.metadata_count)
    }
    fn selected_key(&self, id: usize, index: usize, _: &mut Session) -> Result<Option<KeyId>> {
        let node = self.plan.nodes.get(id).ok_or_else(invalid)?;
        Ok(node
            .children
            .get(node.metadata_count + index)
            .map(|child| self.plan.nodes[*child].key))
    }
    fn child(&self, id: usize, ordinal: usize, _: &mut Session) -> Result<usize> {
        self.plan
            .nodes
            .get(id)
            .and_then(|n| n.children.get(ordinal))
            .copied()
            .ok_or_else(invalid)
    }
}
#[derive(Clone, Copy)]
pub(super) enum Index {
    Native(usize),
    Generated(usize),
}
pub(super) struct Native<'a> {
    pub(super) plan: NativePlan,
    pub(super) generated: Legacy<'a>,
}
impl Plans for Native<'_> {
    type Id = Index;
    fn root(
        &mut self,
        form: &Form,
        raw: &mut GraphInterner,
        session: &mut Session,
    ) -> Result<Index> {
        self.plan.root(form, raw, session).map(Index::Native)
    }
    fn generated(
        &mut self,
        form: &Form,
        raw: &mut GraphInterner,
        session: &mut Session,
    ) -> Result<Index> {
        self.generated
            .root(form, raw, session)
            .map(Index::Generated)
    }
    fn key(&self, id: Index, session: &mut Session) -> Result<KeyId> {
        session.charge_host_work(1)?;
        match id {
            Index::Native(id) => self.plan.node(id, session).map(|n| n.0),
            Index::Generated(id) => self.generated.key(id, session),
        }
    }
    fn metadata(&self, id: Index, session: &mut Session) -> Result<usize> {
        session.charge_host_work(1)?;
        match id {
            Index::Native(id) => self.plan.node(id, session).map(|n| n.1),
            Index::Generated(id) => self.generated.metadata(id, session),
        }
    }
    fn selected_key(
        &self,
        id: Index,
        index: usize,
        session: &mut Session,
    ) -> Result<Option<KeyId>> {
        session.charge_host_work(2)?;
        match id {
            Index::Native(id) => {
                let (_, metadata, children) = self.plan.node(id, session)?;
                let ordinal = metadata.checked_add(index).ok_or_else(invalid)?;
                children
                    .get(ordinal)
                    .map(|child| self.plan.node(*child, session).map(|n| n.0))
                    .transpose()
            }
            Index::Generated(id) => self.generated.selected_key(id, index, session),
        }
    }
    fn child(&self, id: Index, ordinal: usize, session: &mut Session) -> Result<Index> {
        session.charge_host_work(2)?;
        match id {
            Index::Native(id) => self
                .plan
                .node(id, session)?
                .2
                .get(ordinal)
                .copied()
                .map(Index::Native)
                .ok_or_else(invalid),
            Index::Generated(id) => self
                .generated
                .child(id, ordinal, session)
                .map(Index::Generated),
        }
    }
}
