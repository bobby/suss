# Compiled expression artifacts — migration in progress

The native `Compiler::compile_expr_cached` route now uses the staged source,
compiled macro and ABI2 fragment pipeline shared by native scripts and AOT.
Runtime effects execute only when the caller loads the prepared artifact into
its Runtime `Session`. Uncached expression APIs and other prototype routes still
need migration; this change does not establish evaluator retirement or M3 completion.

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
final-head CI remain required. Uncached byte-only expression APIs, prototype
`CoreCache`, compiler-on-Wasm and component-target CLI routes remain unfinished.
Complete macro schemas, dependency/cache policy and pending-I/O lifecycle acceptance
retain their original M3 scope. Refs issue #14; no milestone issue is closed.
