# Persistent atom storage

The native compiled REPL provisions the provenance-tracked core artifact once in
its session Store. `Session::new_repl()` exposes the same profile to native
clients; `Session::new()` retains its minimal bootstrap profile. Reset constructs
and provisions a replacement before discarding the old Store. No input history
is replayed. An unsuccessful replacement leaves the old session usable.

This supplies real descriptor-backed `Atom` objects with GC-owned state and the
pinned state/meta/validator/watches fields. One-argument `atom`, `deref`/`@`,
`reset!` and the two/three/four-argument `swap!` signatures execute compiled code.
State may change type, including nil, false, arrays and closures. Aliases and
captured atom references observe later mutations. Captured old contents retain
their original identity and behavior after replacement, rebinding and GC.
`IAtom`, identity equality/hash, dereference and metadata reads are installed.

Operands execute once in textual order. A throwing swap callback preserves the
atom state while retaining earlier arbitrary effects. A failed definition
initializer still follows the existing session binding-preservation contract.

This is partial atom support, not complete portable atom compatibility. Atom
options, validators, watches/IWatchable, protocol fallback, variadic swap!,
reset-vals!, swap-vals! and compare-and-set! remain unimplemented. Non-nil validator
or watch fields explicitly reject reset before writing state; they are not
silently ignored. Unsupported signatures produce language arity errors. General
object printing is still incomplete, so displaying an Atom itself reports the
existing unsupported-object display error; dereferenced scalar contents display.
Full validators/watches/CAS remain M7 work. This does not complete M3 namespaces,
compiled macros, cancellation or live-memory accounting.

Five pinned source selections (IAtom, Atom, atom, reset!, swap!) retain original
extractions, source hashes and EPL-1.0 notices. The four adaptations are in
`docs/compatibility/patches/atoms-*.json`. The overlay marks all five in-progress;
its arities describe the reference API, not supported signatures. The imported
artifact contains 116 selections/120 licensed files. Loading the artifact does
not certify its pending dependencies or complete core functionality.

Executing evidence:

- `cargo test -p suss-cli --locked --test persistent_repl --test portable_atoms -- --test-threads=2`: eight actual command tests and four native tests, no ignores.
- `scripts/test-atom-oracle.sh`: 28 fresh pinned ClojureScript/Node observations match exact independently decoded native Boolean/Number values. The corpus covers state, aliases, rebinding, old captures, fixed swap signatures, identity/hash stability, operand order and callback failure effects. Native collection occurs between every case.
- Additional native regressions retain owners and old closures through fragment compilation/GC; decode exact error messages for unsupported validator/watch state, signatures and invalid receivers; verify reset provisioning, stale-handle rejection and preservation after failed provisioning.

The pre-implementation command regression failed with located unresolved
atom/deref/swap!/reset! diagnostics. No constructor fixture or opaque-object
success decoder substitutes for real state storage. Full workspace, independent
PR review and final-head CI gate publication readiness separately.
