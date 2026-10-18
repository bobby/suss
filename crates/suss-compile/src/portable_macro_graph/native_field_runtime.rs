//! Private binding of compiled native plans to the actual shipped source owners.
//! No guest values, source certificates or memo receipts live in this registry.
use super::{
    generated_native_fields,
    raw_keys::{
        KeyId,
        compiled_field_codegen::{NativeField, NativePhase},
        compiled_prefix::{BuildError, Prefix},
    },
};
use crate::{
    portable::{
        bootstrap,
        resolve::{DefinitionInfo, Global, Phase},
    },
    portable_session::{Session, SessionError},
};
use std::sync::{Arc, Mutex};
use suss_reader::forms::Form;
type Result<T> = std::result::Result<T, SessionError>;
fn invalid(message: &str) -> SessionError {
    wasmtime::Error::msg(format!("Invalid compiled native field tables: {message}")).into()
}
fn pay(session: &mut Session, units: usize) -> Result<()> {
    session.charge_host_work(u64::try_from(units).map_err(|_| invalid("work extent"))?)
}
fn reserve<T>(values: &mut Vec<T>, extra: usize, session: &mut Session) -> Result<()> {
    if extra > values.capacity() - values.len() {
        let capacity = values
            .len()
            .checked_add(extra)
            .ok_or_else(|| invalid("capacity"))?;
        let bytes = capacity
            .checked_add(values.len())
            .and_then(|n| n.checked_mul(std::mem::size_of::<T>()))
            .ok_or_else(|| invalid("allocation"))?;
        pay(session, bytes)?;
        values
            .try_reserve_exact(capacity - values.len())
            .map_err(|_| invalid("allocation failed"))?;
    }
    Ok(())
}
fn equal(session: &mut Session, a: &str, b: &str) -> Result<bool> {
    pay(session, 1)?;
    if a.len() != b.len() {
        return Ok(false);
    }
    for (a, b) in a.as_bytes().chunks(256).zip(b.as_bytes().chunks(256)) {
        pay(
            session,
            a.len()
                .checked_add(b.len())
                .ok_or_else(|| invalid("text work"))?,
        )?;
        if a != b {
            return Ok(false);
        }
    }
    Ok(true)
}
#[derive(Clone, Copy)]
pub(super) enum Selector {
    Definition,
    Declaration,
    Initializer,
}
impl Selector {
    fn index(self) -> usize {
        match self {
            Self::Definition => 0,
            Self::Declaration => 1,
            Self::Initializer => 2,
        }
    }
}
fn source(info: &DefinitionInfo, selector: Selector) -> Option<&Form> {
    match selector {
        Selector::Definition => Some(&info.definition_form),
        Selector::Declaration => Some(&info.declaration),
        Selector::Initializer => info.initializer_form.as_ref(),
    }
}
struct Binding {
    global: Global,
    owner: Arc<DefinitionInfo>,
    fields: [Option<usize>; 3],
}
pub(crate) struct Registry {
    phase: Phase,
    data: &'static NativePhase,
    pub(super) prefix: Arc<Prefix>,
    bindings: Vec<Binding>,
    by_address: Vec<(usize, usize)>,
}
static RUNTIME: Mutex<Option<Arc<Registry>>> = Mutex::new(None);
static MACRO: Mutex<Option<Arc<Registry>>> = Mutex::new(None);
fn sort_addresses(values: &mut Vec<(usize, usize)>, session: &mut Session) -> Result<()> {
    let n = values.len();
    let mut scratch = Vec::new();
    reserve(&mut scratch, n, session)?;
    pay(session, n)?; // prepay infallible scratch retirement before filling
    let mut width = 1usize;
    while width < n {
        scratch.clear();
        let mut start = 0usize;
        while start < n {
            pay(session, 3)?;
            let middle = start.saturating_add(width).min(n);
            let end = middle.saturating_add(width).min(n);
            let (mut a, mut b) = (start, middle);
            while a < middle || b < end {
                pay(session, 4)?;
                let next = if b == end || (a < middle && values[a] <= values[b]) {
                    let i = a;
                    a += 1;
                    i
                } else {
                    let i = b;
                    b += 1;
                    i
                };
                scratch.push(values[next]);
            }
            start = end;
        }
        std::mem::swap(values, &mut scratch);
        width = width.saturating_mul(2);
    }
    Ok(())
}
/// This does NOT create a missing bootstrap/catalog. No new analysis is hidden
/// in a lookup, and a public restored Environment cannot mint this admission.
/// All new work is debited to the caller's current operation, including startup.
pub(crate) fn initialize(phase: Phase, session: &mut Session) -> Result<Option<Arc<Registry>>> {
    pay(session, 2)?;
    let Some((env, source_sha, dependency_sha)) = bootstrap::loaded_native_source(phase) else {
        return Ok(None);
    };
    let cache = match phase {
        Phase::Runtime => &RUNTIME,
        Phase::Macro => &MACRO,
    };
    // Lock only for lookup/publication. Metering and immutable staging run
    // outside it so another Session never waits through the full table scan.
    pay(session, 2)?;
    if let Some(value) = cache
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .as_ref()
        .cloned()
    {
        return Ok(Some(value));
    }
    let data = match phase {
        Phase::Runtime => &generated_native_fields::RUNTIME,
        Phase::Macro => &generated_native_fields::MACRO,
    };
    if data.phase != phase
        || !equal(session, data.source_sha256, source_sha)?
        || !equal(session, data.dependency_sha256, dependency_sha)?
    {
        return Err(invalid(
            "source/dependency/phase identity; regenerate native fields and bootstrap",
        ));
    }
    let tracing = std::env::var_os("SUSS_METADATA_FUEL_TRACE").is_some();
    if tracing {
        eprintln!(
            "compiled-native-setup phase={phase:?} event=start remaining={:?}",
            session.operation_fuel_remaining().ok()
        );
    }
    let prefix =
        Prefix::build(data, &mut |u| session.charge_host_work(u)).map_err(|e| match e {
            BuildError::Meter(e) => e,
            BuildError::Invalid(s) => invalid(s),
        })?;
    let mut bindings = Vec::new();
    let mut by_address = Vec::new();
    reserve(&mut bindings, data.globals.len(), session)?;
    reserve(&mut by_address, data.globals.len(), session)?;
    let mut ordinal = 0usize;
    for (global, owner) in env.shared_definition_entries() {
        pay(session, 3)?;
        if global.phase() != phase || !equal(session, global.namespace(), "suss.core")? {
            continue;
        }
        let this_ordinal = ordinal;
        ordinal = ordinal
            .checked_add(1)
            .ok_or_else(|| invalid("owner ordinal"))?;
        // Incomplete/historical/new definitions retain the complete old planner;
        // omitting cache eligibility NEVER omits them from the exported graph.
        if !owner.analysis_completed {
            continue;
        }
        let index = bindings.len();
        let known = data
            .globals
            .get(index)
            .ok_or_else(|| invalid("missing compiled owner"))?;
        pay(session, 6)?;
        if known.ordinal != this_ordinal
            || !equal(session, global.namespace(), known.namespace)?
            || !equal(session, global.name(), known.name)?
        {
            return Err(invalid("actual private owner correspondence"));
        }
        let presence = if owner.initializer_form.is_some() {
            7
        } else {
            3
        };
        if known.presence != presence {
            return Err(invalid("field presence"));
        }
        let end = known
            .fields_start
            .checked_add(known.fields_len)
            .ok_or_else(|| invalid("field range"))?;
        let records = data
            .fields
            .get(known.fields_start..end)
            .ok_or_else(|| invalid("field range"))?;
        let mut fields = [None; 3];
        for (offset, field) in records.iter().enumerate() {
            pay(session, 8)?;
            if field.owner != index
                || field.selector >= 3
                || fields[field.selector].is_some()
                || presence & (1 << field.selector) == 0
                || field.root >= field.nodes_len
            {
                return Err(invalid("field descriptor"));
            }
            let end = field
                .nodes_start
                .checked_add(field.nodes_len)
                .ok_or_else(|| invalid("node range"))?;
            if data.nodes.get(field.nodes_start..end).is_none() {
                return Err(invalid("node range"));
            }
            fields[field.selector] = Some(known.fields_start + offset);
        }
        if fields[0].is_none()
            || fields[1].is_none()
            || fields[2].is_some() != owner.initializer_form.is_some()
        {
            return Err(invalid("missing field"));
        }
        // Prepay acquisition plus infallible retirement of both retained Arcs.
        // Their actual source owners are still held by the private bootstrap.
        pay(session, 11)?;
        bindings.push(Binding {
            global: global.clone(),
            owner: owner.clone(),
            fields,
        });
        by_address.push((Arc::as_ptr(owner) as usize, index));
    }
    if bindings.len() != data.globals.len() {
        return Err(invalid("extra compiled owner"));
    }
    sort_addresses(&mut by_address, session)?;
    // Validate LOCAL occurrence domains once for the compiled image. Forward
    // edges plus exactly one incoming edge prove closure and exclude cycles.
    let mut incoming = Vec::<u8>::new();
    for field in data.fields {
        pay(session, 4)?;
        if field.root != 0 || field.nodes_len == 0 {
            return Err(invalid("field root"));
        }
        incoming.clear();
        reserve(&mut incoming, field.nodes_len, session)?;
        pay(
            session,
            field
                .nodes_len
                .checked_mul(2)
                .ok_or_else(|| invalid("scratch lifecycle"))?,
        )?; // fill and prepay retirement
        incoming.resize(field.nodes_len, 0);
        let end = field
            .nodes_start
            .checked_add(field.nodes_len)
            .ok_or_else(|| invalid("node range"))?;
        let nodes = data
            .nodes
            .get(field.nodes_start..end)
            .ok_or_else(|| invalid("node range"))?;
        for (index, node) in nodes.iter().enumerate() {
            pay(session, 6)?;
            if node.key >= data.keys.len() || node.metadata_count > node.children_len {
                return Err(invalid("node key/metadata"));
            }
            let end = node
                .children_start
                .checked_add(node.children_len)
                .ok_or_else(|| invalid("children range"))?;
            let children = data
                .children
                .get(node.children_start..end)
                .ok_or_else(|| invalid("children range"))?;
            for child in children {
                pay(session, 4)?;
                if *child <= index || *child >= field.nodes_len || incoming[*child] != 0 {
                    return Err(invalid("closed local child domain"));
                }
                incoming[*child] = 1;
            }
        }
        for seen in &incoming[1..] {
            pay(session, 1)?;
            if *seen != 1 {
                return Err(invalid("disconnected native row"));
            }
        }
    }
    drop(incoming);
    pay(
        session,
        std::mem::size_of::<Registry>() + 2 * std::mem::size_of::<usize>() + 3,
    )?;
    let value = Arc::new(Registry {
        phase,
        data,
        prefix,
        bindings,
        by_address,
    });
    // All fallible setup precedes registry publication; failures remain retryable.
    pay(session, 4)?; // publication retain and possible race-loser retirement
    let mut slot = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some(existing) = slot.as_ref() {
        return Ok(Some(existing.clone()));
    }
    *slot = Some(value.clone());
    if tracing {
        eprintln!(
            "compiled-native-setup phase={phase:?} event=published remaining={:?}",
            session.operation_fuel_remaining().ok()
        );
    }
    Ok(Some(value))
}
#[derive(Clone)]
pub(super) struct NativePlan {
    registry: Arc<Registry>,
    field: usize,
    source_address: usize,
}
impl Registry {
    pub(super) fn lookup(
        self: &Arc<Self>,
        session: &mut Session,
        global: &Global,
        owner: &Arc<DefinitionInfo>,
        selector: Selector,
        form: &Form,
    ) -> Result<Option<NativePlan>> {
        pay(session, 4)?;
        if global.phase() != self.phase
            || !source(owner, selector).is_some_and(|actual| std::ptr::eq(actual, form))
        {
            return Ok(None);
        }
        let wanted = Arc::as_ptr(owner) as usize;
        let (mut low, mut high) = (0, self.by_address.len());
        while low < high {
            pay(session, 4)?;
            let middle = low + (high - low) / 2;
            if self.by_address[middle].0 < wanted {
                low = middle + 1
            } else {
                high = middle
            }
        }
        while let Some(&(address, index)) = self.by_address.get(low) {
            pay(session, 5)?;
            if address != wanted {
                break;
            }
            let binding = &self.bindings[index];
            if Arc::ptr_eq(owner, &binding.owner)
                && equal(session, global.namespace(), binding.global.namespace())?
                && equal(session, global.name(), binding.global.name())?
            {
                if let Some(field) = binding.fields[selector.index()] {
                    pay(session, 2)?;
                    return Ok(Some(NativePlan {
                        registry: self.clone(),
                        field,
                        source_address: form as *const Form as usize,
                    }));
                }
            }
            low += 1;
        }
        Ok(None)
    }
}
impl NativePlan {
    fn descriptor(&self) -> &'static NativeField {
        &self.registry.data.fields[self.field]
    }
    pub(super) fn root(
        &self,
        form: &Form,
        raw: &super::raw_keys::compiled_prefix::GraphInterner,
        session: &mut Session,
    ) -> Result<usize> {
        pay(session, 5)?;
        if self.source_address != form as *const Form as usize
            || !raw.has_prefix(&self.registry.prefix)
        {
            return Err(invalid("native source/prefix consumption domain"));
        }
        Ok(self.descriptor().root)
    }
    pub(super) fn len(&self) -> usize {
        self.descriptor().nodes_len
    }
    pub(super) fn node(
        &self,
        index: usize,
        session: &mut Session,
    ) -> Result<(KeyId, usize, &'static [usize])> {
        pay(session, 9)?;
        let field = self.descriptor();
        if index >= field.nodes_len {
            return Err(invalid("local plan node"));
        }
        let global = field
            .nodes_start
            .checked_add(index)
            .ok_or_else(|| invalid("node index"))?;
        let node = self
            .registry
            .data
            .nodes
            .get(global)
            .ok_or_else(|| invalid("node index"))?;
        let end = node
            .children_start
            .checked_add(node.children_len)
            .ok_or_else(|| invalid("children extent"))?;
        let children = self
            .registry
            .data
            .children
            .get(node.children_start..end)
            .ok_or_else(|| invalid("children extent"))?;
        if node.key >= self.registry.data.keys.len() || node.metadata_count > children.len() {
            return Err(invalid("node key/metadata"));
        }
        // initialize() checked every immutable local child domain. Access is
        // still bounds checked; callers stream or select children in O(1).
        Ok((KeyId(node.key), node.metadata_count, children))
    }
}

#[cfg(test)]
#[path = "native_field_runtime_tests.rs"]
mod tests;
