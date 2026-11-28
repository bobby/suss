//! Execute the actual application artifact through an independent typed Rust host.
#![cfg(not(target_family = "wasm"))]
use std::{path::PathBuf, sync::OnceLock};
use suss_cli::portable_aot::{AotOptions, compile_file_with_options};
use suss_reader::Symbol;
use suss_usermanager_host::{Application, Request, decode_request};

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
