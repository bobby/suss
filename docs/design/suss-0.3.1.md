# Suss 0.3.1 specification

Status: accepted target design, 2026-09-29. **This describes the intended system,
not features already implemented.** Implementation evidence lives in
[the roadmap](../../ROADMAP.md) and [the session handoff](../roadmap/handoff.md).

[ADR-0001](../adr/0001-result-option-and-panic.md) proposes a staged move to nominal
Result/Option types and eventually native semantics without nil or throw/catch.
It is **not accepted** and does not supersede this contract.

This supersedes earlier architecture claims in README, ROADMAP, CLAUDE and
METADATA_DESIGN. Changes to this contract require a dated decision entry below.

## 1. Product and release contract

Suss compiles a ClojureScript-compatible portable language to WebAssembly GC.
It supports three artifacts: a WASI command, a component implementing a selected
WIT world, and a browser ES module with typed host bindings. Wasmtime is the
reference server host. Portability means hosts implementing the declared feature
profile and required imports; it does not mean every historical WASI runtime.
The compiler, macro runtime and shipped REPL must not require Java or JavaScript.
A JVM ClojureScript compiler and Node may be development-only test oracles.

The compatibility reference is the `clojurescript/` submodule at
`c4295f303100bbf5afac449242d30bca1126f1a1` (1.12.134). The contract is portable
ClojureScript behavior, not JVM Clojure's numeric tower or Java interfaces.
JS, npm and Closure interoperability are not source compatibility promises.
Portable public core definitions, their arities, macros and persistent types
are the eventual compatibility target. Exceptions must be individually recorded
with a reason and alternative, never inferred from the presence of `js/` in a
function's implementation.

Release gates:

* WASI alpha (M6): persistent compiled REPL, documented core subset and persistent
  collections, typed WIT interoperability, WASI 0.3.1 including asynchronous I/O.
* Compatibility beta (M7): every portable inventory item implemented and tested;
  source adaptations and deliberate exclusions published.
* Browser beta (M8): shared semantics corpus passes in supported browsers, typed
  bridge and distributable ES modules. Browser feasibility is tested in M0.
* CSP extension (M9): channels and `go` compatibility after futures and streams.

## 2. What survives the prototype

Retain the repository, Rust workspace, reader knowledge, tests and collection
algorithms as references. Replace architecture incrementally at tested boundaries.
The existing backend is a temporary executable baseline, not the new ABI. Do not
maintain two production compilers indefinitely. Delete obsolete paths once their
replacement meets the associated acceptance tests; preserve history in Git.

The audit found repeated condition evaluation, unvalidated component output,
hidden `suss/print-str` imports, incomplete WIT types, bump-only canonical memory,
UTF-8 strings and integer semantics, source-replaying REPL state, and conformance
tests that accepted any GC struct and did not fail on errors. Existing test counts
are not evidence of ClojureScript or WASI compatibility.

## 3. Compatibility inventory and core import

`docs/compatibility/cljs-core.edn` is generated from pinned source definitions.
The generator records phase, declaration kind, source range and SHA-256. Manual
review belongs in a separate tracked overrides file so regeneration cannot erase
it. Initial `unassessed` entries are an explicit backlog, not exclusions. Protocol
methods and generated constructors must also be accounted for during review.

For each definition, review: public/private visibility, dependencies (including
macros and protocols), portable/adapted/host-specific classification, adaptation
patch, tests and implementation status. Track arities and reader branches.

Import in three layers:

1. Small runtime intrinsics: storage, arithmetic/coercions, dispatch, roots,
   exceptions, scheduling and host calls.
2. Adapted foundations: collections, strings, numbers, protocols and host-facing
   operations with JS implementation details replaced by those intrinsics.
3. Unmodified portable upstream forms evaluated in Suss namespaces.

Retain upstream copyright and EPL notices on copied or adapted source, record
origin and patch hashes, and ship the corresponding license. Do not relabel
upstream code as the repository's MIT/Apache code. Extraction must use forms and
source provenance, not textual deletion of JS-looking lines.

