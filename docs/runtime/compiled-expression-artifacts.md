# Compiled expression artifacts — migration in progress

The native `Compiler::compile_expr_cached` and `compile_expr_with_info` routes use the staged source,
compiled macro and ABI2 fragment pipeline shared by native scripts and AOT.
Runtime effects execute only when the caller loads the prepared artifact into
its Runtime `Session`. The byte-only expression API and other prototype routes still
need migration; this change does not establish evaluator retirement or M3 completion.

Both native entrypoints prepare fresh user inputs in an isolated compiled Macro
session. They share the reproducible compiled core bootstrap; neither caches user
macro state between compilations. The `compile_expr_with_info` migration is staged
and awaits executing regressions, bootstrap reproduction, independent review and
full validation. The evidence below records the preceding cached-API migration.

The native example and benchmark callers are staged onto this artifact API too.
The example executes its original three expressions and displays rooted results
after GC. Benchmarks retain the original source inputs, keep each result's Store
alive, and use one artifact/Store per measured initialization to bound residency.
Initialization includes preflight, core/dependency initializers, expression
execution and Store teardown; preparation and empty-Store creation are setup.
Bundle size includes every module, including core. These measurements are not
comparable to historical standalone-module measurements. Example and benchmark
smoke execution remain pending; no timing or compatibility result is claimed.

The existing 201-case conformance consumer is staged onto the uncached entrypoint
with unchanged cases, expectations, fuel bounds and failure classification. Its
execution remains pending. Cached-API public regressions remain in place, covering
the other native entrypoint while both share the same preparation implementation.

An expression can need the compiled core, namespace dependencies and several
ordered input fragments. `CompiledExpr.prepared` owns an `ExpressionArtifact`
containing the bootstrap and complete `PreparedInput` plans, including dependency
identities. Its `wasm` field is an inspection copy of the entry module. Execution
uses the owned bundle; entry bytes alone are insufficient. Use `execute` with an
empty Runtime `Session`, then retain that session for incremental inputs and
rooted results. This representation is host-owned, not a serialized distributable
bundle or standalone core module.

Preparation parses source and executes macros in an isolated compiled Macro
session. Execution preflights every module's ABI, compiler identity and validation
before any initializer. It then uses the existing dependency initialization and
load-once machinery. Completed effects survive a later initializer failure.
Language exception payloads remain rooted in the caller's Store for inspection
and prompt recovery. Populated or Macro sessions are rejected before initialization.

`execute_with_core` lets host code capture canonical core values before user
redefinition. The callback runs after complete preflight and bootstrap initialization,
before user initializers. Its effects survive later failure. Reset during the
callback invalidates the pending execution and prevents publishing user initializers
into the replacement Store. Ordinary interactive inputs use the incremental API.

Executing evidence on the migration branch:

- Ten public checks pass: lexical `&env`, deferred throws, persistent atoms and
  GC roots, dependencies after source deletion, load-once, nominal ExceptionInfo
  observation, prompt recovery, populated-session rejection, empty Macro-session rejection and callback reset.
- A corrupt-last-module unit passes, requiring no published bindings or resident
  fragments. The exact 1,057-element vector fixture passes with a bounded 100M
  instruction allowance after an actual 20M fuel failure; the original corpus
  retains its 20M allowance and unchanged inputs/expectations.
- The original 201-case conformance baseline passes through the cached API. The
  independent 201-case compiled harness, 23 decoder checks and public checks pass.
- All original 16 differential expressions match a fresh pinned ClojureScript
  execution, including bits, UTF-16, thrown values and effects. The host wrapper
  now uses portable `catch :default` syntax. Seven former failures were reconciled
  only after actual fresh agreement; their old records remain in parent `88abea7`.
- Retained `symbol` keeps both arities and every Symbol/string/keyword/Var/error
  branch. The complete Var dependency retains all fields, protocols and 22 methods;
  only its property-read spelling is normalized. Private UTF-16 search/slice
  operations replace storage interop with checked bounds. A shared 40-case pinned
  corpus agrees in both native phases after GC; six identifier tests pass.
