# Compiled official command draft

Native `suss compile app.sus --main app -o app.wasm` prepares source through the
shared portable pipeline, with isolated compiled macros and ordered Runtime
dependencies. The source namespace must match the selected entry namespace.
`--src` supplies dependency roots. WIT/project selection flags conflict with this
fixed command profile and fail before replacing an output artifact.

The component exports `wasi:cli/run@0.3.1#run` with the official asynchronous
`func() -> result` shape. Default `suss run app.wasm -- ...` selects that exact
interface. Explicit `--invoke wasi:cli/run@0.3.1#run` and the `run` alias
select the same command when there is no freestanding `run` export. The host supplies the artifact path as argv[0], including when no
user arguments are supplied. The adapter excludes argv[0] and passes the remaining
strings, in order, to the live source `-main` function. Empty strings and Unicode
are preserved. Ordinary completion succeeds regardless of the returned value;
an uncaught source exception fails. A returned number is not a process exit code.

Command sources can explicitly call `(wasi.cli/exit-with-code status)` or require
`wasi.cli` with an alias. The status must be an integer from 0 through 255;
invalid numbers and other values raise a catchable language error. The official
host exit effect stops the instance and native runner with that status, including
when called during Runtime initialization. This capability is available only to
the Runtime phase; compiled macros cannot acquire it or execute a Runtime initializer.

The complete pinned CLI dependency graph is embedded and checked against
[the release lock](../roadmap/wasi-wit-lock.json). Reachable component value and
canonical function types come from the resolved upstream graph. This command
imports the environment argument function and, when explicitly bound by command
preparation, the exit-with-code function. The WIT source and
[upstream license](../../vendor/wasi/LICENSE.md) are recorded in the lock; the
Rust adapter and allocator are original code.

Canonical UTF16 buffers live in a separate linear memory. Each argument is copied
into an owned, rooted language string before its canonical buffer is released.
The allocator checks alignment, ownership, memory32 arithmetic and growth failure,
copies the preserved prefix when moving allocations, and reuses freed capacity.
Source cells can retain argument strings across later calls and GC independently
of canonical memory reuse. This does not measure live language heap usage or
implement the M3 leak-accounting gate.

Focused execution evidence currently includes three command compiler tests, five native
command tests, all 31 affected native CLI tests, four allocator/core unit tests,
both phase image reproductions without Java, four bootstrap execution tests and
118 Python tests. Compiler tests execute the exact async result type in fresh
Stores, repeat calls after GC, and verify retained Unicode strings while later
argument buffers are reused. Native tests also execute compiled macros and source
dependencies and preserve prior output on selection errors. See
[the handoff](../roadmap/handoff.md) for actual commands and failure/fix history.

The exit tests execute status 0/73/255 and initializer status19, catch invalid
negative/out-of-range/fractional/nonfinite/nil/boolean values, and reject capability
access in the compiled Macro phase while preserving the previous artifact.

This remains a draft. Remaining WASI capabilities,
source suspension and rooted continuations, component-host migration, evaluator
retirement, published dependencies and original M3 pending-I/O/lifecycle gates
remain open. Independent review, the unfiltered workspace baseline and final-head
CI are still required before publishing this change as ready. No M3 issue is closed.