`suss.core` is canonical; `cljs.core` aliases the same bindings. Accept `.sus`,
`.cljs` and `.cljc`; reject ambiguous namespace resolutions. Reader conditional
features are `:suss` and `:cljs`, taking the first matching branch in source order.
Namespace aliases, refers, exclusions, phase imports and qualified symbols are
resolved consistently in AOT, macros and REPL.

## 4. Observable language semantics

Ordinary numbers have IEEE-754 binary64 behavior, including NaN, infinity, signed
zero and ClojureScript bitwise coercions. An i31 optimization is permitted only
if indistinguishable from that contract. Ratios and arbitrary precision integers
are not silently inherited from the current reader. Unsupported numeric forms
must produce diagnostics. Exact WIT 64-bit integers use separate wrapper values.

Strings are sequences of UTF-16 code units. Character literals are one-unit
strings where ClojureScript treats them that way. Test astral characters, lone
surrogates, length, indexing, slicing, comparison, hashing and printing. UTF-8 is
an external encoding, not the internal indexing contract.

Only nil and false are falsey. Evaluate the callee, arguments, collection entries,
conditions and bindings exactly once in source order. Short circuiting must avoid
unselected operands. `recur` is tail-position checked and evaluates all replacement
bindings before assignment. Both `loop` recurrence and function-level `recur`
reuse the recurrence frame without growing the call stack. Wrong arity,
unsupported forms and unresolved names
are errors with source locations, never nil, `Unknown`, invalid Wasm or a trap
used in place of a language diagnostic.

Persistent lists, vectors/subvectors, array/hash/sorted maps, hash/sorted sets,
queues, records, sequences, lazy/chunked sequences and map entries must implement
upstream observable equality, hash, metadata and ordering contracts. Sequential
equality crosses sequential types. Map/set equality is order independent. A
collection's hash must agree with its equality. Test collision nodes, trie
boundaries, structural sharing and metadata preservation. Transients enforce their
lifecycle and error after persistence. Reduced values stop reduction without
forcing subsequent sequence elements. Implement all reduction/transducer arities.

Core also includes atoms (validators, watches and identity-based CAS), volatiles,
delays, dynamic bindings, protocols, multimethods, exceptions, printing and portable
macros. Laziness and chunking are observable through effects and must follow the
pinned reference, not just produce the same final values.

## 5. Compiler pipeline

One pipeline serves AOT, REPL and compiled macros:

```
source -> reader forms + spans + metadata -> phase/namespace resolution
       -> macro expansion -> HIR -> explicit evaluation-order/control-flow IR
       -> closure and async lowering -> WasmGC -> target bindings -> validation
```

Keep reader metadata separate from runtime metadata. HIR retains binding identity,
source location, inferred information and resolved globals. Normalize every effectful
operand to a temporary before lowering branching, dispatch and ABI conversions.
No backend instruction emitter may re-emit an operand to inspect its type.

The control-flow IR has explicit blocks, values, branches, calls, throws and
suspension points. Verify dominance, arities, tail positions, types and effects
before code generation. Runtime tests use descriptor identity; Wasm structural
`ref.test` alone is not a nominal type test. Reachability may remove unused host
imports. Optimization must preserve effect ordering and exceptions.

## 6. Shared GC runtime ABI (version 2)

Use a small stable, versioned recursive type group imported by every compiled
fragment. `Value` is an eqref, with nil/booleans as i31 sentinels, boxed f64,
UTF-16 arrays, and typed runtime objects. Initially use a universal closure
`(environment, argument-array) -> Value`; arity checks are centralized. Specialized
call paths can be introduced later with equivalent tests.

A user-defined object carries a stable type descriptor and a field array rather
than introducing a new cross-fragment Wasm struct layout for each `deftype`.
Descriptors hold nominal identity, fields, protocol implementations and metadata.
Closures, binding cells, exception values and dynamic binding frames have stable
layouts. Published compiled artifacts record compiler, runtime ABI and dependency
versions and fail early on mismatch. Generated module type indices are never
public ABI identifiers.

GC owns language objects. Canonical ABI linear memory is separately allocated and
freed; component resources require explicit lifetime management. GC finalization
must not be the correctness mechanism for file handles or other host resources.

