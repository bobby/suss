//! Execute the actual application artifact through an independent typed Rust host.
#![cfg(not(target_family = "wasm"))]
use std::{path::PathBuf, sync::OnceLock};
use suss_cli::portable_aot::{AotOptions, compile_file_with_options};
use suss_reader::Symbol;
use suss_usermanager_host::{Application, Request, decode_request};
#[path = "support/usermanager_canonical_stats.rs"]
mod canonical_stats;

fn component() -> &'static [u8] {
    static COMPONENT: OnceLock<Vec<u8>> = OnceLock::new();
    COMPONENT.get_or_init(|| {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/usermanager");
        compile_file_with_options(
            &root.join("src/usermanager/app.cljc"),
            &root.join("api.wit"),
            Some("usermanager"),
            &[],
            &AotOptions {
                exports: [
                    ("open", "open"),
                    ("handle", "request"),
                    ("changes", "changes"),
                    ("close", "close"),
                ]
                .into_iter()
                .map(|(name, source)| {
                    (
                        format!("suss:usermanager/application@0.1.0#{name}"),
                        Symbol::parse(&format!("usermanager.app/{source}")),
                    )
                })
                .collect(),
                imports: [
                    "open",
                    "key",
                    "users",
                    "departments",
                    "get-user",
                    "put-department",
                    "insert-user",
                    "update-user",
                    "delete-user",
                ]
                .into_iter()
                .map(|name| {
                    (
                        format!("suss:usermanager/storage@0.1.0#{name}"),
                        Symbol::parse(&format!("usermanager.storage/{name}")),
                    )
                })
                .collect(),
                ..Default::default()
            },
        )
        .unwrap()
    })
}

