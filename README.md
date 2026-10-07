# Suss

Suss is an experimental Clojure dialect compiling to WebAssembly, named after
Julie and Gerald Jay Sussman. Its resurrection targets portable **ClojureScript**
semantics, WASI 0.3.1 commands and WIT libraries, and browser ES modules.

The current compiler is a prototype. It does not yet implement that target:
WIT support is partial, the persistent compiled REPL is incomplete, and the core library has
known semantic gaps. See the [accepted specification](docs/design/suss-0.3.1.md),
[roadmap](ROADMAP.md) (status in [milestones](https://github.com/bobby/suss/milestones)), [compatibility inventory](docs/compatibility/README.md)
and [latest implementation evidence](docs/roadmap/handoff.md).

## Development

Initialize the pinned ClojureScript reference with `git submodule update --init
clojurescript`. Rust edition 2024 is required; use the checked-in Cargo.lock.

```sh
cargo run -p suss-cli -- -e '(+ 1 2 3)'
cargo run -p suss-cli -- --help
cargo test --workspace --locked -- --test-threads=2
python3 scripts/cljs_inventory.py --check
```

The native CLI embeds Wasmtime. External `wasmtime` and `wasm-tools` are useful
for artifact inspection. A JVM is not required to build or run Suss. The planned
ClojureScript differential oracle may use Java and Node during development.

The core compiler and Rust source use the repository license. The ClojureScript
reference and any future imported forms retain their upstream EPL notices.
Contributors and coding agents should read [AGENTS.md](AGENTS.md).

The [bootstrap core import](docs/compatibility/CORE-IMPORT.md) packages its
ClojureScript-derived source and byte-preserved EPL notices/licenses separately.
