# Global call-site size audit

These are controlled, executed historical artifacts immediately before the typed
calling-convention implementation and at its merge (#370 / ADR-0004). Exact
commits and conditions are in `method.json`; every before/after source digest
matches. The harness is outside the compiler's fingerprinted sources, and the
historical compiler sources and lockfile are unchanged.

Each program includes actual shipped core and its IFn catalog. Its exported
function makes 0, 1, 2 or 10 calls through a global var; each call increments a
counter. All sixteen components validate, instantiate without host imports, and
return the expected counter through a typed canonical call. The receipts retain
component and source SHA256 values; actual artifacts remain in the root
`target/probe/m11-call-history-full-{before,after}` directories.

| Construct | Before code bytes | After code bytes |
| --- | ---: | ---: |
| First zero-argument global call | 225 | 84 |
| Subsequent zero-argument calls, average from 2 to 10 | 226.375 | 84 |
| First four-argument global call | 289 | 124 |
| Subsequent four-argument calls, average from 2 to 10 | 311.875 | 124 |

These are whole-function executable code differences, including var reads,
literal operands, temporaries and local-declaration encoding. The larger legacy
cases cross local-index encoding boundaries. They are not the roughly 30-byte
closure-check/invoke instruction sequence alone. The current ABI 7 regression executes the same counter pattern with dynamic vars:
eight additional calls cost 536 bytes at arity 0 (67 per call) and 792 at arity 4
(99 per call), below the 160/220-byte bounds. Structural tests confirm no argument
array or universal invoke on the call site, and the JVM-referenced convention
suite passes all three cases. Logs identify their actual source separately.

| Artifact | Before bytes | After bytes |
| --- | ---: | ---: |
| Runtime bootstrap | 2,493,556 | 1,905,352 |
| Macro bootstrap | 2,489,492 | 1,901,288 |
| Bootstrap executable bodies, either phase | 2,315,666 | 1,556,333 |
| Complete component, zero-argument callee, 0 calls | 2,798,704 | 2,213,213 |
| Complete component, zero-argument callee, 10 calls | 2,801,071 | 2,214,081 |
| Complete component, four-argument callee, 0 calls | 2,798,751 | 2,213,267 |
| Complete component, four-argument callee, 10 calls | 2,801,930 | 2,214,539 |

The original issue's earlier 280–490-byte estimates are not used as this
experiment's baseline. An initial bare-Environment control omitted core/IFn and
therefore skipped the old source-level callable protocol; it is not representative
of ordinary compiled programs. Its 59/91 → 84/124-byte results remain separately
under `target/probe/m11-call-history-{before,after}` and are not reported as a
size improvement.

The issue owner explicitly folded #313 into #353 / ADR-0004. That accepted design
also corrects Clojure arity and evaluation-order behavior; the earlier incorrect
behavior is not an oracle. Current Clojure calling-convention and structural
suites remain the semantic gates. This receipt is artifact/size evidence, not a
claim of general Clojure compatibility or a fresh complete workspace pass.

To reproduce, add `probe.rs` temporarily as
`crates/suss-compile/examples/m11_call_size.rs` in detached worktrees at the two
recorded commits and run `cargo +stable run --locked -p suss-compile --example
m11_call_size -- <output-directory>`. The harness caches native code only in the
output directory's parent, uses the pinned historical Wasmtime, and records both
actual execution and emitted sizes. No Java or Node runtime is used.
