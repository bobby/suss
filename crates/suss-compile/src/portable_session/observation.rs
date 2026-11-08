//! A synchronous, Store-scoped observation point for portable harnesses.
use super::*;
use wasmtime::{Caller, HeapType};

const NAMESPACE: &str = "suss.harness.observation";
const NAME: &str = "capture";

impl Session {
    /// Install the fixed Runtime-only harness observer. The callback runs inline
    /// with the guest call and receives the single ABI2 argument as a rooted Val.
    /// It has no Session reference and must not re-enter the Store.
    pub fn install_observation_hook(
        &mut self,
        callback: impl for<'a> Fn(StoreContextMut<'a, ()>, Val) -> wasmtime::Result<u32>
        + Send
        + Sync
        + 'static,
    ) -> Result<(), SessionError> {
        if self.phase != Phase::Runtime {
            return Err(wasmtime::Error::msg("Observation hooks require Runtime phase").into());
        }
        if portable::resolve::locate_source_if_present(NAMESPACE, &self.options.source_paths, 0..0)
            .map_err(SessionError::Compile)?
            .is_some()
            || self.environment.has_namespace(self.phase, NAMESPACE)
        {
            return Err(wasmtime::Error::msg(
                "Observation hook cannot replace an existing namespace",
            )
            .into());
        }
        let symbol = suss_reader::Symbol::namespaced(NAMESPACE, NAME);
        if self.environment.resolve(self.phase, &symbol, 0..0).is_ok() {
            return Err(wasmtime::Error::msg(
                "Observation hook target already has a source binding",
            )
            .into());
        }

        self.budget()?;
        let mut environment = self.environment.clone();
        let cell_identity = environment
            .declare_cell(self.phase, NAMESPACE, NAME)
            .map_err(SessionError::Compile)?;
        let provided = ModuleIdentity::new(self.phase, NAMESPACE).map_err(SessionError::Compile)?;
        let closure_new = self
            .runtime
            .get_func(&mut self.store, "closure-new")
            .ok_or_else(|| wasmtime::Error::msg("Missing ABI2 closure constructor"))?;
        let parameter = closure_new.ty(&self.store).params().nth(1).unwrap();
        let HeapType::ConcreteFunc(invoke_type) = parameter.unwrap_ref().heap_type() else {
            return Err(wasmtime::Error::msg("Unexpected ABI2 invoke type").into());
        };
        // The boxed long layout, for tokens too wide for an i31.
        let long_type = {
            let mut scope = RootScope::new(&mut self.store);
            let long_box = self
                .runtime
                .get_func(&mut scope, "long-box")
                .ok_or_else(|| wasmtime::Error::msg("Missing ABI3 long constructor"))?;
            let mut boxed = [Val::null_any_ref()];
            long_box.call(&mut scope, &[Val::I64(i64::MAX)], &mut boxed)?;
            boxed[0]
                .unwrap_anyref()
                .ok_or_else(|| wasmtime::Error::msg("long-box returned null"))?
                .as_struct(&scope)?
                .ok_or_else(|| wasmtime::Error::msg("long-box returned a non-struct"))?
                .ty(&scope)?
        };
        let factory = Func::try_new(
            &mut self.store,
            invoke_type.clone(),
            move |mut caller: Caller<'_, ()>, params, results| {
                if params.len() != 2 || results.len() != 1 {
                    return Err(wasmtime::Error::msg("Unexpected ABI2 invoke signature"));
                }
                let array = params[1]
                    .unwrap_anyref()
                    .ok_or_else(|| wasmtime::Error::msg("Expected ABI2 argument array"))?
                    .as_array(&caller)?
                    .ok_or_else(|| wasmtime::Error::msg("Expected ABI2 argument array"))?;
                if array.len(&caller)? != 1 {
                    return Err(wasmtime::Error::msg("Observation hook arity mismatch"));
                }
                let value = array.get(&mut caller, 0)?;
                let token = callback(caller.as_context_mut(), value)?;
                // The token is a long: an i31 when it fits, otherwise boxed with
                // the runtime-derived layout, without reentry.
                results[0] = Val::AnyRef(Some(if token < 1 << 30 {
                    wasmtime::AnyRef::from_i31(&mut caller, wasmtime::I31::wrapping_u32(token))
                } else {
                    let allocator = wasmtime::StructRefPre::new(&mut caller, long_type.clone());
                    wasmtime::StructRef::new(&mut caller, &allocator, &[Val::I64(i64::from(token))])?
                        .to_anyref()
                }));
                Ok(())
            },
        )?;

        let mut scope = RootScope::new(&mut self.store);
        let nil = self.runtime.get_func(&mut scope, "nil").unwrap();
        let mut env = [Val::null_any_ref()];
        nil.call(&mut scope, &[], &mut env)?;
        let mut closure = [Val::null_any_ref()];
        closure_new.call(
            &mut scope,
            &[
                env[0].clone(),
                Val::FuncRef(Some(factory)),
                Val::I32(1),
                Val::I32(1),
            ],
            &mut closure,
        )?;
        let mut cell = [Val::null_any_ref()];
        self.runtime
            .get_func(&mut scope, "binding-unbound")
            .unwrap()
            .call(&mut scope, &[], &mut cell)?;
        let cell_type = cell[0]
            .unwrap_anyref()
            .unwrap()
            .as_struct(&scope)?
            .unwrap()
            .ty(&scope)?;
        let global = Global::new(
            &mut scope,
            GlobalType::new(
                ValType::Ref(RefType::new(false, cell_type.into())),
                Mutability::Const,
            ),
            cell[0].clone(),
        )?;
        let mut linker = self.linker.clone();
        linker.define(
            &scope,
            cell_identity.import_module(),
            &cell_identity.import_name(),
            global,
        )?;
        self.runtime
            .get_func(&mut scope, "binding-set")
            .unwrap()
            .call(&mut scope, &[cell[0].clone(), closure[0].clone()], &mut [])?;
        drop(scope);

        self.linker = linker;
        self.cells.insert(cell_identity, global);
        self.environment = environment;
        self.provided.insert(provided);
        Ok(())
    }
}
