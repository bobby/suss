//! Genuine Error prerequisite authoring; UNCOMPILED/UNEXECUTED, bootstrap stale.
#[path = "../../suss-compile/tests/support/portable_decode.rs"]
mod portable_decode;
use portable_decode::{Decoder, Observation};
use serde_json::{Value, json};
use suss_cli::portable_session::{Session, SessionError, SessionOptions, SessionValue};
use suss_compile::portable::resolve::Phase;
use wasmtime::{AnyRef, OwnedRooted};

fn raw_prototype(
    session: &mut Session,
    owner: &SessionValue,
    ordinary: bool,
) -> OwnedRooted<AnyRef> {
    session
        .inspect(owner, |mut store, value| {
            let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            assert_eq!(object.fields(&mut store)?.count(), 4);
            let fields = object
                .field(&mut store, if ordinary { 1 } else { 2 })?
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap();
            assert_eq!(fields.len(&store)?, if ordinary { 2 } else { 1 });
            fields
                .get(&mut store, if ordinary { 1 } else { 0 })?
                .unwrap_anyref()
                .unwrap()
                .to_owned_rooted(&mut store)
        })
        .unwrap()
}

#[test]
fn owned_error_ancestry_retains_actual_constructor_and_instance_prototype_references() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
        session.set_operation_fuel(100_000_000);
        session
            .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
            .unwrap();
        session.enter_namespace("user").unwrap();
        let error = session.eval("(.-prototype js/Error)").unwrap();
        let range = session.eval("(.-prototype js/RangeError)").unwrap();
        session.eval("(deftype RawErrorChild [message])").unwrap();
        let child = session.eval("(.-prototype RawErrorChild)").unwrap();
        session
            .eval("(set! (.-__proto__ (.-prototype RawErrorChild)) (.-prototype js/Error))")
            .unwrap();
        let instance = session.eval("(RawErrorChild. 73)").unwrap();
        session.collect().unwrap();
        let expected = session
            .inspect(&error, |mut store, value| {
                value.unwrap_anyref().unwrap().to_owned_rooted(&mut store)
            })
            .unwrap();
        let range_parent = raw_prototype(&mut session, &range, true);
        let child_parent = raw_prototype(&mut session, &child, true);
        let instance_parent = raw_prototype(&mut session, &instance, false);
        let child_ref = session
            .inspect(&child, |mut store, value| {
                value.unwrap_anyref().unwrap().to_owned_rooted(&mut store)
            })
            .unwrap();
        for parent in [&range_parent, &child_parent] {
            assert!(
                session
                    .inspect(&error, |store, _| wasmtime::Rooted::ref_eq(
                        &store, parent, &expected
                    ))
                    .unwrap()
            );
        }
        assert!(
            session
                .inspect(&child, |store, _| wasmtime::Rooted::ref_eq(
                    &store,
                    &instance_parent,
                    &child_ref
                ))
                .unwrap()
        );
        let again = session.eval("(.-prototype js/Error)").unwrap();
        session.collect().unwrap();
        assert!(
            session
                .inspect(&again, |store, value| wasmtime::Rooted::ref_eq(
                    &store,
                    value.unwrap_anyref().unwrap(),
                    &expected
                ))
                .unwrap()
        );
    }
}

