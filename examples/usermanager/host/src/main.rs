use serde_json::json;
use std::io::{self, BufRead};
use suss_usermanager_host::{Application, decode_request};
fn main() -> wasmtime::Result<()> {
    let mut args = std::env::args().skip(1);
    let component = args.next().ok_or_else(|| {
        wasmtime::Error::msg("usage: suss-usermanager-host COMPONENT STORAGE.json|:memory:")
    })?;
    let storage = args
        .next()
        .ok_or_else(|| wasmtime::Error::msg("a storage path is required"))?;
    wasmtime::ensure!(args.next().is_none(), "unexpected argument");
    let mut app = Application::load(&std::fs::read(component)?)?;
    let db = app
        .open(&storage)?
        .map_err(|error| wasmtime::Error::msg(error.message))?;
    for line in io::stdin().lock().lines() {
        let line = line?;
        let input = serde_json::from_str(&line)
            .map_err(wasmtime::Error::from)
            .and_then(decode_request);
        match input {
            Ok(request) => println!("{}", serde_json::to_string(&app.request(&db, &request)?)?),
            Err(error) => println!(
                "{}",
                json!({"Err":{"kind":"Invalid","message":error.to_string()}})
            ),
        }
    }
    app.close(db)?;
    eprintln!(
        "{}",
        json!({"opened":app.store.data().opened,"closed":app.store.data().closed,"live":app.store.data().live,"calls":app.store.data().calls,"writes":app.store.data().writes})
    );
    Ok(())
}