- Two fresh builds reproduce both bootstrap pairs byte-for-byte with Java and
  Node absent. Identity checks and four executing bootstrap tests pass. Strict
  import validation verifies 291 packaged files; the overlay remains partial
  (388 reviewed, 677 unassessed). All 178 Python tests pass.

Independent review, significant fixes, the exact full workspace baseline and
final-head CI remain required. The byte-only `compile_expr` API, prototype
`CoreCache`, compiler-on-Wasm and component-target CLI routes remain unfinished.
Complete macro schemas, dependency/cache policy and pending-I/O lifecycle acceptance
retain their original M3 scope. Refs issue #14; no milestone issue is closed.

The uncached native migration now has three executed regressions for fresh macro
namespaces, captured versus live bindings, and deferred owned exceptions after GC.
The CLI cache fixture retains a complete artifact through eviction and executes it.
The unchanged 201 conformance inputs pass through `compile_expr_with_info`; the
migrated benchmark completes all 30 smoke inputs. These results precede the new
printing source ports and require revalidation with those dependencies.

The original three `test_compile` expressions remain unchanged acceptance inputs.
A shared corpus has 16 exact fresh pinned printing observations. Its first seven
Runtime cases now pass through the complete retained default printer body; the
collection case still raises a language exception. Later cases and the shared
Macro corpus are unexecuted. Writer, options, buffer and storage adaptations remain
partial prerequisites, not full printing compatibility. No new PR is ready; the
exact full workspace baseline and final-head CI remain required.


The retained printing entrypoints now include the complete configurable writer
path and sequential formatter. Length/level limits, custom markers and alternate
writer call/separator ordering pass in both phases after GC. Retaining actual core
source facts exposed a real aggregate macro graph capacity requirement:66,474
recipes. A compile-only regression fails at the former65,536 aggregate capacity
and passes at131,072; original per-form/storage/materialization-work guards remain
unchanged and their regressions pass. Both phase artifacts reproduce without Java
or Node, all178 Python checks pass, and the unchanged201 conformance and three
uncached-artifact regressions pass with this source. That earlier run failed the second shared corpus case. The current retained
default body reaches collections after seven passing Runtime cases; later cases
remain unexecuted. No full-M3 or
new-PR readiness claim follows from these focused results.


The unfinished default printer now has a private storage guard backed by the
existing runtime native-object descriptor check. Both-phase regressions cover
GC, same-layout nominal rejection, scalar/closure/array rejection, single operand
evaluation and invalid arities before effects. It does not implement public
constructor-based `object?`. Bootstrap reproduction and its four executing tests
pass. That guard-only run failed at nil/booleans; the later retained default body
passes those cases and reaches the unfinished collection path.
Complete default printing and the original M3 acceptance remain unfinished.


The entire pinned `pr-writer-impl` declaration and `object?` are now extracted
with original hashes/EPL notices and explicit patches. No printer branch is
removed: remaining constructor/name, eager object-key mapping, identifier matching,
Date/regex/JS-symbol, generic object and collection formatting paths have explicit
unfinished dependencies. The source type-constructor guard reads the actual owned
descriptor through the unwrapped closure environment. A two-phase regression
preserves captured constructors through GC and class redefinition, while rejecting
ordinary functions, protocol values, instances and scalar/array/native objects.
It does not implement mutable foreign constructor flags or constructor names.

The public object predicate compares actual inherited or shadowed constructor
properties with the canonical default Object constructor. A shared fixture's17
observations exactly match a fresh pinned compiler/Node run and execute identically
in both Suss phases after GC, including null prototypes and inherited shadowing.
This does not certify foreign JavaScript object/boxing interoperability.

