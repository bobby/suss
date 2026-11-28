//! Independent typed Rust persistence/transport host; application rules live in Suss.
use std::{collections::BTreeMap, path::PathBuf};
use wasmtime::{
    Config, Engine, Store,
    component::{Component, HasSelf, Linker, Resource, ResourceTable},
};

wasmtime::component::bindgen!({
    path: "../api.wit",
    world: "usermanager",
    with: { "suss:usermanager/storage.session": Session },
    imports: { default: trappable },
    additional_derives: [serde::Serialize, serde::Deserialize, PartialEq],
});
pub use exports::suss::usermanager::application::{Form, Request, View, ViewKind};
pub use suss::usermanager::storage::{Department, ErrorKind, Failure, Patch, User};

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct Database {
    pub users: BTreeMap<i64, User>,
    pub departments: BTreeMap<i64, Department>,
    pub next_id: i64,
}
pub struct Session {
    pub key: u64,
    pub path: Option<PathBuf>,
    pub database: Database,
}
#[derive(Default)]
pub struct State {
    pub table: ResourceTable,
    pub opened: u64,
    pub closed: u64,
    pub live: u64,
    pub calls: u64,
    pub writes: u64,
    pub trace: Vec<String>,
    pub fail_next: Option<String>,
}
fn failure(message: impl ToString) -> Failure {
    Failure {
        kind: ErrorKind::Storage,
        message: message.to_string(),
    }
}
impl State {
    fn before(&mut self, operation: &str) -> Option<Failure> {
        self.calls += 1;
        self.trace.push(operation.to_owned());
        self.fail_next.take().map(failure)
    }
    fn persist(session: &Session) -> Result<(), Failure> {
        let Some(path) = &session.path else {
            return Ok(());
        };
        // Encode before touching the previous file and replace only a complete write.
        let bytes = serde_json::to_vec(&session.database).map_err(failure)?;
        let temporary = path.with_extension(format!("suss-{}.tmp", session.key));
        let result =
            std::fs::write(&temporary, bytes).and_then(|()| std::fs::rename(&temporary, path));
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result.map_err(failure)
    }
    fn borrow(&self, resource: &Resource<Session>) -> wasmtime::Result<&Session> {
        wasmtime::ensure!(
            !resource.owned(),
            "storage operations require borrowed sessions"
        );
        Ok(self.table.get(resource)?)
    }
    fn change(
        &mut self,
        resource: &Resource<Session>,
        f: impl FnOnce(&mut Database) -> Result<(), Failure>,
    ) -> wasmtime::Result<Result<(), Failure>> {
        self.borrow(resource)?;
        let session = self.table.get_mut(resource)?;
        // A failed persistence operation leaves this session's previous data intact.
        let old = serde_json::to_vec(&session.database)?;
        if let Err(error) = f(&mut session.database).and_then(|()| Self::persist(session)) {
            session.database = serde_json::from_slice(&old)?;
            return Ok(Err(error));
        }
        self.writes += 1;
        Ok(Ok(()))
    }
}
impl suss::usermanager::storage::HostSession for State {
    fn drop(&mut self, resource: Resource<Session>) -> wasmtime::Result<()> {
        wasmtime::ensure!(resource.owned(), "drop requires ownership");
        let session = self.table.delete(resource)?;
        self.trace.push(format!("close:{}", session.key));
        self.closed += 1;
        self.live -= 1;
        Ok(())
    }
}
fn apply(row: &mut User, patch: Patch) {
    if let Some(value) = patch.first_name {
        row.first_name = value;
    }
    if let Some(value) = patch.last_name {
        row.last_name = value;
    }
    if let Some(value) = patch.email {
        row.email = value;
    }
    row.department_id = patch.department_id;
}
impl suss::usermanager::storage::Host for State {
    fn open(&mut self, path: String) -> wasmtime::Result<Result<Resource<Session>, Failure>> {
        if let Some(error) = self.before("open") {
            return Ok(Err(error));
        }
        let path = if path == ":memory:" {
            None
        } else {
            Some(PathBuf::from(path))
        };
        let database = match &path {
            Some(path) if path.exists() => match std::fs::read(path)
                .map_err(failure)
                .and_then(|bytes| serde_json::from_slice(&bytes).map_err(failure))
            {
                Ok(database) => database,
                Err(error) => return Ok(Err(error)),
            },
            _ => Database::default(),
        };
        let key = self.opened + 1;
        let session = Session {
            key,
            path,
            database,
        };
        if let Err(error) = Self::persist(&session) {
            return Ok(Err(error));
        }
        let resource = self.table.push(session)?;
        self.opened = key;
        self.live += 1;
        Ok(Ok(resource))
    }
    fn key(&mut self, db: Resource<Session>) -> wasmtime::Result<u64> {
        Ok(self.borrow(&db)?.key)
    }
    fn users(&mut self, db: Resource<Session>) -> wasmtime::Result<Result<Vec<User>, Failure>> {
        if let Some(error) = self.before("users") {
            return Ok(Err(error));
        }
        Ok(Ok(self
            .borrow(&db)?
            .database
            .users
            .values()
            .cloned()
            .collect()))
    }
    fn departments(
        &mut self,
        db: Resource<Session>,
    ) -> wasmtime::Result<Result<Vec<Department>, Failure>> {
        if let Some(error) = self.before("departments") {
            return Ok(Err(error));
        }
        Ok(Ok(self
            .borrow(&db)?
            .database
            .departments
            .values()
            .cloned()
            .collect()))
    }
    fn get_user(
        &mut self,
        db: Resource<Session>,
        id: i64,
    ) -> wasmtime::Result<Result<Option<User>, Failure>> {
        if let Some(error) = self.before("get-user") {
            return Ok(Err(error));
        }
        Ok(Ok(self.borrow(&db)?.database.users.get(&id).cloned()))
    }
    fn put_department(
        &mut self,
        db: Resource<Session>,
        value: Department,
    ) -> wasmtime::Result<Result<(), Failure>> {
        if let Some(error) = self.before("put-department") {
            return Ok(Err(error));
        }
        self.change(&db, |database| {
            database.departments.insert(value.id, value);
            Ok(())
        })
    }
    fn insert_user(
        &mut self,
        db: Resource<Session>,
        value: Patch,
    ) -> wasmtime::Result<Result<User, Failure>> {
        if let Some(error) = self.before("insert-user") {
            return Ok(Err(error));
        }
        let mut inserted = None;
        let result = self.change(&db, |database| {
            let id = database
                .next_id
                .checked_add(1)
                .ok_or_else(|| failure("ID space exhausted"))?;
            let mut row = User {
                id,
                first_name: None,
                last_name: None,
                email: None,
                department_id: None,
            };
            apply(&mut row, value);
            if row.department_id.is_none() {
                return Err(failure("department_id must not be null"));
            }
            database.next_id = id;
            database.users.insert(id, row.clone());
            inserted = Some(row);
            Ok(())
        })?;
        Ok(result.map(|()| inserted.unwrap()))
    }
    fn update_user(
        &mut self,
        db: Resource<Session>,
        id: i64,
        value: Patch,
    ) -> wasmtime::Result<Result<Option<User>, Failure>> {
        if let Some(error) = self.before("update-user") {
            return Ok(Err(error));
        }
        let mut updated = None;
        let result = self.change(&db, |database| {
            if let Some(row) = database.users.get_mut(&id) {
                apply(row, value);
                if row.department_id.is_none() {
                    return Err(failure("department_id must not be null"));
                }
                updated = Some(row.clone());
            }
            Ok(())
        })?;
        Ok(result.map(|()| updated))
    }
    fn delete_user(
        &mut self,
        db: Resource<Session>,
        id: i64,
    ) -> wasmtime::Result<Result<bool, Failure>> {
        if let Some(error) = self.before("delete-user") {
            return Ok(Err(error));
        }
        let mut deleted = false;
        let result = self.change(&db, |database| {
            deleted = database.users.remove(&id).is_some();
            Ok(())
        })?;
        Ok(result.map(|()| deleted))
    }
}