#[test]
fn actual_component_matches_shared_jvm_application_scenarios_and_reopens_persistence() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../examples/usermanager/fixtures/scenario.json"
    ))
    .unwrap();
    let expected = include_str!("../../../examples/usermanager/fixtures/jvm-observations.jsonl")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("storage.json");
    std::fs::write(
        &path,
        serde_json::to_vec(
            &serde_json::json!({"next_id":fixture["next_id"],"users":{},"departments":{}}),
        )
        .unwrap(),
    )
    .unwrap();
    let mut app = Application::load(component()).unwrap();
    let db = app.open(path.to_str().unwrap()).unwrap().unwrap();
    let operations = fixture["operations"].as_array().unwrap();
    assert_eq!(operations.len(), expected.len());
    for (index, (operation, expected)) in operations.iter().zip(expected).enumerate() {
        if let Some(message) = operation.get("fail_next") {
            app.store.data_mut().fail_next = Some(message.as_str().unwrap().to_owned());
        }
        let request = decode_request(operation.clone()).unwrap();
        let result = app.request(&db, &request).unwrap();
        let changes = app.changes(&db).unwrap();
        assert_eq!(
            serde_json::json!({"result":result,"changes":changes,"trace":app.store.data().trace}),
            expected,
            "operation {index}: {operation}"
        );
        app.store.gc(None).unwrap();
    }
    let before = app.request(&db, &Request::ListUsers).unwrap().unwrap();
    app.close(db).unwrap();
    assert_eq!(app.store.data().live, 0);
    assert_eq!(app.store.data().closed, 1);
    let reopened = app.open(path.to_str().unwrap()).unwrap().unwrap();
    assert_eq!(app.changes(&reopened).unwrap(), 0);
    let after = app
        .request(&reopened, &Request::ListUsers)
        .unwrap()
        .unwrap();
    assert_eq!(before.users, after.users);
    app.close(reopened).unwrap();
    assert_eq!(app.store.data().opened, 2);
    assert_eq!(app.store.data().closed, 2);
    assert_eq!(app.store.data().live, 0);
}
#[test]
fn ten_thousand_mixed_calls_keep_sessions_isolated_and_close_all_owners() {
    run_ten_thousand(component(), false);
}
#[test]
fn canonical_live_blocks_and_bytes_return_to_baseline_after_ten_thousand_calls() {
    let bytes = canonical_stats::instrument(component());
    run_ten_thousand(&bytes, true);
}
fn run_ten_thousand(bytes: &[u8], observe: bool) {
    use suss_usermanager_host::{ErrorKind, Form};
    let mut app = Application::load(bytes).unwrap();
    let stats = observe.then(|| {
        app.instance
            .get_typed_func::<(), (u64,)>(&mut app.store, "canonical-stats")
            .unwrap()
    });
    let baseline = stats
        .as_ref()
        .map(|f| f.call(&mut app.store, ()).unwrap().0);
    let check = |app: &mut Application| {
        if let Some(f) = &stats {
            assert_eq!(
                f.call(&mut app.store, ()).unwrap().0,
                baseline.unwrap(),
                "live canonical blocks/bytes"
            );
        }
    };
    assert_eq!(
        app.store
            .data()
            .memory_events
            .iter()
            .filter(|(current, desired)| *current == 0 && *desired == 65536)
            .count(),
        1
    );
    app.store.data_mut().small_memory_limit = Some(65536);
    let first = app.open(":memory:").unwrap().unwrap();
    let second = app.open(":memory:").unwrap().unwrap();
    app.request(
        &first,
        &decode_request(serde_json::json!({"op":"seed-department","id":4,"name":"Development"}))
            .unwrap(),
    )
    .unwrap()
    .unwrap();
    app.request(
        &first,
        &Request::SaveUser(Form {
            id: None,
            first_name: Some(Some("retained λ🦀".into())),
            last_name: Some(Some("User".into())),
            email: None,
            department_id: Some("4".into()),
        }),
    )
    .unwrap()
    .unwrap();
    let mut changes = 1;
    for index in 0..10_000 {
        let form = |id: &str, name: String| {
            Request::SaveUser(Form {
                id: Some(id.into()),
                first_name: Some(Some(name)),
                last_name: None,
                email: None,
                department_id: Some("4".into()),
            })
        };
        let request = match index % 8 {
            0 => {
                changes += 1;
                form("bad", "must not write".into())
            }
            1 => {
                changes += 1;
                form("1", format!("retained λ🦀 {index}"))
            }
            2 => {
                changes += 1;
                app.store.data_mut().fail_next = Some("stress storage error".into());
                form("1", "must not write".into())
            }
            3 => Request::EditUser(Some(1)),
            4 => {
                changes += 1;
                Request::DeleteUser(-17)
            }
            5 => Request::ListUsers,
            6 => {
                changes = 0;
                Request::ResetChanges
            }
            _ => {
                changes += 1;
                form("１", format!("retained λ🦀 {index}"))
            }
        };
        let result = app.request(&first, &request).unwrap();
        match index % 8 {
            0 => assert_eq!(result.unwrap_err().kind, ErrorKind::Parse),
            2 => assert_eq!(result.unwrap_err().kind, ErrorKind::Storage),
            _ => {
                result.unwrap();
            }
        }
        assert_eq!(app.changes(&first).unwrap(), changes, "call {index}");
        assert_eq!(app.store.data().live, 2);
        check(&mut app);
        if index % 100 == 0 {
            app.store.gc(None).unwrap();
            assert_eq!(app.changes(&second).unwrap(), 0);
            assert!(
                app.request(&second, &Request::ListUsers)
                    .unwrap()
                    .unwrap()
                    .users
                    .is_empty()
            );
        }
    }
    app.store.gc(None).unwrap();
    let retained = app
        .request(&first, &Request::EditUser(Some(1)))
        .unwrap()
        .unwrap()
        .user
        .unwrap();
    assert!(retained.first_name.unwrap().starts_with("retained λ🦀"));
    app.close(first).unwrap();
    assert_eq!(app.store.data().live, 1);
    assert_eq!(app.changes(&second).unwrap(), 0);
    app.close(second).unwrap();
    app.store.gc(None).unwrap();
    assert_eq!(app.store.data().live, 0);
    assert_eq!(app.store.data().opened, 2);
    assert_eq!(app.store.data().closed, 2);
    check(&mut app);
    eprintln!(
        "canonical baseline={baseline:?}; memory growth events={:?}",
        app.store.data().memory_events
    );
}

