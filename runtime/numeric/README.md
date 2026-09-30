# Private numeric conversion build input

This allocator-free Rust core Wasm is embedded into the generated shared GC
runtime. It has no host imports, JavaScript, WASI, table or start function.
It implements primitive StringToNumber and NumberToString for arithmetic coercion;
it is not a replacement for public core definitions or object conversion.

Decimal parsing uses Rust core's correctly rounded binary64 parser after an
original ECMAScript grammar and whitespace gate. Unsigned hexadecimal/octal/binary
parsing rounds the significant bits once, using guard/sticky bits and ties to even.
Formatting uses pinned ryu-js 1.0.3. See [NOTICE](NOTICE) and the retained licenses.
The semantic reference is ClojureScript c4295f303100bbf5afac449242d30bca1126f1a1;
no upstream core forms are ported here.

The Rust toolchain, dependency checksum, source/license hashes and Wasm hash are
pinned in artifact/manifest.json. From the repository root:

```sh
python3 scripts/numeric_runtime.py --check
python3 scripts/numeric_runtime.py --rebuild-check
```

Rebuilding requires the pinned Rust 1.98.0 wasm32-unknown-unknown target.
`--record` rebuilds and records intentional changes; review every source/binary
and manifest diff. Builds use an isolated temporary target directory and preserve
the developer environment flags. CI verifies hashes and executes the actual
embedded helper through the GC runtime; the local rebuild check additionally
requires byte-identical compiler output. No JVM/Node is needed to ship or execute.

The encoder validates the trusted artifact, appends its ordinary function types
after the unchanged recursive GC prelude and relocates type/global references.
Its private memory contains Rust stack and static constants. Scratch starts after
the initial memory and grows to a checked high water mark; it is reused on each
non-reentrant conversion. GC strings are copied into scratch, never mutated.
Before every helper call the Rust stack pointer is reset, recovering after fuel
traps. Allocation failure throws a typed language exception. This memory is not
the canonical component allocator; memory ownership/free/post-return are still
M5 work. Native session reset replaces the memory together with the Store.

The 210-case source corpus and 1,024-sample formatting/round-trip matrix execute
against fresh pinned ClojureScript observations and independently decoded GC
results. These bounded tests do not certify all binary64 inputs or public core
compatibility. Objects/closures requiring primitive conversion remain unsupported.
