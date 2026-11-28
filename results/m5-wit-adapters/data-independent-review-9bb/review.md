# Independent review of #523 / #524

Reviewed commit: `9bb6c3dcf8d704a141d571a8013ec4e40afd47ba`.
Scope: the bounded exported-data-codec acceptance criteria in #524, not all M5 work.
Disposition: request changes for the two P2 findings below.

The review read the accepted design, roadmap, compatibility inventory, handoff,
issue/PR contracts, recursive codec and component type encoding, focused tests,
and full-baseline receipts. The source at the reviewed head is identical to
8788d7a7; its additional changes publish documentation/evidence. The recorded
baseline is 2,759 passed, zero failed and 15 existing explicit ignores across
393 Rust/doc-test summaries. The explicitly included Binaryen control also
passed. I did not rerun that five-hour baseline. Final-head CI remains a
separate promotion requirement.

No tracked files, commits or GitHub state were changed by this review. All
probe files and outputs are confined to this ignored directory. No RUSTFLAGS
override was used. The inspected CLI binary SHA-256 was
`765aae8de0686aaf79538d5e6febb58fa356ac0766ef13a628d3e4af5a01672b`.

## Findings

### P2: schema-only imported data aliases are rejected

At the reviewed commit, `crates/suss-compile/src/portable/aot.rs:379` rejects
every non-random imported interface. This includes an interface containing only
supported record types and no functions/resources. `validate_boundary` then
returns the unsupported-import diagnostic at lines 394–397.

`aliases.wit` contains:

```wit
package test:review-aliases;
interface data {
  record inner { v: u8 }
  record outer { x: inner }
}
world api {
  use data.{outer, inner as late};
  export run: func(x: outer, y: late) -> outer;
}
```

`aliases.sus` defines `(ns app) (defn ^:export run [x y] x)`.
The exact compilation below exits 1 with:

```text
Compilation error: Unsupported feature: Portable AOT imported WIT adapters remain unimplemented at bytes 0..0
```

This is a data-provider/alias acceptance gap, not a request for imported function
authority. A repair should admit supported function-free data providers before
source macro effects and preserve the selected interface/type identities and
aliases. Removing the guard while silently erasing the provider is insufficient.
Add an executing Rust roundtrip, including declared/versioned/renamed aliases.

### P2: fixed numeric lists omit the packed-array output rule

`composite.rs:250–258` represents `FixedLengthList` as an ordinary non-record
`Product`. Its projector at lines 460–474 only invokes persistent-vector
normalization; it bypasses the numeric packed-array path used by dynamic lists.

`fixed-packed.sus` defines:

```clojure
(ns app)
(defn ^:export run [] (byte-array [-1 1 2]))
```

The WIT declaration is:

```wit
package test:review-fixed;
world api {
  type bytes = list<u8, 3>;
  export run: func() -> bytes;
}
```

Compilation succeeds and validates the component. The actual CLI call exits 1.
The separate Rust/Wasmtime inspector confirms an uncaught component exception
with the original pending payload:

```text
WIT list requires a persistent vector
```

Under the documented numeric-list packed-array rule and #524 criterion 3, the
expected result is `[255 1 2]`. Distinguish fixed lists from tuples, retain exact
length and packed-kind checks, and preserve unsigned byte bits. Do not broaden
tuple acceptance to arbitrary arrays as a side effect. Add malformed-length and
wrong-kind controls alongside the positive fixed-list case.

## Other probes and review limits

- The `ops` component redefines public core `<` and `+`, then returns a generic
  list of tuples. It compiles and executes successfully, producing
  `List([Tuple([U8(1), U8(2)])])`. This did not confirm the suspected arithmetic
  redefinition-isolation issue.
- `root-aliases.wit` defines world-local nested records and a later alias of the
  inner record. It compiles and validates successfully. This probe did not
  execute a value roundtrip and is not described as doing so.
- No other confirmed findings arose from inspection of recursive layouts/flat
  joins, staged nominal exports, complete validation before canonical output
  allocation, scalar/packed handling, successful cleanup and the evidence audit.
- The probes are focused witnesses, not exhaustive interoperability or a new
  full-workspace acceptance run. Fixes require their appropriate regressions,
  renewed review and passing final-head CI.

### Additional requested map-key preflight check — no third finding

The parent requested a check of unsupported map keys with a deliberately
throwing source macro. `bad-map-key.sus` contains:

```clojure
(ns app)
(defmacro effect [] (throw "review-macro-ran"))
(defn ^:export run [] (effect))
```

Both `map<f64, u8>` and `map<key, u8>` with `enum key { a, b }` exit 1 during
WIT resolution, before source macro effects. Their diagnostics are:

```text
invalid map key type: map keys must be bool, u8, u16, u32, u64, s8, s16, s32, s64, char, or string
```

The `review-macro-ran` marker is absent. These public-frontend observations do
not establish that every caller-supplied Resolve graph has the same guard, but
they do not reproduce an additional acceptance failure. The exact commands and
raw logs are retained below. `bad-map-key-pure.sus` was prepared but not executed.

## Separate, unconfirmed #21 follow-up

This is **not a #524 finding** and was not executed on the newer resource tree.
Data map lifting intentionally keeps the last duplicate key. With two distinct
owned resource handles under duplicate keys, coalescing during input COPY might
make the overwritten owner unreachable before a Source lexical scope starts.
GC must not substitute for its destructor. The parent was asked to test that
scenario independently on the resource branch; no resource failure or fix is
claimed by this review.

