use super::super::construction_records::ConstructionRecords;
use super::*;
use crate::{portable::hir::Literal, portable_macro_data::FormBridge};

fn maps(session: &mut Session, count: usize) -> (Vec<SessionValue>, ConstructionRecords) {
    let bridge = FormBridge::new(session)
        .unwrap()
        .recording(session)
        .unwrap();
    let key = bridge.identifier(session, None, "x", true, None).unwrap();
    let number = bridge.scalar(session, &Literal::Number(17.0)).unwrap();
    let mut maps = Vec::new();
    for _ in 0..count {
        maps.push(
            bridge
                .map_values(session, &[(key.clone(), number.clone())])
                .unwrap(),
        );
    }
    (maps, bridge.into_records().unwrap())
}
fn replace(session: &mut Session, body: &str) {
    session
        .eval(&format!(
            "(extend-type PersistentArrayMap IHash (-hash [self] {body}))"
        ))
        .unwrap();
}
fn run(session: &mut Session, value: &SessionValue) -> Result<SessionValue, SessionError> {
    let function = session.eval("(fn [x] (suss.core/-hash x))").unwrap();
    session.invoke(&function, &[value])
}
fn check(session: &mut Session, records: &ConstructionRecords, expected: bool) {
    assert_eq!(records.matches_direct_facts(session).unwrap(), expected);
    let observer = session.dispatch_observer.as_ref().unwrap();
    assert!(observer.receiver(session.store.as_context_mut()).is_none());
}
fn display(session: &mut Session, value: &SessionValue) -> String {
    crate::portable_repl::display(session, value).unwrap()
}
#[test]
fn scoped_memo_stores_cold_named_field_helpers_and_shared_siblings() {
    let mut session = Session::new_macro().unwrap();
    for mode in 0..3 {
        let (values, records) = maps(&mut session, 2);
        match mode {
            0 => replace(&mut session, "(do (set! (.-__hash self) 91) 91)"),
            1 => {
                session
                    .eval("(def scoped-write (fn [x] (set! (.-__hash x) 91)))")
                    .unwrap();
                replace(&mut session, "(do (scoped-write self) 91)");
            }
            _ => {
                session.eval("(extend-type PersistentArrayMap ICounted (-count [self] (do (set! (.-__hash self) 91) 3)))").unwrap();
                replace(&mut session, "(do (count self) 91)");
            }
        }
        let result = run(&mut session, &values[0]).unwrap();
        assert_eq!(display(&mut session, &result), "91");
        check(&mut session, &records, true);
    }
}
#[test]
fn scoped_memo_stores_payload_health_at_write_and_after_restore() {
    let mut session = Session::new_macro().unwrap();
    for (body, expected) in [
        (
            "(do (set! (.-__hash self) 91) (aset (.-arr self) 1 99) (aset (.-arr self) 1 original) 91)",
            true,
        ),
        (
            "(do (aset (.-arr self) 1 99) (set! (.-__hash self) 91) (aset (.-arr self) 1 original) 91)",
            false,
        ),
        (
            "(do (aset (.-arr self) 1 99) (aset (.-arr self) 1 original) (set! (.-__hash self) 91) 91)",
            true,
        ),
    ] {
        let (values, records) = maps(&mut session, 1);
        replace(
            &mut session,
            &format!("(let [original (aget (.-arr self) 1)] {body})"),
        );
        run(&mut session, &values[0]).unwrap();
        check(&mut session, &records, expected);
    }
}
#[test]
fn scoped_memo_stores_throw_publishes_before_unwind_and_next_invocation() {
    let mut session = Session::new_macro().unwrap();
    session
        .eval("(def scoped-error (js/Error. \"scoped intended\"))")
        .unwrap();
    let intended = session.eval("scoped-error").unwrap();
    let (values, records) = maps(&mut session, 1);
    replace(
        &mut session,
        "(do (hash (aget (.-arr self) 0)) (set! (.-__hash self) 91) (throw scoped-error))",
    );
    let SessionError::Language(actual) = run(&mut session, &values[0]).unwrap_err() else {
        panic!("expected original language exception")
    };
    session
        .inspect(&actual, |mut store, actual| {
            let intended = intended.value.to_rooted(&mut store);
            assert!(Rooted::<AnyRef>::ref_eq(
                &store,
                actual.unwrap_anyref().unwrap(),
                &intended
            )?);
            Ok(())
        })
        .unwrap();
    check(&mut session, &records, true);
    replace(&mut session, "(.-__hash self)");
    let value = run(&mut session, &values[0]).unwrap();
    assert_eq!(display(&mut session, &value), "91");
    check(&mut session, &records, true);
}
#[test]
fn scoped_memo_stores_generic_writes_keep_exact_old_receipt_without_minting() {
    let mut session = Session::new_macro().unwrap();
    let (values, records) = maps(&mut session, 1);
    replace(&mut session, "(do (set! (.-__hash self) 91) 91)");
    run(&mut session, &values[0]).unwrap();
    let get = session.eval("(fn [x] (.-__hash x))").unwrap();
    let old = session.invoke(&get, &[&values[0]]).unwrap();
    let put = session.eval("(fn [x v] (set! (.-__hash x) v))").unwrap();
    let fresh = session.eval("(+ 90 1)").unwrap();
    session.invoke(&put, &[&values[0], &fresh]).unwrap();
    check(&mut session, &records, false);
    session.invoke(&put, &[&values[0], &old]).unwrap();
    check(&mut session, &records, true);
    let (others, other_records) = maps(&mut session, 1);
    session.invoke(&put, &[&others[0], &old]).unwrap();
    check(&mut session, &other_records, false);
    // Scope is receiver-specific, even for the same physical class.
    session.eval("(def scoped-other nil)").unwrap();
    let call = session
        .eval("(fn [x other] (do (set! scoped-other other) (-hash x)))")
        .unwrap();
    let (fresh_values, fresh_records) = maps(&mut session, 2);
    replace(&mut session, "(do (set! (.-__hash scoped-other) 91) 91)");
    session
        .invoke(&call, &[&fresh_values[0], &fresh_values[1]])
        .unwrap();
    check(&mut session, &fresh_records, false);
}
#[test]
fn scoped_memo_stores_unrecorded_watch_and_wrong_slot_never_mint() {
    let mut session = Session::new_macro().unwrap();
    let original = session.eval("{:x 17}").unwrap();
    let fields = session.data_fields(&original).unwrap();
    let class = session.eval("PersistentArrayMap").unwrap();
    let refs = fields.iter().collect::<Vec<_>>();
    let (unrecorded, inputs) = session.data_construct_with_inputs(&class, &refs).unwrap();
    replace(&mut session, "(do (set! (.-__hash self) 91) 91)");
    run(&mut session, &unrecorded).unwrap();
    assert!(!inputs.unwrap().matches(&mut session, &unrecorded).unwrap());
    let (values, records) = maps(&mut session, 1);
    replace(&mut session, "(do (set! (.-cnt self) 99) 91)");
    run(&mut session, &values[0]).unwrap();
    check(&mut session, &records, false);
}
#[test]
fn scoped_memo_stores_forced_gc_in_nested_protocol_scope() {
    let mut session = Session::new_macro().unwrap();
    let (values, records) = maps(&mut session, 1);
    session.eval("(def scoped-gc nil)").unwrap();
    session.eval("(extend-type PersistentArrayMap ICounted (-count [self] (do (scoped-gc) (set! (.-__hash self) 91) 3)))").unwrap();
    replace(&mut session, "(do (count self) 91)");
    let ctor = session
        .runtime
        .get_func(&mut session.store, "closure-new")
        .unwrap();
    let parameter = ctor.ty(&session.store).params().nth(1).unwrap();
    let wasmtime::HeapType::ConcreteFunc(ty) = parameter.unwrap_ref().heap_type() else {
        panic!()
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let observer = session.dispatch_observer.clone().unwrap();
    let host = Func::new(
        &mut session.store,
        ty.clone(),
        move |mut caller, args, results| {
            assert!(observer.receiver(caller.as_context_mut()).is_some());
            caller.gc(None)?;
            assert!(observer.receiver(caller.as_context_mut()).is_some());
            observed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            results[0] = args[0].clone();
            Ok(())
        },
    );
    let nil_value = session.eval("nil").unwrap();
    let helper = {
        let mut scope = RootScope::new(&mut session.store);
        let mut result = [Val::null_any_ref()];
        let nil = Val::AnyRef(Some(nil_value.value.to_rooted(&mut scope)));
        ctor.call(
            &mut scope,
            &[nil, Val::FuncRef(Some(host)), Val::I32(0), Val::I32(0)],
            &mut result,
        )
        .unwrap();
        own(
            &mut scope,
            result[0].clone(),
            session.identity,
            &session.handles,
        )
        .unwrap()
    };
    let call = session
        .eval("(fn [x helper] (do (set! scoped-gc helper) (-hash x)))")
        .unwrap();
    session.invoke(&call, &[&values[0], &helper]).unwrap();
    assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    check(&mut session, &records, true);
}

#[test]
fn scoped_memo_stores_poststore_fuel_failure_publishes_none_and_recovers() {
    let mut session = Session::new_macro().unwrap();
    let (values, records) = maps(&mut session, 1);
    replace(
        &mut session,
        "(do (hash (aget (.-arr self) 0)) (set! (.-__hash self) 91) 91)",
    );
    let function = session.eval("(fn [x] (-hash x))").unwrap();
    let clear = session
        .eval("(fn [x] (do (set! (.-_hash (aget (.-arr x) 0)) nil) (set! (.-__hash x) nil)))")
        .unwrap();
    let get = session.eval("(fn [x] (.-__hash x))").unwrap();
    let default = session.options.fuel_per_operation;
    let mut witnessed = false;
    for fuel in (1..=65_536).step_by(64) {
        session.invoke(&clear, &[&values[0]]).unwrap();
        session.set_operation_fuel(fuel);
        let result = session.invoke(&function, &[&values[0]]);
        session.set_operation_fuel(default);
        let value = session.invoke(&get, &[&values[0]]).unwrap();
        if result.is_err() && display(&mut session, &value) == "91" {
            let error = result.unwrap_err();
            assert!(
                matches!(error, SessionError::Trap(ref e) if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
                "{error}"
            );
            let watches = registry_lock(&session.memo_ledger.entries)
                .watches
                .iter()
                .filter_map(Weak::upgrade)
                .collect::<Vec<_>>();
            assert!(
                watches
                    .iter()
                    .filter(|watch| watch.owner == "PersistentArrayMap")
                    .all(|watch| registry_lock(&watch.receipt).is_none()),
                "failed parent store cannot publish; completed child effects remain"
            );
            check(&mut session, &records, false);
            witnessed = true;
            break;
        }
        assert!(
            result.is_err(),
            "completed before post-store failure was witnessed"
        );
    }
    assert!(witnessed);
    session.invoke(&clear, &[&values[0]]).unwrap();
    session.invoke(&function, &[&values[0]]).unwrap();
    check(&mut session, &records, true);
}

#[test]
fn scoped_memo_stores_other_child_replacement_cannot_be_restored_into_authority() {
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session)
        .unwrap()
        .recording(&mut session)
        .unwrap();
    let number = bridge.scalar(&mut session, &Literal::Number(17.0)).unwrap();
    let child = bridge.vector_values(&mut session, &[number]).unwrap();
    let key = bridge
        .identifier(&mut session, None, "x", true, None)
        .unwrap();
    let root = bridge.map_values(&mut session, &[(key, child)]).unwrap();
    let records = bridge.into_records().unwrap();
    let class = session.eval("PersistentVector").unwrap();
    let descriptor = session.data_descriptor(&class, true).unwrap();
    let key_name = format!(
        "key-{}",
        "suss.core/IHash/-hash/1"
            .bytes()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let handles = session.handles.clone();
    let key = super::super::canonical_methods::binding(
        &mut session,
        "suss.internal.protocol-keys",
        &key_name,
        &handles,
    )
    .unwrap();
    let original = {
        let mut scope = RootScope::new(&mut session.store);
        let descriptor = Val::AnyRef(Some(descriptor.value.to_rooted(&mut scope)));
        let key = Val::AnyRef(Some(key.value.to_rooted(&mut scope)));
        let getter = session
            .runtime
            .get_func(&mut scope, "protocol-method-get")
            .unwrap();
        let mut result = [Val::null_any_ref()];
        getter
            .call(&mut scope, &[descriptor, key], &mut result)
            .unwrap();
        own(
            &mut scope,
            result[0].clone(),
            session.identity,
            &session.handles,
        )
        .unwrap()
    };
    session
        .eval("(extend-type PersistentVector IHash (-hash [_] 999))")
        .unwrap();
    replace(&mut session, "(do (set! (.-__hash self) 91) 91)");
    run(&mut session, &root).unwrap();
    {
        let mut scope = RootScope::new(&mut session.store);
        let descriptor = Val::AnyRef(Some(descriptor.value.to_rooted(&mut scope)));
        let key = Val::AnyRef(Some(key.value.to_rooted(&mut scope)));
        let original = Val::AnyRef(Some(original.value.to_rooted(&mut scope)));
        let setter = session
            .runtime
            .get_func(&mut scope, "protocol-method-set")
            .unwrap();
        setter
            .call(&mut scope, &[descriptor, key, original], &mut [])
            .unwrap();
    }
    check(&mut session, &records, false);
}
#[test]
fn scoped_memo_stores_private_raw_helpers_cannot_be_imported() {
    let mut session = Session::with_options_in(Default::default(), Phase::Macro).unwrap();
    for name in [
        "macro-memo-store-install",
        "macro-memo-array-store",
        "macro-memo-field-set",
    ] {
        let mut types = crate::runtime_abi::prelude();
        types.ty().function([], []);
        let mut imports = wasm_encoder::ImportSection::new();
        imports.import(
            "suss.runtime",
            name,
            wasm_encoder::EntityType::Function(crate::runtime_abi::TYPE_COUNT),
        );
        let mut module = wasm_encoder::Module::new();
        module
            .section(&types)
            .section(&imports)
            .section(&crate::runtime_abi::Manifest::default().section());
        let bytes = crate::portable::artifact_identity::annotate_source(
            &module.finish(),
            Phase::Macro,
            None,
            Some(&[]),
        )
        .unwrap();
        let error = session
            .install(&[&bytes], Default::default(), &[])
            .unwrap_err();
        assert!(error.to_string().contains("host-only"), "{name}: {error}");
    }
}
#[test]
fn scoped_memo_stores_nested_ihash_overrides_outer_receiver() {
    let mut session = Session::new_macro().unwrap();
    let (values, records) = maps(&mut session, 2);
    session
        .eval("(def scoped-outer nil) (def scoped-inner nil)")
        .unwrap();
    replace(
        &mut session,
        "(if (identical? self scoped-outer) (-hash scoped-inner) (do (set! (.-__hash scoped-outer) 91) 91))",
    );
    let call = session.eval("(fn [outer inner] (do (set! scoped-outer outer) (set! scoped-inner inner) (-hash outer)))").unwrap();
    session.invoke(&call, &[&values[0], &values[1]]).unwrap();
    check(&mut session, &records, false);
}

#[test]
fn scoped_memo_stores_cold_canonical_child_inside_custom_parent() {
    let mut session = Session::new_macro().unwrap();
    let (values, records) = maps(&mut session, 1);
    replace(
        &mut session,
        "(let [h (hash-unordered-coll self)] (set! (.-__hash self) h) h)",
    );
    let result = run(&mut session, &values[0]).unwrap();
    assert_eq!(display(&mut session, &result), "1217894065");
    check(&mut session, &records, true);
}

#[test]
fn scoped_memo_stores_poison_at_child_write_cannot_be_upgraded_after_restore() {
    let mut session = Session::new_macro().unwrap();
    for (restore_before, expected) in [(false, false), (true, true)] {
        let (values, records) = maps(&mut session, 1);
        let body = if restore_before {
            "(let [a (.-arr self) old (aget a 1)] (aset a 1 99) (aset a 1 old) (hash (aget a 0)) (set! (.-__hash self) 91) 91)"
        } else {
            "(let [a (.-arr self) old (aget a 1)] (aset a 1 99) (hash (aget a 0)) (aset a 1 old) (set! (.-__hash self) 91) 91)"
        };
        replace(&mut session, body);
        run(&mut session, &values[0]).unwrap();
        check(&mut session, &records, expected);
        let watches = registry_lock(&session.memo_ledger.entries)
            .watches
            .iter()
            .filter_map(Weak::upgrade)
            .collect::<Vec<_>>();
        let keyword = watches
            .iter()
            .find(|watch| watch.owner == "Keyword")
            .unwrap();
        assert_eq!(
            registry_lock(&keyword.receipt)
                .as_ref()
                .unwrap()
                .source_safe,
            expected,
            "restoring payload cannot upgrade the child write-time proof"
        );
    }
}

#[test]
fn scoped_memo_stores_unregistered_and_foreign_domain_ancestors_grant_no_layout() {
    let mut session = Session::new_macro().unwrap();
    for foreign in [false, true] {
        let (values, records) = maps(&mut session, 1);
        let key = session.eval("(fn [x] (aget (.-arr x) 0))").unwrap();
        let key = session.invoke(&key, &[&values[0]]).unwrap();
        let (parent, _keep_foreign_records) = if foreign {
            let (mut parents, records) = maps(&mut session, 1);
            (parents.remove(0), Some(records))
        } else {
            (session.eval("{:unregistered 17}").unwrap(), None)
        };
        session.eval("(def scoped-cross-domain-key nil)").unwrap();
        replace(&mut session, "(do (hash scoped-cross-domain-key) 91)");
        let invoke = session
            .eval("(fn [parent key] (do (set! scoped-cross-domain-key key) (-hash parent)))")
            .unwrap();
        session.invoke(&invoke, &[&parent, &key]).unwrap();
        check(&mut session, &records, false);
    }
}
#[test]
fn scoped_memo_stores_delegated_canonical_write_keeps_extent_and_write_time_proof() {
    use crate::portable_session::memo_leaf_test_support::Method;
    let mut session = Session::new_macro().unwrap();
    session.eval("(def leaf-original-method nil) (def scoped-delegate-calls 0)").unwrap();
    let method = Method::capture(&mut session, "PersistentArrayMap").unwrap();
    method.bind_delegate(&mut session).unwrap();
    for mode in 0..4 {
        let (values, records) = maps(&mut session, 1);
        let body = match mode {
            0 | 3 => "(do (set! scoped-delegate-calls (inc scoped-delegate-calls)) (leaf-original-method self))",
            1 => "(let [a (.-arr self) old (aget a 1)] (aset a 1 99) (aset a 1 old) (leaf-original-method self))",
            _ => "(let [a (.-arr self) old (aget a 1)] (aset a 1 99) (let [h (leaf-original-method self)] (aset a 1 old) h))",
        };
        replace(&mut session, body);
        session.collect().unwrap();
        let result = if mode == 3 {
            // Same genuine implementation, but no live IHash dispatcher extent.
            let direct = session.eval("(fn [self] (leaf-original-method self))").unwrap();
            session.invoke(&direct, &[&values[0]]).unwrap()
        } else {
            run(&mut session, &values[0]).unwrap()
        };
        if mode != 2 {
            assert_eq!(display(&mut session, &result), "1217894065");
        }
        // The rejected cases still execute the store and restore the payload;
        // rejection concerns write-time authority, not missing guest effects.
        let inspect = session.eval("(fn [x] [(some? (.-__hash x)) (aget (.-arr x) 1)])").unwrap();
        let stored = session.invoke(&inspect, &[&values[0]]).unwrap();
        assert_eq!(display(&mut session, &stored), "[true 17]");
        check(&mut session, &records, mode < 2);
        if mode == 0 {
            let calls = session.eval("scoped-delegate-calls").unwrap();
            assert_eq!(display(&mut session, &calls), "1");
        }
    }
    method.restore(&mut session).unwrap();
}