fn value_json(value: Observation) -> Value {
    match value {
        Observation::Nil => json!({"tag":"nil"}),
        Observation::Bool(value) => json!({"tag":"bool","value":value}),
        Observation::Number(bits) => json!({"tag":"number","bits":format!("{bits:016x}")}),
        Observation::String(units) => json!({"tag":"string","units":units}),
        other => panic!("Unexpected closed Error scalar: {other:?}"),
    }
}
fn scalar_json(session: &mut Session, value: &SessionValue, classes: &[ErrorClass]) -> Value {
    session
        .inspect(value, |mut store, value| {
            if value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .is_some_and(|v| v.get_u32() == 6)
            {
                return Ok(json!({"tag":"undefined"}));
            }
        if let Some(owner) = value.unwrap_anyref().unwrap().as_struct(&store)? {
            if owner.fields(&mut store)?.count() == 4 {
                let descriptor = owner.field(&mut store, 0)?;
                assert!(wasmtime::Rooted::ref_eq(&store, descriptor.unwrap_anyref().unwrap(), &classes[0].backing_descriptor)?);
                assert!(wasmtime::StructType::eq(&owner.ty(&store)?, &classes[0].backing_type));
                let fields = owner.field(&mut store, 1)?.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                assert_eq!(fields.len(&store)?, 2);
                let table = fields.get(&mut store, 0)?.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                assert_eq!(table.len(&store)?, 8, "closed data descriptor object");
                let mut values = std::collections::BTreeMap::new();
                for index in (0..8).step_by(2) {
                    let key = table.get(&mut store, index)?;
                    let Observation::String(key) = Decoder::new(64).decode(&mut store, &key)? else { panic!("descriptor key") };
                    let slot = table.get(&mut store, index+1)?.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                    assert_eq!(slot.len(&store)?, 2);
                    assert_eq!(slot.get(&mut store, 0)?.unwrap_anyref().unwrap().as_i31(&store)?.unwrap().get_u32(), 7);
                    let payload = slot.get(&mut store, 1)?;
                    let payload = if payload.unwrap_anyref().unwrap().as_i31(&store)?.is_some_and(|v| v.get_u32() == 6) {
                        json!({"tag":"undefined"})
                    } else { value_json(Decoder::new(4097).decode(&mut store, &payload)?) };
                    assert!(values.insert(String::from_utf16(&key).unwrap(), payload).is_none());
                }
                for key in ["writable", "enumerable", "configurable"] {
                    assert_eq!(values[key]["tag"], "bool");
                }
                assert_eq!(values.keys().map(String::as_str).collect::<Vec<_>>(), ["configurable", "enumerable", "value", "writable"]);
                return Ok(json!({"tag":"data-descriptor","value":values["value"],"writable":values["writable"]["value"],"enumerable":values["enumerable"]["value"],"configurable":values["configurable"]["value"]}));
            }
        }
            Ok(value_json(Decoder::new(4097).decode(&mut store, &value)?))
        })
        .unwrap()
}
struct ErrorClass {
    _anchor: SessionValue,
    descriptor: OwnedRooted<AnyRef>,
    owner_type: wasmtime::StructType,
    backing_descriptor: OwnedRooted<AnyRef>,
    backing_type: wasmtime::StructType,
    name: &'static str,
}
fn classes(session: &mut Session) -> Vec<ErrorClass> {
    ["Error", "RangeError", "TypeError"]
        .into_iter()
        .map(|name| {
            let anchor = session.eval(&format!("(js/{name}.)")).unwrap();
            let (descriptor, owner_type, backing_descriptor, backing_type) = session
                .inspect(&anchor, |mut store, value| {
                    let owner = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    assert_eq!(owner.fields(&mut store)?.count(), 5);
                    let descriptor = owner
                        .field(&mut store, 0)?
                        .unwrap_anyref()
                        .unwrap()
                        .to_owned_rooted(&mut store)?;
                    let carrier = owner
                        .field(&mut store, 2)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap();
                    assert_eq!(carrier.len(&store)?, 2);
                    let backing = carrier
                        .get(&mut store, 1)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_struct(&store)?
                        .unwrap();
                    assert_eq!(backing.fields(&mut store)?.count(), 4);
                    let backing_descriptor = backing
                        .field(&mut store, 0)?
                        .unwrap_anyref()
                        .unwrap()
                        .to_owned_rooted(&mut store)?;
                    Ok((
                        descriptor,
                        owner.ty(&store)?,
                        backing_descriptor,
                        backing.ty(&store)?,
                    ))
                })
                .unwrap();
            ErrorClass {
                _anchor: anchor,
                descriptor,
                owner_type,
                backing_descriptor,
                backing_type,
                name,
            }
        })
        .collect()
}
fn error_json(session: &mut Session, value: &SessionValue, classes: &[ErrorClass]) -> Value {
    session.inspect(value, |mut store, value| {
        let owner = value.unwrap_anyref().unwrap().as_struct(&store)?.expect("actual Error storage");
        assert_eq!(owner.fields(&mut store)?.count(), 5);
        let descriptor = owner.field(&mut store, 0)?;
        let descriptor = descriptor.unwrap_anyref().unwrap();
        let class = classes.iter().find(|class| wasmtime::Rooted::ref_eq(&store, &class.descriptor, descriptor).unwrap()).expect("canonical Error/RangeError descriptor");
        assert!(wasmtime::StructType::eq(&owner.ty(&store)?, &class.owner_type));
        let schema = descriptor.as_struct(&store)?.unwrap();
        assert_eq!(schema.fields(&mut store)?.count(), 5);
        assert_eq!(schema.field(&mut store, 0)?.unwrap_i64(), match class.name { "Error" => 7, "RangeError" => 10, "TypeError" => 11, _ => unreachable!() });
        let message = owner.field(&mut store, 1)?;
        let Observation::String(units) = Decoder::new(4097).decode(&mut store, &message)? else { panic!("raw UTF16 message") };
        let carrier = owner.field(&mut store, 2)?.unwrap_anyref().unwrap().as_array(&store)?.expect("owned mutable Error carrier");
        assert_eq!(carrier.len(&store)?, 2);
        let flags = carrier.get(&mut store, 0)?.unwrap_anyref().unwrap().as_i31(&store)?.expect("constructor-time presence flags").get_u32();
        assert!(flags <= 3, "exact message/cause presence mask");
        let backing = carrier.get(&mut store, 1)?.unwrap_anyref().unwrap().as_struct(&store)?.expect("native backing owner");
        assert_eq!(backing.fields(&mut store)?.count(), 4);
        assert!(wasmtime::StructType::eq(&backing.ty(&store)?, &class.backing_type));
        let backing_descriptor = backing.field(&mut store, 0)?;
        assert!(wasmtime::Rooted::ref_eq(&store, backing_descriptor.unwrap_anyref().unwrap(), &class.backing_descriptor)?);
        let fields = backing.field(&mut store, 1)?.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
        assert_eq!(fields.len(&store)?, 2);
        let table = fields.get(&mut store, 0)?.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
        let len = table.len(&store)?;
        assert!(len <= 8192 && len % 2 == 0);
        let mut properties = std::collections::BTreeMap::new();
        for index in (0..len).step_by(2) {
            let key = table.get(&mut store, index)?;
            let Observation::String(key) = Decoder::new(4097).decode(&mut store, &key)? else { panic!("raw property key") };
            let descriptor = table.get(&mut store, index+1)?.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            assert_eq!(descriptor.len(&store)?, 2);
            let attributes = descriptor.get(&mut store, 0)?.unwrap_anyref().unwrap().as_i31(&store)?.unwrap().get_u32();
            assert!(attributes <= 15 && attributes & 8 == 0, "closed raw Error data properties");
            let payload = descriptor.get(&mut store, 1)?;
            let payload = if payload.unwrap_anyref().unwrap().as_i31(&store)?.is_some_and(|v| v.get_u32() == 6) { json!({"tag":"undefined"}) }
                else { value_json(Decoder::new(4097).decode(&mut store, &payload)?) };
            assert!(properties.insert(key, (attributes, payload)).is_none(), "duplicate raw key");
        }
        let message = properties.get(&"message".encode_utf16().collect::<Vec<_>>());
        let cause = properties.get(&"cause".encode_utf16().collect::<Vec<_>>());
        // Constructor observations must preserve both the physical snapshots
        // and the independently decoded live nonenumerable descriptors.
        assert_eq!(message.is_some(), flags & 2 != 0);
        if let Some((attributes, value)) = message { assert_eq!(*attributes, 5); assert_eq!(*value, json!({"tag":"string","units":units})); }
        assert_eq!(cause.is_some(), flags & 1 != 0);
        if let Some((attributes, _)) = cause { assert_eq!(*attributes, 5); }
        let cause = cause.map(|(_, value)| value.clone()).unwrap_or(json!({"tag":"undefined"}));
        let initial_cause = owner.field(&mut store, 3)?;
        let initial_cause = if initial_cause.unwrap_anyref().unwrap().as_i31(&store)?.is_some_and(|v| v.get_u32() == 6) { json!({"tag":"undefined"}) }
            else { value_json(Decoder::new(128).decode(&mut store, &initial_cause)?) };
        assert_eq!(cause, initial_cause, "physical cause snapshot remains exact");
        assert!(flags & 1 != 0 || cause == json!({"tag":"undefined"}));
        Ok(json!({"tag":"error","class":class.name,"message":units,"messageOwn":message.is_some(),"causeOwn":flags & 1 != 0,"cause":cause}))
    }).unwrap()
}
#[test]
fn raw_error_constructors_ancestry_original_throws_and_effects_match_pinned_after_gc() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/error-constructor-cases.json"
    ))
    .unwrap();
    assert_eq!(
        corpus["upstream"],
        "c4295f303100bbf5afac449242d30bca1126f1a1"
    );
    assert_eq!(corpus["cases"].as_array().unwrap().len(), 151);
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
        session.set_operation_fuel(100_000_000);
        session
            .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
            .unwrap();
        session.enter_namespace("user").unwrap();
        session.eval("(def error-effects 0) (def error-step (fn [n] (set! error-effects (+ (* error-effects 10) n))))").unwrap();
        let classes = classes(&mut session);
        session.collect().unwrap();
        for case in corpus["cases"].as_array().unwrap() {
            session.eval("(set! error-effects 0)").unwrap();
            let result = session.eval(case["source"].as_str().unwrap());
            let value = match (case["mode"].as_str().unwrap(), result) {
                ("return", Ok(value)) | ("throw", Err(SessionError::Language(value))) => value,
                (_, result) => panic!("{} unexpected mode: {result:?}", case["id"]),
            };
            session.collect().unwrap();
            let actual = if case["expected"]["value"]["tag"] == "error" {
                error_json(&mut session, &value, &classes)
            } else {
                scalar_json(&mut session, &value, &classes)
            };
            assert_eq!(actual, case["expected"]["value"], "{}", case["id"]);
            let trace = session.eval("error-effects").unwrap();
            session.collect().unwrap();
            let mut decoder = Decoder::new(128);
            assert_eq!(
                value_json(decoder.decode_session(&mut session, &trace).unwrap()),
                case["expected"]["trace"],
                "{} ordered trace",
                case["id"]
            );
            let recovery = session.eval("(+ 17 43)").unwrap();
            let mut decoder = Decoder::new(128);
            assert_eq!(
                value_json(decoder.decode_session(&mut session, &recovery).unwrap()),
                json!({"tag":"number","bits":"404e000000000000"})
            );
        }
        let constructor = session.eval("js/Error").unwrap();
        let invoke = session
            .eval("(fn [ctor] (ctor \"retained\" (js-obj \"cause\" 73)))")
            .unwrap();
        session.collect().unwrap();
        let value = session.invoke(&invoke, &[&constructor]).unwrap();
        session.collect().unwrap();
        assert_eq!(
            error_json(&mut session, &value, &classes),
            json!({"tag":"error","class":"Error","message":[114,101,116,97,105,110,101,100],"messageOwn":true,"causeOwn":true,"cause":{"tag":"number","bits":"4052400000000000"}})
        );
    }
}