Subsequent parent follow-up: the parent reports that an actual resource component
with a map value containing handles is rejected by the pinned Wasmtime
51.0.0-rc.1 validator before any call. The cited primary validator is
wasmparser 0.263.0, component.rs:4044–4047, which explicitly disallows handle
values because duplicate-key insertion can drop elements. Thus the proposed
runtime owned-map leak is ruled out for that validated path; the separate
resource task is to diagnose the unsupported shape before source effects.
That follow-up was performed by the parent, not by this data-codec review.

## Exact executed commands

All commands below used this working directory:

```text
/home/bobby/.codex-workspaces/worktrees/ios-bc59f46b-2529-47d6-9d06-a888fafcdd03/suss/target/probe/m5-composites
```

The `.sus`, `.wit` and `inspect.rs` inputs are retained beside this report. The
commands are the literal compile/run invocations used during review, including
scope limits and redirections. Commands creating those retained input files are
not needed to reproduce them from this directory.

### Core-redefinition witness — compile exit 0, run exit 0

```sh
env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus systemd-run --user --scope -p MemoryHigh=3G -p MemoryMax=4G -p CPUQuota=100% target/probe/cargo/debug/suss --build-dir target/probe/review-524/cache compile target/probe/review-524/ops.sus -w target/probe/review-524/ops.wit --export run=app/run -o target/probe/review-524/ops.wasm > target/probe/review-524/ops-compile.stdout 2> target/probe/review-524/ops-compile.stderr

env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus systemd-run --user --scope -p MemoryHigh=3G -p MemoryMax=4G -p CPUQuota=100% timeout 90s target/probe/cargo/debug/suss run target/probe/review-524/ops.wasm --invoke run > target/probe/review-524/ops-run.stdout 2> target/probe/review-524/ops-run.stderr
```

### Schema-only provider witness — compile exit 1

```sh
env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus systemd-run --user --scope -p MemoryHigh=3G -p MemoryMax=4G -p CPUQuota=100% target/probe/cargo/debug/suss --build-dir target/probe/review-524/cache compile target/probe/review-524/aliases.sus -w target/probe/review-524/aliases.wit --export run=app/run -o target/probe/review-524/aliases.wasm > target/probe/review-524/aliases-compile.stdout 2> target/probe/review-524/aliases-compile.stderr
```

### World-local nested/later alias witness — compile exit 0

```sh
env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus systemd-run --user --scope -p MemoryHigh=3G -p MemoryMax=4G -p CPUQuota=100% target/probe/cargo/debug/suss --build-dir target/probe/review-524/cache compile target/probe/review-524/aliases.sus -w target/probe/review-524/root-aliases.wit --export run=app/run -o target/probe/review-524/root-aliases.wasm > target/probe/review-524/root-aliases-compile.stdout 2> target/probe/review-524/root-aliases-compile.stderr
```

### Fixed packed numeric list witness — compile exit 0, run exit 1

```sh
env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus systemd-run --user --scope -p MemoryHigh=3G -p MemoryMax=4G -p CPUQuota=100% target/probe/cargo/debug/suss --build-dir target/probe/review-524/cache compile target/probe/review-524/fixed-packed.sus -w target/probe/review-524/fixed-packed.wit --export run=app/run -o target/probe/review-524/fixed-packed.wasm > target/probe/review-524/fixed-packed-compile.stdout 2> target/probe/review-524/fixed-packed-compile.stderr

env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus systemd-run --user --scope -p MemoryHigh=3G -p MemoryMax=4G -p CPUQuota=100% timeout 90s target/probe/cargo/debug/suss run target/probe/review-524/fixed-packed.wasm --invoke run > target/probe/review-524/fixed-packed-run.stdout 2> target/probe/review-524/fixed-packed-run.stderr
```

### Pending-payload inspector — build exit 0, inspector exit 0

The inspector intentionally observes the failed component call and prints its
pending exception; its process exit 0 is **not** a passing fixed-list call.

```sh
env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus systemd-run --user --scope -p MemoryHigh=3G -p MemoryMax=4G -p CPUQuota=100% rustup run stable rustc --edition=2024 target/probe/review-524/inspect.rs --extern wasmtime=target/probe/cargo/debug/deps/libwasmtime-6231b9cb28d7d6e4.rlib -L dependency=target/probe/cargo/debug/deps -o target/probe/review-524/inspect > target/probe/review-524/inspect-compile.stdout 2> target/probe/review-524/inspect-compile.stderr

env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus systemd-run --user --scope -p MemoryHigh=3G -p MemoryMax=4G -p CPUQuota=100% target/probe/review-524/inspect > target/probe/review-524/inspect.stdout 2> target/probe/review-524/inspect.stderr
```

### Requested invalid map-key checks — both compile exits 1 at WIT resolution

```sh
env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus systemd-run --user --scope -p MemoryHigh=3G -p MemoryMax=4G -p CPUQuota=100% target/probe/cargo/debug/suss --build-dir target/probe/review-524/cache compile target/probe/review-524/bad-map-key.sus -w target/probe/review-524/bad-map-key-f64.wit --export run=app/run -o target/probe/review-524/bad-map-key-f64.wasm > target/probe/review-524/bad-map-key-f64-compile.stdout 2> target/probe/review-524/bad-map-key-f64-compile.stderr

env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus systemd-run --user --scope -p MemoryHigh=3G -p MemoryMax=4G -p CPUQuota=100% target/probe/cargo/debug/suss --build-dir target/probe/review-524/cache compile target/probe/review-524/bad-map-key.sus -w target/probe/review-524/bad-map-key-enum.wit --export run=app/run -o target/probe/review-524/bad-map-key-enum.wasm > target/probe/review-524/bad-map-key-enum-compile.stdout 2> target/probe/review-524/bad-map-key-enum-compile.stderr
```
