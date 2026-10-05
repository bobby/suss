# Public compiler pipeline migration

The native `Compiler::compile(source, wit)` entry now uses the compiled source
macro host and portable AOT pipeline. It resolves one unambiguous WIT world and
checks implemented adapter capabilities before any Macro-phase execution. It
prepares source and dependencies using the default `src` search path, infers
unambiguous freestanding `^:export` mappings, and validates component assembly.
Runtime initializers execute only when the component is instantiated.

Macros receive the same actual lexical `&env` data as the native session. Ordinary
`defn` executes as a compiled macro in the isolated Macro Store. Its adaptation
preserves docs, attribute precedence and argument lists, and uses existing native
function lowering for fixed and variadic signatures. Core aliases, lexical
shadowing, core exclusions and user-defined macro precedence have executing coverage. Source attribution, hashes, adaptations and
the distributed EPL text are recorded in
[the provenance record](../compatibility/compiled-defn-provenance.json).
Complete portable `defn` and analyzer-schema acceptance remain open.

The public component regressions independently decode typed scalar results,
retain atom state across calls and GC, and inspect the actual language exception
payload from a deferred throwing initializer. Deep loop and function recurrence
execute at least100000 iterations under an explicit2MiB Wasm stack limit, with
simultaneous argument swapping and bounded fuel per call. Scalar async exports
execute through the canonical async adapter. This does not establish source
suspension, rooted continuations or cancellation during pending I/O.

Unsupported WIT shapes produce named capability diagnostics before effectful
macros run. Mapping and component failures retain separate error categories.
Pure components have no hidden printing import. Generic WIT adapters remain M5
work; unsupported shapes are still tested explicitly. Wasm-host macro execution
for this public entry is reported unsupported rather than invoking a fallback.

The native `Compiler::compile_for_main(source, namespace)` now uses shared
compiled command preparation. It validates the requested leading namespace
before executing macros and resolves dependencies from `src`. File-mode command
compilation delegates to the same internal helper while retaining its source
path. The artifact exports the official asynchronous `wasi:cli/run@0.3.1`
function and imports the reachable environment/exit capabilities. `-main`
receives only user argument strings. Normal values, including numbers, false,
nil and NaN, complete successfully; uncaught exceptions fail. Explicit exit
uses the checked u8 status policy, with invalid statuses catchable as language
errors. Runtime initialization remains deferred to artifact instantiation.

Seven new command regressions execute the public library output, covering real
lexical macro data, ordered empty/Unicode arguments, phase capability isolation,
normal and explicit exit behavior, retained state across repeated typed calls
and GC, and independently decoded initializer exception payload42. Six failed
on the unchanged parent and pass on the replacement; the phase-isolation test
already passed. Existing typed and CLI command fixtures also pass. Independent
review, the full baseline and final-head CI remain required for this increment.

Other public methods, including file, namespace, project, expression and
cache entry points, still use prototype paths. Production evaluator retirement
is incomplete. Native file/project preparation shares the new capability
preflight, but that does not migrate `Compiler::compile_files` or its peers.
Issues #12–#15 and M3 remain open. Each increment requires independent review,
the unfiltered workspace baseline and final reviewed-head CI before readiness.

Focused commands:

```sh
cargo test -p suss-compile --locked --test public_compiled_pipeline -- --test-threads=2
cargo test -p suss-compile --locked --test toolchain_profile -- --test-threads=2
cargo test -p suss-compile --locked --test portable_command -- --test-threads=2
cargo test -p suss-cli --locked --test public_compiled_command -- --test-threads=2
cargo test -p suss-cli --locked --test compiled_official_command -- --test-threads=2
sh scripts/verify-bootstrap.sh
```

The handoff records actual failures and terminal results. Next migrate the
remaining public artifact boundaries while preserving executing value assertions,
then remove the production evaluator and complete pending-I/O lifecycle and
live-memory accounting. No issue closure follows from these focused tests.
