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

Other public methods, including file, namespace, main, project, expression and
cache entry points, still use prototype paths. Production evaluator retirement
is incomplete. Native file/project preparation shares the new capability
preflight, but that does not migrate `Compiler::compile_files` or its peers.
Issues #12–#15 and M3 remain open. Each increment requires independent review,
the unfiltered workspace baseline and final reviewed-head CI before readiness.

Focused commands:

```sh
cargo test -p suss-compile --locked --test public_compiled_pipeline -- --test-threads=2
cargo test -p suss-compile --locked --test toolchain_profile -- --test-threads=2
sh scripts/verify-bootstrap.sh
```

The handoff records actual failures and terminal results. Next migrate the
remaining public artifact boundaries while preserving executing value assertions,
then remove the production evaluator and complete pending-I/O lifecycle and
live-memory accounting. No issue closure follows from these focused tests.
