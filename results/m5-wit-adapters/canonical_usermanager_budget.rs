//! End-to-end application measurements; report callback costs without subtraction.
use serde_json::{Value, json};
use std::io::Write;
use std::time::Instant;
use suss_usermanager_host::{Application, Department, Form, Request};

fn mark(app: &Application) -> (Instant, u64, u64) {
    (
        Instant::now(),
        app.store.data().capability_time_ns(),
        app.store.data().capability_call_count(),
    )
}
fn row(
    app: &Application,
    before: (Instant, u64, u64),
    repetition: usize,
    operation: &str,
    count: usize,
) -> Value {
    json!({"repetition":repetition,"operation":operation,"ns":before.0.elapsed().as_nanos(),"count":count,
        "host_capability_ns":app.store.data().capability_time_ns()-before.1,
        "host_capability_calls":app.store.data().capability_call_count()-before.2})
}
fn form(id: Option<String>, label: String) -> Form {
    Form {
        id,
        first_name: Some(Some(label)),
        last_name: None,
        email: None,
        department_id: Some("4".into()),
    }
}
fn main() -> wasmtime::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .ok_or_else(|| wasmtime::Error::msg("usage: measure COMPONENT DATASET-SIZE"))?;
    let size = args
        .next()
        .ok_or_else(|| wasmtime::Error::msg("missing dataset size"))?
        .parse::<usize>()?;
    wasmtime::ensure!(
        size > 0 && args.next().is_none(),
        "positive size and exactly two arguments required"
    );
    let start = Instant::now();
    let bytes = std::fs::read(path)?;
    let read_ns = start.elapsed().as_nanos();
    let mut app =
        Application::load(&bytes).map_err(|error| error.context("load measurement application"))?;
    let load_ns = start.elapsed().as_nanos();
    let db = app
        .open(":memory:")?
        .map_err(|e| wasmtime::Error::msg(e.message))?;
    let first = app
        .request(&db, &Request::Departments)?
        .map_err(|e| wasmtime::Error::msg(e.message))?;
    assert!(first.departments.is_empty());
    println!(
        "{}",
        json!({"phase":"first-response","dataset":size,"read_ns":read_ns,
        "load_through_instantiation_ns":load_ns,"through_first_useful_response_ns":start.elapsed().as_nanos()})
    );
    std::io::stdout().flush()?;
    app.request(
        &db,
        &Request::SeedDepartment(Department {
            id: 4,
            name: "Development".into(),
        }),
    )?
    .unwrap();
    let setup = mark(&app);
    for id in 0..size {
        app.request(
            &db,
            &Request::SaveUser(Form {
                id: None,
                first_name: Some(Some(format!("Name {id:08} λ🦀"))),
                last_name: Some(Some(format!("Family {:08}", size - id))),
                email: Some(Some(format!("user{id}@example.test"))),
                department_id: Some("4".into()),
            }),
        )?
        .unwrap();
    }
    let mut rows = vec![row(&app, setup, 0, "setup-through-guest-crud", size)];
    println!("{}", json!({"phase":"setup-complete","dataset":size}));
    std::io::stdout().flush()?;
    let mut changes = size as u64;
    for repetition in 0..5 {
        let before = mark(&app);
        let fuel_budget = 3_000_000_000u64;
        app.store.set_fuel(fuel_budget)?;
        let view = app.bindings.suss_usermanager_application()
            .call_handle(&mut app.store, wasmtime::component::Resource::new_borrow(db.rep()), &Request::ListUsers)
            .map_err(|error| {
                error.context(format!("list dataset {size}, repetition {repetition}"))
            })?
            .unwrap();
        assert_eq!(view.users.len(), size);
        assert_eq!(view.changes, changes);
        assert_eq!(
            view.users[0].value.last_name.as_deref(),
            Some("Family 00000001")
        );
        let mut list = row(&app, before, repetition, "joined-sorted-list", size);
        list["fuel_budget"] = json!(fuel_budget);
        list["fuel_consumed"] = json!(fuel_budget - app.store.get_fuel()?);
        println!("{}", json!({"phase":"diagnostic-list","measurement":list,"qualification":"explicit diagnostic caller budget; host defaults unchanged"}));
        rows.push(list);
        let before = mark(&app);
        for _ in 0..1000 {
            assert_eq!(app.changes(&db)?, changes);
        }
        rows.push(row(
            &app,
            before,
            repetition,
            "guest-host-key-change-counter",
            1000,
        ));
        // Create/read/partial-update/delete through guest and both WIT directions.
        let before = mark(&app);
        for index in 0..100 {
            let id = (size + repetition * 100 + index + 1) as i64;
            app.request(&db, &Request::SaveUser(form(None, "temporary".into())))?
                .unwrap();
            let edit = app.request(&db, &Request::EditUser(Some(id)))?.unwrap();
            assert_eq!(edit.user.unwrap().first_name.as_deref(), Some("temporary"));
            app.request(
                &db,
                &Request::SaveUser(form(Some(id.to_string()), "updated".into())),
            )?
            .unwrap();
            app.request(&db, &Request::DeleteUser(id))?.unwrap();
            changes += 3;
        }
        rows.push(row(
            &app,
            before,
            repetition,
            "create-read-update-delete",
            100,
        ));
        assert_eq!(app.changes(&db)?, changes);
    }
    app.close(db)?;
    assert_eq!(app.store.data().live, 0);
    let status = std::fs::read_to_string("/proc/self/status")?;
    println!(
        "{}",
        json!({"phase":"completed","dataset":size,"measurements":rows,
        "storage_operations":app.store.data().calls,"host_capability_ns":app.store.data().capability_time_ns(),
        "host_capability_calls":app.store.data().capability_call_count(),"writes":app.store.data().writes,
        "opened":app.store.data().opened,"closed":app.store.data().closed,"live":app.store.data().live,
        "memory_growth_events":app.store.data().memory_events,
        "peak_rss":status.lines().find(|line|line.starts_with("VmHWM:")).unwrap_or("unknown"),
        "limitations":"Raw linear-memory events are not GC-heap accounting. Canonical baseline measured separately by read-only observer tests. Boundary counter includes real guest work, resource lowering and host key lookup. Storage here is in memory; separate CLI scenario includes JSON file I/O."})
    );
    Ok(())
}