Current provenance verifies318 packaged files and415 reviewed/650 unassessed
inventory declarations. Both newly retained declarations remain in-progress.
The current default printer passes the first seven Runtime corpus cases: empty,
nil/booleans, exceptional numbers, escaped/lone-surrogate/astral strings and
symbols/keywords. The full printing target has11 passes and1 failure at collections;
later cases and the shared Macro corpus are not covered by that result.


Map formatter continuation: the complete pinned `keyword`, `strip-ns`, `lift-ns`,
`print-prefix-map` and `print-map` declarations are retained, with original source
hashes/EPL notices and explicit in-progress adaptations. Provenance now verifies
323 packaged files and420 reviewed/645 unassessed declarations. Keyword preserves
both arities, identity/Symbol conversion, empty/trailing/multiple slash parts,
namespace/name conversion, and UTF-16 units. Its fixed slash split uses original
owned UTF-16 storage, not general JavaScript split interoperability.

A fresh pinned oracle exposed the difference between multi-argument `str_` macro
calls and its first-class runtime function: macro calls convert each argument
before concatenation; runtime nil-first calls return empty. The retained keyword
and map formatter call sites now expand the macro behavior to ordered single-
argument conversions/concatenation. Runtime `str_` remains unchanged, and its
nil-first behavior has fresh primary/native evidence. General analyzer-dependent
`str_` macro implementation remains unfinished.

The explicit loop binding expansion also preserves the pinned macro's distinct
initial and per-iteration destructuring. The `(seq m)` initializer executes once;
custom seq/first/next/nth callbacks expose both sets of bindings. A shared custom
cursor/indexed-entry trace agrees with fresh pinned generated/executed code.
Four shared fixture results match both Runtime/Macro phases after forced GC:
15 keyword observations, seven map writer traces plus option restoration, three
runtime string-function calls, and the custom cursor/indexed-entry trace. These
checks cover formatting/namespace lifting and effects, not full collection
`IPrintWithWriter` implementations or general lazy/chunked acceptance.

Collection printer extensions now retain the complete pinned standalone
`IPrintWithWriter` form and its EPL notice/source hash. The adapter installs the
20 available nominal type stanzas in upstream order through `extend-type`, with
unchanged method bodies. A provenance regression rejects target/body changes,
reordering and duplicates. Sixteen absent source-type stanzas remain explicitly
pending in the patch and compatibility inventory. This is a partial source port;
full original printing corpus, both-phase GC execution, unchanged consumer
checks and final PR gates still determine acceptance.

The unchanged custom writer expression exposed a separate compiler bug after
collection printing became executable: compiled macro transport gives field
symbols indexing-reader metadata (`:file`, `:line`, `:column`, `:end-line`,
`:end-column`), which the bounded type analyzer rejected as field attributes.
These source facts are now retained without changing the three mutation flags.
Focused checks still reject immutable assignment, false mutation flags, local
shadowing and unsupported field options. The original custom writer fixture is
preserved to verify actual protocol execution through macro transport and GC.

Function printing now reads owned source labels from closures. The compiler
adapts the pinned `fn-self-name`/`munge` behavior from actual namespace, declaration
and parent function scopes, retaining punctuation and public `cljs.core` identity.
The Rust adaptation records the pinned source locations and EPL notice. General
functions keep their source facts on the signature wrapper; lowering transfers
the label to its actual owner without inventing a lexical ID or labeling synthetic
delegates. Unknown kernel names are not treated as anonymous names. Eight fresh
pinned fixtures match both Stores after GC, covering nested scopes, punctuation,
multiple arities, variadics, core names and captured functions/property writes
through redefinition; captured functions are re-read and called after collection.
The two original function-printing cases also pass in both phases. A direct ABI
regression roots an exact UTF16 label only through its closure and rejects unset
names, wrong owners and wrong initializer values as language exceptions.
Full original corpus/consumer checks follow in the handoff; this does not complete
constructor/storage branches, all printer contracts or the original M3 gates.
