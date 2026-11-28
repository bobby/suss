# UserManager component and Rust CLI

This adapts Sean Corfield's UserManager model/controller at the exact revision
and Apache-2.0 provenance in `provenance.json`. Shared `app.cljc` contains the
Database record/Lifecycle/IFn implementation, guest atoms, form conversion,
transient namespaced maps, insert/update choice, department join and name sort.
Rust supplies typed storage and JSON-line transport. The rendered result is a
typed view model instead of Ring/Selmer HTML. Storage is a synchronous JSON-file
adapter for the original JDBC/SQLite capability, with one writer per file.

From the repository root:

```sh
python3 examples/usermanager/build.py
cargo +stable run --profile test --locked -p suss-usermanager-host -- target/usermanager/application.wasm target/usermanager/storage.json
```

Send one JSON request per line on standard input. End input to explicitly close
the owned session. The CLI writes results to stdout and storage/resource counters
to stderr. It reopens an existing storage file; use `:memory:` for ephemeral data.

```json
{"op":"seed-department","id":4,"name":"Development"}
{"op":"save-user","form":{"first_name":"λ🦀","last_name":"Example","department_id":"４"}}
{"op":"list-users"}
{"op":"edit-user","id":1}
{"op":"save-user","form":{"id":"1","email":null,"department_id":"4"}}
{"op":"delete-user","id":1}
{"op":"reset-changes"}
```

`departments` lists departments by name. `edit-user` without an ID returns the
new-user form. Saved form fields are strings or null. Missing name/email fields
remain absent in updates; explicit null clears a field. Missing/empty form IDs
or zero choose insertion; nonzero signed IDs choose update. IDs stay exact signed
longs, including beyond 2^53; a missing update target does not insert a row.

The storage WIT interface exposes raw rows and CRUD operations. Joining and
sorting users happens in the guest; users without a matching department are
excluded from the list, while editing can still read them. The controller
increments its per-session change counter before parsing and storage. Nonempty
malformed/overflowing numeric input returns a typed parse failure without a
write. Structured host storage failures retain the counter increment.

The checked-in scenario and JVM observations use exactly the same application
namespace under Clojure1.12.6, with an explicit development-only storage adapter.
Regenerate the observations with:

```sh
python3 examples/usermanager/oracle/generate.py
cargo +stable test -p suss-cli --locked --test usermanager_application -- --test-threads=2
```

The scenario checks ordering, Unicode, exact IDs, missing/zero/negative IDs,
partial updates, null values, repeated updates, missing users, parse and storage
failure effects, resets and persistence reopening. Its fault-injection metadata
is a test capability rather than a CLI application operation. The JVM dependency
is development-only; shipped guest/host code does not use Java.

The first recorded component comparison executes all29 shared JVM observations
and reopens persistent storage with retained rows, two closes and zero live
resources. Its source-qualified commands and limitations are recorded in
`../../results/m5-wit-adapters/nested-associative-live-lookup-bootstrap-order/`.
These focused controls are separate from the larger
lifetime stress and application cost measurements.

The application regression target also runs 10,000 mixed operations with isolated
sessions and moved/closed/escaped-borrow controls. A separate copy of the actual
component receives a read-only test observer over its existing canonical
allocator. Its live allocation count/bytes return to the initial zero baseline
after each call, GC and final close; the original component runs the same stress
separately. The observer is not a shipped export.

Reproduce application measurements separately from CI correctness tests:

~~~sh
cargo +stable build --profile test --locked -p suss-cli -p suss-usermanager-host --bins
python3 examples/usermanager/measure.py --suss target/debug/suss \
  --host target/debug/suss-usermanager-host --measure target/debug/measure \
  --output target/usermanager/measurements
~~~

Use a new output directory. The default command performs three fresh compiler
cache builds and cache-reuse builds, then three processes per runtime for
datasets of16,1,000 and10,000 users. Each process executes five sorted joined
lists, five1,000-call counter batches and five100-chain
create/read/partial-update/delete batches. It also executes the JSON-line host,
a malformed form and persistent-file reopening. Raw logs/JSON, exit status,
source/component/executable hashes, hardware/load/cgroup limits, inclusive
startup/operation times, callback time/counts and peak RSS remain in the output.
Run examples/usermanager/summarize.py on that directory to regenerate spreads.

This builds the optimized test profile (workspace opt-level1, dependencies opt-level2) and uses Wasmtime unoptimized
component compilation. Fresh compiler caches do not drop OS/filesystem caches.
The JVM comparison needs the development Clojure CLI/JDK and uses the shared
namespace with its persistent-map memory adapter; Rust serializes the database
before writes for rollback and can persist JSON. Callback instrumentation and
all startup/storage costs stay included, without subtraction. The ordinary
artifact exports no allocation observer; exact canonical live-counter evidence
comes from the separate instrumented lifetime run. These are machine-specific
baselines, not universal speed claims.

The Rust example gives each application request a bounded two-billion-fuel
allowance. The recorded10,000-user joined view uses about1.09billion after
canonical allocation was indexed; the earlier100-million example limit was
insufficient for this complete guest request. This host policy does not change
compiler budgets or the allocator's unchanged100-million-fuel regression
budget. Startup/open/change-counter/close calls retain their separate limits.
