# Shared script and AOT source preparation

`portable_aot::prepare_source` accepts complete source text, an optional source
path and source roots. It returns prepared Runtime fragments for component
assembly. Native script execution and AOT source preparation share the reader
and staged compilation routine: namespace/dependency preparation, source spans,
compiled top-level macro definitions, expansion and declaration rollback.

The AOT host starts with the shipped Runtime compiler catalog and an isolated
compiled Macro session. Runtime source initializers execute when the resulting
component is instantiated. Source preparation requires compiler facts and never
constructs a Runtime Session. The temporary Macro session can be dropped once
expansion has produced owned compiler artifacts.

Runtime dependencies are compiled in dependency order and included before their
input fragment. Their Wasm retains its own immutable source/phase identity;
staged catalogs supply the shared cells used by component assembly. AOT starts
with suss.core provided and the user namespace selected, then follows explicit
source namespace declarations. Root source paths retain the caller's supplied
identity; dependency lookup records canonical filesystem paths and exact text
hashes. These catalogs come from compilation rather than raw-Wasm reconstruction.

Two executing tests pass. An inline source macro expands code that references a
real Runtime dependency through its alias; the component preserves macro/path
provenance, once-only initialization, subsequent call effects and GC. A second
source prepares successfully despite an unconditional Runtime throw; actual
component instantiation preserves independently decoded first payload17 rather
than later99. The first dependency-path assertion assumed a noncanonical temp
path; checking the loader contract corrected that test setup and strengthened
its exact source-hash assertion. Production lookup behavior stayed unchanged.

Twenty-four existing AOT/native entrypoint, macro, phase and namespace tests pass,
including compile-failure rollback of macro bindings and preservation of completed
dependencies/effects. Both bootstrap images reproduce byte-for-byte without Java,
bootstrap4 and Python118 pass. This CLI-only change leaves the compiler-source
fingerprint and shared ABI layout unchanged. Independent review added a third executing check: a diamond dependency graph
remains deduplicated across repeated source namespace imports, dependency-first
initializer observations are112 with intermediate11/111, and the isolated Macro
phase records1. All values are called after GC in the assembled component. The
original two tests remain intact. Required full baseline and exact final-head CI
remain acceptance gates.

The source staging code is original Rust. Native CLI file and namespace compilation now use
this source preparation and component assembly. Project, main and component-host
compilation remain legacy. Complete selected-WIT boundaries,
remaining frontend migration, evaluator retirement,
published dependency policy and scheduler/cancellation/live heap acceptance remain
open original M3 work. This API and its focused results do not complete M3.

## Native file compilation and typed invocation

`suss compile app.sus -w wit --wit-world api-world --src src
--export 'test:app/api@1.2.3#calculate=app/calculate' -o app.wasm`
resolves a WIT file or package directory, including dependencies, selects its
world, prepares source with isolated compiled macros and assembles the component.
Each function needs an explicit `--export PATH=VAR` mapping; `^:export` shorthand
is unfinished. Ambiguous world selection and invalid mappings fail before output
is replaced. Runtime initializers execute at component instantiation, including
language exceptions; the compiler host does not execute them.

`suss run app.wasm --invoke 'test:app/api@1.2.3#calculate' -- 19` resolves
the exact interface path and parses arguments against the component signature.
Malformed or out-of-range scalar arguments, wrong arity and missing functions
fail explicitly. Selection, function type, arity and scalar input validation use
the component's uninstantiated export metadata before creating guest instances;
invalid requests therefore cannot run Runtime initializers. With valid requests,
initializer failures still fail the command. Explicit invocation of a function named `run` accepts typed
parameters; ordinary numeric results print and are not process exit statuses.
Default command argv remains a separate path. Async/composite invocation and the
official command-world workflow remain unfinished. Scalar argument parsing also
works for externally supplied components; this does not extend the AOT adapter's
supported WIT types.

Three command/artifact regressions pass. Actual CLI compilation and invocation
cover compiled macros, dependencies, versioned interfaces, mixed scalar values,
once-only initializers, fresh Stores and GC. Selection/mapping failures preserve
prior output; Runtime throw compiles successfully and instantiation preserves
first payload17. Against the previous runner, malformed s32 input actually
succeeded and printed zero; the new runner rejects it. Fifteen affected existing
CLI tests, Python118 and Java-free byte-exact image reproduction/bootstrap4 pass.
CLI-only changes leave compiler fingerprints and bootstrap images unchanged.
Independent review, required unfiltered baseline and final-head CI are still
pending for this increment. This does not complete M3 or retire the evaluator.

## Namespace entrypoints

`suss compile -n app.core -w wit --wit-world api-world --src src
--export 'test:app/api@1.2.3#calculate=app.core/calculate' -o app.wasm`
uses canonical namespace lookup across `.sus`, `.cljs` and `.cljc` files. The
default source directory is `src`; repeated roots naming the same physical file
are deduplicated, while distinct matching sources fail as ambiguous. Source
conditional selection follows the portable ClojureScript branch. The leading
namespace must match the requested identity before a Macro session is created.
Validation and compilation consume the same immutable text and selected forms,
using the dependency loader's header grammar and declaration matching. Combining
`--namespace` with a positional file or `--main` is an explicit error.

Three executing command/component regressions pass. A macro imported from a
separate namespace and a dependency shared across Macro and Runtime phases
preserve once-only effects. Typed interface calls remain correct after GC in two
fresh Stores. Missing/wrong declarations, missing/ambiguous source and conflicting
entry modes preserve prior output. Runtime throw compiles successfully; actual
instantiation independently decodes first17 rather than later99 in fresh Stores.
The controlled previous-code namespace regression fails. Both phase images have
been regenerated for the compiler helper and reproduce byte-for-byte without
Java; bootstrap4 passes. Independent review, unfiltered full baseline and exact
final-head CI remain pending for this increment. Explicit export mappings remain
required; project/main/component-host migration and evaluator retirement are open.

Independent namespace review also rejects missing `--wit` before project routing
and rejects project-only `--world`/`--config` options in namespace mode. Actual
before regressions exposed legacy project fallback and ignored selection options;
all four namespace checks and four parent file checks pass after the guards.
Required full baseline and exact final-head CI remain pending.