## 7. Persistent compiled REPL and macros

A session owns one long-lived Wasmtime Store, shared runtime, binding cells,
namespaces, loaded modules and scheduler. Compile each input as an incremental
core module importing that runtime. Existing atoms, closures, types and values
remain rooted and usable across fragments. Global lookups observe redefinitions;
a previously captured function value retains its original behavior.

Never rebuild session state by replaying source. Initializers and `defonce` run
once. A compile error changes no session bindings. A failed definition initializer
leaves its previous binding; arbitrary effects performed before a runtime error
are not rolled back. Runtime exceptions return control to the prompt. Define
explicit reset and cancellation behavior. Track resident generated code separately
from live GC memory; first release may retain fragment code until session reset.

Macros run as compiled Suss in an isolated compile-time session with separate
phase namespaces, `&form`, `&env`, syntax quote, unquote/splicing and deterministic
gensyms. Use a bounded bootstrap expander and reproducible, versioned bootstrap
artifacts to break the compiler/core cycle. Remove the tree-walking macro evaluator
when compiled bootstrap tests pass. Cache keys include source, compiler/runtime
ABI, macro dependency graph, target profile and flags. No JVM in the shipped path.

M0 must first prove cross-module recursive GC type sharing, roots across GC,
closures called across fragments and new nominal types. If the selected engine
cannot support this, record a decision before implementing the REPL architecture.

## 8. WIT boundary

Resolve arbitrary selected WIT worlds with upstream tooling; generate adapters
from the resolved type graph. A hand-maintained table of WASI function names is
not the architecture. Preserve dependency package versions from official WIT.
Support imports/exports, interfaces, constructors, methods, statics, resources,
all scalar and composite types, maps, adopted `implements`/external-id semantics,
async functions, futures and streams. Compile-time unsupported-feature errors are
acceptable during development; release claims require interoperability tests.

| WIT type | Suss boundary value |
| --- | --- |
| bool; small integer types; f32/f64 | Boolean or checked ordinary number; f32 rounds at boundary |
| s64/u64 | Exact signed/unsigned wrapper, decimal-string constructors; checked `to-number` |
| string | UTF-16 string; reject lone surrogates when producing Unicode WIT strings |
| char | One Unicode scalar, distinct validation from ordinary character indexing |
| list, tuple | Persistent vector, with tuple length checked |
| record | Keyword-keyed map with generated field schema |
| enum; flags | Keyword; keyword set |
| variant | Tagged vector `[:case]` or `[:case payload]` |
| option | `[:none]` or `[:some value]`, preserving `some(nil)` |
| result | `[:ok]`/`[:err]` or tagged payload, according to the WIT case type |
| map<K,V> | Persistent map; validate allowed keys; last incoming duplicate wins |
| own/borrow resource | Opaque handle with ownership/lifetime tracking |
| future/stream | Typed asynchronous wrapper |

A WIT map is a boundary type, not a replacement for persistent HAMTs. Do not
promise ordering or zero-copy transfer. Reject unknown keys, missing required
fields, malformed variants and out-of-range values with boundary diagnostics.
Canonical resources cannot escape borrow lifetimes or be reused after move/close.
Provide explicit close and lexical scope APIs, including exceptional cleanup.

Implement alignment, checked memory growth, realloc that preserves old contents,
free, post-return and async transfer lifetimes. Test invalid guest/host data,
allocations on error and cancellation, and repeated calls. Use canonical ABI
machinery and generated fixtures as the reference rather than inventing encodings.
Pure components have exactly their selected world imports; printing must not add
a private hidden import to every artifact.

## 9. Asynchronous execution

Start with futures and streams; CSP channels and `go` are M9. `suss.async/future`
is a macro forming a suspendable body. `suss.async/await` is legal inside such a
body; reject it elsewhere. Provide resolved/rejected futures, completion and
`cancel!`. Async WIT imports return futures; async exports use a declared future
result. A future returned as a WIT value is not implicitly flattened.

