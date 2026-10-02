//! Original compiler binding facts for compiled macro environments.
//! Retain actual declarations and analyzed initializers; never reconstruct facts
//! by evaluating source or replacing unknown information with a scalar stand-in.
use super::*;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalKind {
    Let,
    Loop,
    Argument { index: usize, rest: bool },
    FunctionName,
    Catch,
}

#[derive(Debug, Clone)]
pub struct LocalBinding {
    /// Current lowered binding used for lexical lookup, including loop remapping.
    pub id: BindingId,
    pub ty: Type,
    /// Original reader declaration, preserving its span and metadata.
    pub declaration: Form,
    pub kind: LocalKind,
    /// Actual analyzed initializer; absent for parameters/self/catch bindings.
    pub initializer: Option<Arc<Hir>>,
    /// Previous lexical declaration, excluding compiler-only ID remapping.
    pub shadow: Option<Arc<LocalBinding>>,
}
impl Analyzer<'_> {
    pub(super) fn insert_local(
        &mut self,
        declaration: &Form,
        id: BindingId,
        ty: Type,
        kind: LocalKind,
        initializer: Option<Hir>,
    ) -> Option<LocalBinding> {
        let Kind::Symbol(name) = &declaration.kind else {
            unreachable!("validated declaration")
        };
        let shadow = self.locals.get(&name.name).cloned().map(Arc::new);
        self.locals.insert(
            name.name.clone(),
            LocalBinding {
                id,
                ty,
                declaration: declaration.clone(),
                kind,
                initializer: initializer.map(Arc::new),
                shadow,
            },
        )
    }
    pub(super) fn remap_local(&mut self, name: &str, id: BindingId, ty: Type) {
        let record = self
            .locals
            .get_mut(name)
            .expect("existing parameter binding");
        record.id = id;
        record.ty = ty;
    }
}