#[test]
fn owned_error_mutation_retains_backing_and_rejects_invalid_presence_masks_after_gc() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
        session.set_operation_fuel(100_000_000);
        session
            .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
            .unwrap();
        session.enter_namespace("user").unwrap();
        let anchors = classes(&mut session);
        let value = session
            .eval("(def raw-owned-error (js/Error. \"before\")) raw-owned-error")
            .unwrap();
        let backing = session
            .inspect(&value, |mut store, v| {
                let owner = v.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let carrier = owner
                    .field(&mut store, 2)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                carrier
                    .get(&mut store, 1)?
                    .unwrap_anyref()
                    .unwrap()
                    .to_owned_rooted(&mut store)
            })
            .unwrap();
        session
            .eval("(set! (.-message raw-owned-error) \"after\")")
            .unwrap();
        session.collect().unwrap();
        session
            .inspect(&value, |mut store, v| {
                let owner = v.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let carrier = owner
                    .field(&mut store, 2)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                let current = carrier.get(&mut store, 1)?;
                assert!(wasmtime::Rooted::ref_eq(
                    &store,
                    current.unwrap_anyref().unwrap(),
                    &backing
                )?);
                Ok(())
            })
            .unwrap();
        let message = session.eval("(.-message raw-owned-error)").unwrap();
        assert_eq!(
            Decoder::new(32)
                .decode_session(&mut session, &message)
                .unwrap(),
            Observation::String("after".encode_utf16().collect())
        );
        for mask in [4, 7, 0x7fff_ffff] {
            session
                .inspect(&value, |mut store, v| {
                    let owner = v.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    let carrier = owner
                        .field(&mut store, 2)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap();
                    let mask_value =
                        AnyRef::from_i31(&mut store, wasmtime::I31::new_u32(mask).unwrap());
                    carrier.set(&mut store, 0, wasmtime::Val::AnyRef(Some(mask_value)))?;
                    Ok(())
                })
                .unwrap();
            session.collect().unwrap();
            let error = match session.eval("(.-message raw-owned-error)") {
                Err(SessionError::Language(value)) => value,
                other => panic!("malformed Error mask must reject: {other:?}"),
            };
            session
                .inspect(&error, |mut store, v| {
                    let owner = v.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    let descriptor = owner.field(&mut store, 0)?;
                    assert!(wasmtime::Rooted::ref_eq(
                        &store,
                        descriptor.unwrap_anyref().unwrap(),
                        &anchors[0].descriptor
                    )?);
                    let message = owner
                        .field(&mut store, 1)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap();
                    let units = message
                        .elems(&mut store)?
                        .map(|v| v.unwrap_i32() as u16)
                        .collect::<Vec<_>>();
                    assert_eq!(
                        units,
                        "Invalid nominal operation"
                            .encode_utf16()
                            .collect::<Vec<_>>()
                    );
                    Ok(())
                })
                .unwrap();
            session
                .inspect(&value, |mut store, v| {
                    let owner = v.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                    let carrier = owner
                        .field(&mut store, 2)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_array(&store)?
                        .unwrap();
                    let mask_value =
                        AnyRef::from_i31(&mut store, wasmtime::I31::new_u32(2).unwrap());
                    carrier.set(&mut store, 0, wasmtime::Val::AnyRef(Some(mask_value)))?;
                    Ok(())
                })
                .unwrap();
            session.collect().unwrap();
            let message = session.eval("(.-message raw-owned-error)").unwrap();
            assert_eq!(
                Decoder::new(32)
                    .decode_session(&mut session, &message)
                    .unwrap(),
                Observation::String("after".encode_utf16().collect())
            );
        }
    }
}