Use stackless continuations and the canonical callback ABI, not experimental
stackful switching or OS threads. Lower locals, exception handlers and dynamic
bindings live across suspension into rooted GC continuation objects. A cooperative
scheduler tracks Pending, Ready, Failed and Cancelled tasks and resumes each
continuation at most once. Preserve try/finally, dynamic scope and cleanup across
all transitions. Reentrancy and cancellation races require explicit tests.

Streams provide bounded asynchronous chunk reads/writes, backpressure and explicit
EOF distinct from a nil element. Define single ownership of read/write endpoints,
close, errors and cancellation. A WIT `result::err` is an ordinary typed value,
not automatically a task failure. Runtime traps remain distinguishable from
language exceptions and typed host errors. Fairness tests ensure a task cannot
starve completed I/O. Host adapters must obey the same scheduler contracts.

## 10. Targets and public interfaces

Planned commands (not all implemented):

```
suss repl
suss -e '(+ 1 2)'
suss script.sus
suss -m app.core arg1 arg2
suss compile -m app.core -o app.wasm
suss compile -n library.core -w api.wit --wit-world api -o library.wasm
suss compile --target browser -n app.core -o dist/
suss run app.wasm
suss check
suss doctor
```

Project configuration records source paths, target, entry namespace, selected WIT
world, dependency lock and explicit imported/exported WIT path to Suss var mappings.
`^:export` is shorthand only for unambiguous freestanding exports. CLI command
bindings use the official asynchronous command world. `-main` receives argument
strings; normal completion succeeds, uncaught exceptions fail, and explicit exit
uses the command's documented status policy. Do not silently treat every returned
number as a process exit code.

Bind all capabilities in the WASI 0.3.1 release package graph, including CLI,
filesystem, clocks, random, sockets, HTTP service/middleware and async I/O. Use
host-granted capabilities with explicit configuration. Probe every actual import
and feature; a version string alone is not proof. Candidate toolchain: Wasmtime
49.0.1 with its matching wasm-tools libraries (0.258.0) and wit-bindgen (0.61.1).
These are candidates until M0 probes pass; the existing prototype uses older
versions. Lock the tested versions and WIT content hashes, not floating branches.

Browser output is ES modules, declarations, Wasm and explicit initialization.
Use the same runtime and continuation semantics, adapting futures to Promises and
streams/events to typed wrappers. Provide console, timers, fetch and minimal typed
DOM/event bindings. Unsupported filesystem/socket capabilities produce capability
errors. Direct core-Wasm + ES module loading does not depend on a complete browser
WASI implementation. Jco component transpilation is an optional separately tested
packaging route. Browser bridge interop is explicit, not unrestricted JS syntax.

## 11. Verification and CI

The test oracle must decode actual values independently of Suss equality or
printing. Use lossless tagged values (including float bits, UTF-16 units, maps,
sets, sequences and exceptions) and ordered effect traces. Decoder errors,
unknown tags, missing files, malformed cases and missing expected results fail.
Never accept an opaque GC object as a wildcard. Explicit tracked known failures
must distinguish wrong output, compile failure, validation, trap and decode error;
an unexpected pass requires updating the baseline.

Required suites:

* Differential ClojureScript values/effects, every public portable arity, condition
  effects exactly once, left-to-right calls, reduce/map/transducer edge cases.
* Numeric and UTF-16 boundaries, metadata, collision nodes, equality/hash,
  persistent sharing, transient invalidation, lazy/chunked effects.
* Session persistence, redefinition, defonce, old closures/new types, macro phases,
  compile error isolation, runtime exception recovery and interruption during I/O.
* Rust-to-Suss and Suss-to-Rust fixtures for every WIT type and nested combinations,
  resources, exact integers, maps, async imports/exports, nested future/stream values.
* Cancellation, reentrancy, backpressure, EOF, exceptional cleanup; 10,000 varied
  calls with allocation/resource counters returning to baseline after cleanup.
* Official WASI world scenarios, exact import shape, validation under the declared
  profile, reproducible bootstrap without Java installed, browser shared corpus.

Reuse engines and cached core analysis. Bound fuel/time/memory and parallelism.
Use lean debug/test profiles locally; large stress tests can run in dedicated CI.
A green legacy suite is not a completed release gate. Publish passing, failing,
skipped and unassessed counts separately.