#[test]
fn application_resource_identity_survives_moves_and_rejects_escaped_and_closed_aliases() {
    use suss_usermanager_host::Session as StorageSession;
    use wasmtime::component::Resource;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/usermanager");
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("app.cljc");
    let wit = directory.path().join("api.wit");
    let mut text = std::fs::read_to_string(root.join("src/usermanager/app.cljc")).unwrap();
    text.push_str(
        r#"
      (def escaped-handle (atom nil))
      (def moved-handle (atom nil))
      (def closed-handle (atom nil))
      (defn remember [handle] (reset! escaped-handle handle) nil)
      (defn move-owner [handle] (reset! moved-handle handle) handle)
      (defn close-owner [handle] (reset! closed-handle handle) (close handle))
      (defn rejected-handle? [handle]
        (try (usermanager.storage/key handle) false (catch :default error true)))
      (defn escaped [] (rejected-handle? @escaped-handle))
      (defn moved [] (rejected-handle? @moved-handle))
      (defn closed [] (rejected-handle? @closed-handle))
    "#,
    );
    std::fs::write(&source, text).unwrap();
    let text=std::fs::read_to_string(root.join("api.wit")).unwrap().replace(
        "world usermanager { import storage; export application; }",
        "world usermanager { import storage; export application; use storage.{session}; export remember:func(db:borrow<session>); export move-owner:func(db:own<session>)->own<session>; export close-owner:func(db:own<session>)->u64; export escaped:func()->bool; export moved:func()->bool; export closed:func()->bool; }");
    std::fs::write(&wit, text).unwrap();
    let mut options = AotOptions {
        exports: [
            ("open", "open"),
            ("handle", "request"),
            ("changes", "changes"),
            ("close", "close"),
        ]
        .into_iter()
        .map(|(name, source)| {
            (
                format!("suss:usermanager/application@0.1.0#{name}"),
                Symbol::parse(&format!("usermanager.app/{source}")),
            )
        })
        .collect(),
        imports: [
            "open",
            "key",
            "users",
            "departments",
            "get-user",
            "put-department",
            "insert-user",
            "update-user",
            "delete-user",
        ]
        .into_iter()
        .map(|name| {
            (
                format!("suss:usermanager/storage@0.1.0#{name}"),
                Symbol::parse(&format!("usermanager.storage/{name}")),
            )
        })
        .collect(),
        ..Default::default()
    };
    for name in [
        "remember",
        "move-owner",
        "close-owner",
        "escaped",
        "moved",
        "closed",
    ] {
        options.exports.push((
            name.into(),
            Symbol::parse(&format!("usermanager.app/{name}")),
        ));
    }
    let bytes =
        compile_file_with_options(&source, &wit, Some("usermanager"), &[], &options).unwrap();
    let mut app = Application::load(&bytes).unwrap();
    let db = app.open(":memory:").unwrap().unwrap();
    let rep = db.rep();
    let remember = app
        .instance
        .get_typed_func::<(Resource<StorageSession>,), ()>(&mut app.store, "remember")
        .unwrap();
    remember
        .call(&mut app.store, (Resource::new_borrow(rep),))
        .unwrap();
    app.store.gc(None).unwrap();
    let escaped = app
        .instance
        .get_typed_func::<(), (bool,)>(&mut app.store, "escaped")
        .unwrap();
    assert!(escaped.call(&mut app.store, ()).unwrap().0);
    let mover = app
        .instance
        .get_typed_func::<(Resource<StorageSession>,), (Resource<StorageSession>,)>(
            &mut app.store,
            "move-owner",
        )
        .unwrap();
    let db = mover.call(&mut app.store, (db,)).unwrap().0;
    assert_eq!(db.rep(), rep, "same underlying Rust persistence resource");
    app.store.gc(None).unwrap();
    let moved = app
        .instance
        .get_typed_func::<(), (bool,)>(&mut app.store, "moved")
        .unwrap();
    assert!(moved.call(&mut app.store, ()).unwrap().0);
    assert_eq!(app.changes(&db).unwrap(), 0);
    assert!(
        app.request(&db, &Request::ListUsers)
            .unwrap()
            .unwrap()
            .users
            .is_empty()
    );
    let closer = app
        .instance
        .get_typed_func::<(Resource<StorageSession>,), (u64,)>(&mut app.store, "close-owner")
        .unwrap();
    assert_eq!(closer.call(&mut app.store, (db,)).unwrap().0, 1);
    app.store.gc(None).unwrap();
    let closed = app
        .instance
        .get_typed_func::<(), (bool,)>(&mut app.store, "closed")
        .unwrap();
    assert!(closed.call(&mut app.store, ()).unwrap().0);
    assert_eq!(app.store.data().opened, 1);
    assert_eq!(app.store.data().closed, 1);
    assert_eq!(app.store.data().live, 0);
}