pub struct Application {
    pub store: Store<State>,
    pub bindings: Usermanager,
}
impl Application {
    pub fn load(bytes: &[u8]) -> wasmtime::Result<Self> {
        let mut config = Config::new();
        config
            .wasm_gc(true)
            .gc_heap_initial_size(8 * 1024 * 1024)
            .wasm_function_references(true)
            .wasm_tail_call(true)
            .wasm_exceptions(true)
            .wasm_component_model(true)
            .wasm_component_model_implements(true)
            .consume_fuel(true)
            .cranelift_opt_level(wasmtime::OptLevel::None);
        let engine = Engine::new(&config)?;
        let component = Component::new(&engine, bytes)?;
        let imports = component
            .component_type()
            .imports(&engine)
            .map(|(name, _)| name.to_owned())
            .collect::<Vec<_>>();
        wasmtime::ensure!(
            imports == ["suss:usermanager/storage@0.1.0"],
            "Unexpected component imports: {imports:?}"
        );
        let mut linker = Linker::new(&engine);
        Usermanager::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state)?;
        let mut store = Store::new(&engine, State::default());
        store.set_fuel(100_000_000)?;
        let bindings = Usermanager::instantiate(&mut store, &component, &linker)?;
        Ok(Self { store, bindings })
    }
    pub fn open(&mut self, path: &str) -> wasmtime::Result<Result<Resource<Session>, Failure>> {
        self.store.set_fuel(100_000_000)?;
        self.bindings
            .suss_usermanager_application()
            .call_open(&mut self.store, path)
    }
    pub fn request(
        &mut self,
        db: &Resource<Session>,
        request: &Request,
    ) -> wasmtime::Result<Result<View, Failure>> {
        self.store.set_fuel(100_000_000)?;
        self.bindings.suss_usermanager_application().call_handle(
            &mut self.store,
            Resource::new_borrow(db.rep()),
            request,
        )
    }
    pub fn changes(&mut self, db: &Resource<Session>) -> wasmtime::Result<u64> {
        self.store.set_fuel(100_000_000)?;
        self.bindings
            .suss_usermanager_application()
            .call_changes(&mut self.store, Resource::new_borrow(db.rep()))
    }
    pub fn close(&mut self, db: Resource<Session>) -> wasmtime::Result<u64> {
        self.store.set_fuel(100_000_000)?;
        self.bindings
            .suss_usermanager_application()
            .call_close(&mut self.store, db)
    }
}