## 12. Session protocol and decisions

Read this spec, ROADMAP, inventory and handoff before implementation. Select an
unblocked issue; add a regression before fixing behavior. Implement a bounded
vertical slice, validate it, then update evidence, known failures and the next
concrete step. Every milestone closes only when its acceptance evidence exists.
Record deviations here and change affected tests/docs in the same change.

Decisions accepted 2026-09-29: ClojureScript contract; WASI first; WasmGC required;
persistent compiled REPL in the first release; no shipped JVM; futures/streams
before CSP; explicit browser bridge; incremental replacement of the prototype.

2026-09-29 evidence clarification: WebAssembly arithmetic producing a canonical
NaN permits either sign. The differential observation comparator accepts this
sign difference while retaining raw bits; changed NaN payloads, finite bits and
signed zero still fail. Storage and boundary round-trip tests check NaN bits
exactly. This does not change language equality or make NaN equal to itself.
See the [WebAssembly floating-point rules](https://www.w3.org/TR/wasm-core/).

2026-10-01 identity-hashing ABI decision: advance the prototype shared runtime
ABI from1 to2 before collection hashing integration. Append one mutable Value
slot to closures, descriptors, ordinary objects and exceptions, initialized nil
and assigned a numeric UID lazily. Existing data/metadata/cause offsets and the
single ten-type recursive group remain intact. This supplies GC-owned identity
for the pinned default IHash dependency without a global table retaining hashed
objects. UID identity is not structural equality and does not replace Murmur3.
Artifact manifests reject ABI1 before initialization; actual incompatible Wasm
layouts also fail linking. Existing generated artifacts must be rebuilt. The
universal invocation, source semantics and release gates do not change. Evidence
and remaining limits belong in docs/runtime/identity-hashing.md and handoff.

2026-10-01 collection literal ordering decision: preserve textual evaluation
order for vector, map and set literals. In a map, evaluate each key followed by
its value before advancing to the next pair, regardless of collection size.
In a set, evaluate entries in textual order. Each expression runs exactly once;
a thrown entry prevents later entries and construction. This clarifies section4
and deliberately differs from incidental reader hash iteration and the pinned
compiler's large-map all-keys-before-values emission. It does not specify map/set
iteration order or change equality/hash semantics. Differential evidence must
retain exact, separately asserted Suss and pinned results for these variances;
they are not compatibility matches or skipped cases. See
[collection literal evidence](../runtime/collection-literals.md).

2026-10-05 recurrence clarification: explicit `recur` to an enclosing `loop` or
function guarantees constant call-stack usage. Evaluate replacement operands
exactly once in source order into temporaries before assigning any recurrence
binding, then transfer control back to that recurrence target. This makes the
portable ClojureScript requirement explicit: the pinned compiler's `:recur`
emitter in `clojurescript/src/main/clojure/cljs/compiler.cljc` evaluates temporaries,
assigns the frame parameters and emits `continue`. The existing tail-position and
arity diagnostics remain required. General optimization of ordinary recursive
calls is not added by this clarification. Executing evidence and validation gates
belong in the runtime docs and handoff; this entry does not claim M3 complete.

## Primary references

* [ClojureScript differences](https://clojurescript.org/about/differences)
* [Pinned core source](https://github.com/clojure/clojurescript/tree/c4295f303100bbf5afac449242d30bca1126f1a1/src/main)
* [WASI Preview 3](https://wasi.dev/releases/wasi-p3)
* [Component Model WIT](https://github.com/WebAssembly/component-model/blob/main/design/mvp/WIT.md)
* [Component Model concurrency](https://github.com/WebAssembly/component-model/blob/main/design/mvp/Concurrency.md)
* [Wasmtime 49.0.1 dependencies](https://github.com/bytecodealliance/wasmtime/blob/v49.0.1/Cargo.toml)
* [Jco transpilation](https://bytecodealliance.github.io/jco/transpiling.html)

Upstream proposals and tool releases move. Recheck these sources when executing
M0 and record actual tested revisions; do not equate an open proposal with support.
