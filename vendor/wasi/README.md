# Official WASI WIT source lock

`wasi-wit-0.3.1/` is the unmodified content of the official
[WASI v0.3.1 release archive](https://github.com/WebAssembly/WASI/releases/tag/v0.3.1).
The release tag resolves to `59e48bfe3fae9bf2480eb15abd8f55999eb3b395`.
[The source lock](../../docs/roadmap/wasi-wit-lock.json) records the download URL,
archive SHA-256, every WIT file hash, package declarations and local dependencies.
[LICENSE.md](LICENSE.md) is copied from that upstream revision. The upstream W3C
Community Contributor License Agreement applies; these files are not relabeled
as Suss MIT/Apache code.

Each directory is a separately resolvable package with its own dependency copies.
Some dependency files deliberately contain a subset of the primary package's
interfaces. Preserve them byte for byte. Package versions are taken from source,
never rewritten to match the release label (this archive happens to use 0.3.1
for all six packages).

```sh
python3 scripts/wasi_lock.py
# Optional: verify a downloaded release archive against the lock and source files
python3 scripts/wasi_lock.py --archive /path/to/wasi-wit-0.3.1.tar.gz
python3 scripts/wasi_lock.py --wasm-tools /path/to/wasm-tools-1.258.0 \
  --output docs/roadmap/wasi-wit-probes.json
```

The first command verifies tracked file content, identities and dependency
closure offline. The second uses upstream tooling to resolve each graph and
validate/decode its binary WIT package. Neither executes WASI capabilities.
The prototype compiler still uses `crates/suss-compile/src/wasi/`; this source
lock is the replacement input, pending compiler-tools migration and executing
binding tests. No bundled prototype WIT is deleted yet.