use serde_json::Value;
fn text(value: Option<&Value>) -> wasmtime::Result<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        _ => Err(wasmtime::Error::msg("form fields must be strings or null")),
    }
}
fn field(value: Option<&Value>) -> wasmtime::Result<Option<Option<String>>> {
    match value {
        None => Ok(None),
        other => Ok(Some(text(other)?)),
    }
}
pub fn decode_request(input: serde_json::Value) -> wasmtime::Result<Request> {
    let op = input
        .get("op")
        .and_then(Value::as_str)
        .ok_or_else(|| wasmtime::Error::msg("request requires op"))?;
    Ok(match op {
        "list-users" => Request::ListUsers,
        "departments" => Request::Departments,
        "reset-changes" => Request::ResetChanges,
        "edit-user" => Request::EditUser(match input.get("id") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                value
                    .as_i64()
                    .ok_or_else(|| wasmtime::Error::msg("id must be a signed long"))?,
            ),
        }),
        "delete-user" => Request::DeleteUser(
            input
                .get("id")
                .and_then(Value::as_i64)
                .ok_or_else(|| wasmtime::Error::msg("id must be a signed long"))?,
        ),
        "seed-department" => Request::SeedDepartment(Department {
            id: input["id"]
                .as_i64()
                .ok_or_else(|| wasmtime::Error::msg("id must be a signed long"))?,
            name: input["name"]
                .as_str()
                .ok_or_else(|| wasmtime::Error::msg("name must be a string"))?
                .into(),
        }),
        "save-user" => {
            let form = input
                .get("form")
                .and_then(Value::as_object)
                .ok_or_else(|| wasmtime::Error::msg("save-user requires form"))?;
            Request::SaveUser(Form {
                id: text(form.get("id"))?,
                first_name: field(form.get("first_name"))?,
                last_name: field(form.get("last_name"))?,
                email: field(form.get("email"))?,
                department_id: text(form.get("department_id"))?,
            })
        }
        _ => return Err(wasmtime::Error::msg("unknown operation")),
    })
}
