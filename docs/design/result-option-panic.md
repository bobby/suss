# Result, Option, and panic contract for ADR-0001

Status: **proposed for review**, 2026-11-20. This is a concrete design for
[ADR1-01](https://github.com/bobby/suss/issues/50) and
[ADR1-02](https://github.com/bobby/suss/issues/51), not acceptance or executing
evidence. [The accepted design](suss-0.3.1.md) continues to govern shipped code.
Examples and fixture names below are acceptance targets, not supported APIs.

## Decisions requested

Review these decisions separately. Accepting the tactical stage does not accept
the strategic stage. Acceptance records must name the reviewer, date, scope,
and any amendments in #50 and in dated accepted-design decisions.

| Stage | Proposed decision | Rationale and release condition |
| --- | --- | --- |
| Tactical | Accept nominal Result/Option/Unit, explicit composition and conversions, and versioned nominal WIT bindings; preserve Clojure source behavior | Prevent domain-data collisions at typed boundaries without changing compatible core. Ship only after value, WIT and compatibility evidence passes. Panic has its own later execution gate. |
| Strategic | Accept the native migration target, initially behind an explicit experimental profile | Make failure, absence, completion and null distinct throughout native programs. This deliberately gives up Clojure source compatibility in that profile. No native release until the complete core, macro, resource, async and target gates pass. |

The proposed default remains the Clojure-compatible profile throughout tactical
delivery and native preview. A future default switch needs a separate release
decision after native acceptance; installing a new compiler must not reinterpret
existing source. Keep the compatible profile supported through native preview
and at least one stable release following native stable availability. Removing
it then requires a separate decision and migration notice; there is no automatic
removal date or promise of a second compiler. One pipeline and runtime implement
both profiles with distinct core/bootstrap identities.

Native conditionals use **false-only truthiness**. Compatible conditionals retain
nil-and-false truthiness. The native profile rejects source nil, including quoted
nil, and application throw/catch; it must never silently select compatible core.
Accepting this stage therefore also requires reconciling AGENTS.md's Clojure
contract, the inventory, and the M7 compatibility gate as described below.

## Nominal values and inspection

Public namespaces are `suss.result`, `suss.option`, `suss.unit` and `suss.error`;
examples use aliases `result`, `option`, `unit` and `error`.

| Operation | Contract |
| --- | --- |
| `(result/ok x)`, `(result/err e)` | Exactly one payload; construct closed Result cases Ok and Err |
| `(option/some x)`, `(option/none)` | Exactly one payload for Some; zero for None |
| `(unit/unit)` | Zero arguments; returns the sole Unit value |
| `result/result?`, `result/ok?`, `result/err?` | Total one-argument predicates; unrelated values give false |
| `option/option?`, `option/some?`, `option/none?`, `unit/unit?` | Total one-argument predicates with the same rule |

Cases are immutable, are not records, sequences, maps, invokable values or
extensible protocols, and have no user-writable fields or metadata. `count`,
`seq`, field mutation and generic record constructors cannot unwrap them.
`type` returns the corresponding closed Result, Option or Unit type value;
`identical?` follows reference identity without promising constructor interning.
`=` compares type, case and payload under ordinary payload equality; hashes
combine a fixed type/case seed with the ordinary payload hash. Equal cases have
equal hashes, and hash collisions alone never imply equality. Structural
lookalikes, user-defined types and redefinitions cannot acquire case identity.
All cases are truthy in both profiles, including Some(false) and tactical
Some(nil), Ok(nil), Err(nil). An Error implementation inside Ok remains success.

Use private runtime-owned descriptors shared by every fragment and both phase
bootstraps. Ordinary nominal-object storage is acceptable only if fields cannot
be mutated through generic interop and user descriptors cannot spoof these
identities. Nominal checks include runtime identity, case and exact field shape,
not just `ref.test`, printed names or a user-supplied numeric ID. None and Unit
need separate identities. The independent decoder must read and verify this
storage directly against runtime-owned descriptors, including after forced GC;
it must reject invalid cases/arity rather than call guest predicates or printing.

Readable printing uses reserved tags:

```clojure
#suss.result/ok [:err :domain-event]
#suss.result/err nil
#suss.option/some false
#suss.option/none []
#suss.unit/unit []
```

Source reading, `read-string` and `suss.edn/read-string` recognize the same tags
as constants, never evaluate payloads, and reject nonempty/nonvector payloads
for the two zero-payload tags. Native reading rejects nested nil too. Unknown
tags remain errors; tag handlers cannot be replaced by namespace redefinition.
Resource-bearing payloads use an unreadable diagnostic representation, since a
reader cannot recreate ownership. Printing does not authorize serialization.

`suss.error/Error` is an optional diagnostic protocol with `message` (String),
`data` (persistent map), and `cause` (Option of any diagnostic payload).
It controls neither recovery nor propagation. `error/inspect` returns
`{:message string :data map :cause option}`. Without Error it returns the fixed
message "Error payload", `{:suss.error/payload e}`, and None. This fallback does
not invoke the payload's printer or stringify arbitrary data. Invalid protocol
return types produce a contract fault. Protocol failures are not swallowed;
supervisor reporting uses a bounded non-calling fallback if inspection fails,
and detects cause cycles. WIT transports only its declared schema, never this
protocol or arbitrary diagnostic state automatically.

## Matching, composition, and lexical propagation

Matching forms are exhaustive macros with exactly the shown clause heads;
clause heads are syntax, not calls or user-rebindable constructors:

```clojure
(result/case expression
  (ok [value] success-body...)
  (err [error] failure-body...))
(option/case expression
  (some [value] present-body...)
  (none [] absent-body...))
```

Clause order is immaterial. Duplicate, missing, unknown or malformed clauses
are located compilation errors. Each binder vector contains exactly the shown
number of binding patterns; patterns follow that profile's destructuring rules.
Evaluate the scrutinee once, check its nominal type, bind only the selected
payload and evaluate only the selected body. Wrong-type diagnostics identify the
form, expected type, actual type and source span. Empty bodies follow the
profile: compatible nil, native Unit. There is no implicit catch-all success.

`result/map`, `result/map-err` and `option/map` take `[f outcome]`, call `f` once
only on the selected case, and rewrap its return value, including nested cases.
`result/bind` and `option/bind` use the same argument order and require the
callback's return to be the same nominal container type. They flatten exactly
one level. Nonselected cases pass through unchanged. Wrong input or callback
return types fault; exceptions/panic do not become Err/None.
`result/value-or` and `option/value-or` take `[outcome fallback]` and unwrap
success/presence, otherwise return fallback. They are ordinary eager functions:
all arguments evaluate before the call. Use matching for a lazy fallback.
`option/to-result` takes `[option error]`, giving Ok(payload) or Err(error), with
the same eager rule. No implicit conversion or truthiness-based selection occurs.

```clojure
(result/result-let [x (result/ok false)
                    y (result/ok [:err :event])]
  [x y]) ; Ok([false [:err :event]])
(option/option-let [x (option/some false)] x) ; Some(false)
```

Each binding vector has an even number of pattern/initializer entries. Evaluate
initializers exactly once, left to right, in the environment extended by earlier
success bindings. The first Err or None exits **this lexical form**, returning
that original case without reconstructing it; no later initializer or body runs.
Unwrap before destructuring. Success wraps the last body value in Ok/Some even
if it is already an outcome. An empty binding vector still wraps the body.
Macro expansion uses hygienic locals and ordinary lexical branches, not a global
exception or task-wide return. A closure invoked in an initializer returns to
that initializer normally; a propagation form in a closure exits its own scope.
Do not permit `recur` to jump through a pending result wrapper or cleanup scope;
diagnose it as non-tail. Nested loops wholly inside such a body remain legal.

Initializer resource scopes and dynamic bindings unwind before inspecting the
case returned by that initializer; enclosing scopes unwind when their lexical
body finishes. Propagation must not close an enclosing scope prematurely. The
same rules hold across suspension. Exhaustiveness is syntax checked; dynamic
payload types are runtime checked. A discarded-result lint is optional and
cannot claim static prevention of ignored errors.

## Tactical conversions and WIT contract

Compatible core lookup, sequence, exception, printing and truthiness semantics
remain Clojure's. Add these explicitly named conversions:

| Operation | Result and loss |
| --- | --- |
| `option/from-nil [x]` | nil -> None; otherwise Some(x); present nil cannot be recovered |
| `option/to-nil-lossy [o]` | None -> nil; Some(x) -> x; loses None versus Some(nil) |
| `unit/to-nil-lossy [u]` | Accepts only Unit; returns nil |
| `option/find [associative key]` | Presence-aware lookup: missing -> None, present -> Some(value), including nil/false; reject non-associative inputs |
| `result/attempt body...` | Macro: Ok(last value), or Err(original thrown payload) for compatible language exceptions only; preserves cleanup |
| `result/or-throw [r]` | Ok(x) -> x; Err(e) throws ExceptionInfo with message from error/inspect and data `{:suss.result/error e}`; cause is the original language exception when e is one |

The three nil conversions, attempt and or-throw belong only to compatibility.
They never capture panic, trap, fuel exhaustion, host errors or cancellation.
If Error inspection itself faults during or-throw, that fault propagates.
Presence-aware find works in both profiles. Resource restrictions still apply.

Generated WIT adapters use a versioned binding contract `nominal-v1`:

| Resolved WIT schema | Nominal value |
| --- | --- |
| `option<T>` | None or Some(value validated as T) |
| `result<T,E>` | Ok(value validated as T) or Err(value validated as E) |
| Result case with no payload | That case with Unit; any other payload is rejected |

Unit is not a WIT primitive. Ok(nil) is legal in compatible language code but
cannot cross `result<string,E>`. Some(nil) is not representable by `option<bool>`;
use an explicitly nullable domain schema if that distinction must cross WIT.
Nested option/result cases retain every level. Some(None) can cross an appropriate
nested schema. Payloadless Ok(Unit) and None remain distinct. Ordinary WIT
variants retain their existing tagged-vector contract; result-looking vectors
are domain data and must fail result/option conversion. Outgoing adapters reject
wrong cases/payloads with a located boundary fault before transfer; incoming
adapters validate canonical discriminants, memory and payloads before publication.

The wire schema and Rust-generated types do not change merely because Suss's
representation changes. Binding manifests do change: old `tagged-v1` Suss
artifacts require regeneration. No silent legacy-vector acceptance. Migration
tools may perform explicitly requested schema-directed conversions, including
arity checks; those conversions must not be installed at ordinary call sites.

## Panic, cleanup, and supervision

`suss.panic/panic` takes one diagnostic payload and never returns. Use a private
runtime panic EH tag, distinct from compatible language exception tags and host
traps. Compatible catch, including a catch-all, cannot resume panic. Compiler
errors remain located compilation diagnostics. Dynamic arity/type/contract
faults remain language exceptions in compatibility and panic in native.
An explicit returned Err is ordinary successful execution in either profile.

Native cleanup is `(suss.scope/finally body-expression cleanup-expression)`;
both expressions may be `do` blocks. Evaluate body once, then cleanup exactly
once on normal completion, propagated cases, panic and cooperative cancellation.
Return the body's value on successful cleanup. Resource scopes desugar into this
form, close acquired resources once in reverse acquisition order, and restore
dynamic bindings even if a close fails. Failed acquisition closes only prior
successful acquisitions. Ownership cannot escape a borrow by wrapping it in a
case, closure or future. No GC finalizer provides correctness.

| Pending outcome | Cleanup failure policy |
| --- | --- |
| Normal value, including Err/None | A cleanup fault replaces the return; record a discarded case in supervisor diagnostics when applicable |
| Panic or cooperative cancellation | Preserve the original outcome; attach cleanup faults as ordered suppressed diagnostics, continue other lexical cleanups |
| Compatible language exception in compatible try/finally | Retain existing Clojure finally precedence; new native cleanup does not rewrite it |
| Trap, exhausted engine fuel, hard interruption, host abort | Report the engine/host outcome; no guest-cleanup guarantee |

An explicit Err from a cleanup expression is data; resource-close adapters must
inspect their declared Result and turn a failed mandatory close into a cleanup
fault. A cleanup fault cannot be handled by resuming the unwound body. Shield
cooperative cleanup from repeated cancellation, bound it with a separate host
deadline/budget, and report forced termination as a distinct abort with incomplete
cleanup. Do not replenish the failed computation's exhausted fuel and resume it.

The supervisor records separate outcomes: returned value (including Err),
compatible language exception, panic, trap, fuel exhaustion, cooperative
cancellation, and forced abort. Source effects already performed remain visible.
A failed initializer never publishes its replacement binding. After a successfully
unwound panic, a REPL may continue only after the invocation has no pending tasks,
borrows or cleanup frames. Engine failures, incomplete cleanup and host uncertainty
poison that invocation/session and require reset before further evaluation.
Independent sessions/tasks may continue only when they share no poisoned state.
Reset explicitly discards session state; it is never automatic source replay.

Suspended continuations root cases, dynamic frames and resource ownership.
Exactly one of completion or cancellation wins; a resumed callback after terminal
completion cannot run cleanup twice or publish a failed initializer. Awaiting
Future<Result<T,E>> yields that Result, including Err; nested futures are not
flattened. A panicked task fails at the supervisor, not as a declared WIT Err.
Stream EOF is a separate scheduler event from elements None, Some(false), or Unit.

WASI command bindings follow the actual selected command world: normal return
succeeds, an uncaught execution fault fails, and arbitrary returned numbers or
Result values do not imply an exit status. An explicit command helper maps an
application Result to the world's documented success/error return; Err without
that helper is still data. Component exports return declared Err normally;
panic uses the host's execution-failure path, never fabricates a schema error.
Browser promises resolve declared Err values and reject execution failures with
the separate outcome kind. Cancellation remains distinct from application Err.
Each target must prove this against its real bindings before release.

## Native core migration and artifact identity

Every row requires individual inventory review, source/provenance records and
executing tests, not a global nil textual replacement:

| Surface | Native contract |
| --- | --- |
| `get`, keyword/map/vector invocation, `get-in` | No-default lookup gives Option; explicit-default arities give value/default by presence; a present None or Unit stays present |
| `find` | Option of map entry, distinguishing missing from a present Option payload |
| `first`, `nth` without default, `peek` | Option of element; empty/out-of-bounds -> None; explicit-default nth returns value/default |
| `seq`, `next` | Option of nonempty sequence; Some contains an actual nonempty sequence; elements may themselves be None/Unit |
| `rest`, `empty`, empty collections | Return collections, including the correct empty type; Unit/None are not empty collections |
| Sequence consumers, lazy/chunked traversal, `apply`, transducers | Match outer exhaustion explicitly; preserve element cases and observable realization order |
| Map destructuring, including nested keys | Each potentially missing leaf is Option; required nested container must be present and associative or fault; do not flatten a present Option-valued leaf |
| Sequential destructuring | Each potentially missing fixed element is Option; rest binding is a sequence, possibly empty; `:as` retains the original input |
| `:or` defaults | Replace only outer missing bindings with Some(default), preserving the binding's Option contract; evaluate a default only on absence, once, in binding traversal order; present false/None/Unit never chooses it |
| `if-let`, `when-let` | Retain false-only conditional binding; never unwrap cases; warn on direct Option/Result use and point to matching |
| `if-some`, `when-some` | Require Option, bind Some payload including false, and choose absence branch on None |
| `do` with no forms, omitted if branch, empty function/let body | Unit; no manufactured None |
| Effect APIs (`println`, watches, setters), successful cleanup | Unit unless that API already has a meaningful return value |
| Optional function arguments, multi-arity functions | Omitted argument represented explicitly by its API; compiler does not fill missing arity slots with None/Unit |
| Validators, delays, memoization | Validator predicates keep Boolean contracts; delayed/memoized values may be any case; private unrealized/cache-miss markers are not application None/Unit |
| External JSON/JS null | A separate nominal boundary Null value, truthy, not a lookup miss or source nil; explicit boundary conversion only |
| Unbound vars, unused object slots, internal sentinels | Private runtime states; reading an unbound var faults and never yields an application case |
| Parsing, I/O, validation, checked collection operations | Expected operational failures return documented Result/Option; unchecked programmer misuse panics |
| `throw`, `catch`, `try`, `ex-info`, exception adapters | Reject application throw/catch and legacy try syntax with migration diagnostics; use Result matching and lexical cleanup; legacy exception constructors/adapters remain in compatibility |
| Macros, syntax quote, compiler data, bootstrap | Profile-specific core expansion and payload validation; native expansion cannot emit nil or legacy exception syntax; macro artifacts carry their own profile and dependency identity |

Native map destructuring traverses explicit bindings in source order, with
`:keys`/`:strs`/`:syms` entries in their vector order. This also defines default
evaluation order independently of map storage iteration. A missing intermediate
in `get-in` yields None (or the explicit default); a present non-associative
intermediate faults rather than masquerading as absence. An empty path returns
Some(input), or input for the default-taking arity. The compatible profile keeps
Clojure's existing default evaluation behavior, including eager expressions;
native lazy defaults are a deliberate, separately tested departure.

Before accepting the strategic contract, reconcile M7 as the compatible-profile
Clojure gate and add explicit native source/core/target gates. Unchanged behavior
continues to use JVM Clojure and the jank suite; departures have separate exact
native fixtures. Neither baseline changes nor new skips count as native evidence.
Extend inventory reviews with per-profile contracts/tests without marking
unassessed declarations implemented. Record upstream licenses and patch hashes
for every adapted macro/core form. Numeric semantics remain ADR-0003 in both
profiles; no ClojureScript numeric recordings become native expected values.

Artifact manifests and cache keys include semantic profile (`clojure-v1` or
`native-v1`), binding contract (`tagged-v1` or `nominal-v1`), core/bootstrap digest,
compiler identity, runtime ABI and dependency identities. Unknown/missing profile
identity in new-format artifacts fails before guest initialization. A runtime
layout/tag change bumps ABI; a binding or source contract change bumps its own
identity even if layouts stay the same. Do not reserve the next numeric ABI in
this proposal: the implementation must advance from the then-current version.
Versioned bootstrap artifacts are regenerated and reproduced without Java.
An incremental session cannot change profile; reset constructs another session.
Cross-profile language-module linking fails even for coincidentally equal Wasm
layouts. Interoperation uses explicit typed WIT boundaries between instances.
Phase imports, macro caches, AOT, REPL, CLI and browser all enforce these gates.

## Executable acceptance targets

These are **proposed suites**, with no passing counts asserted here. Add actual
test names and raw receipts to #50–#57 as they execute. Every semantic assertion
must observe independently decoded values and separately asserted effect traces;
valid encoding, a guest equality result or a guest printer alone is insufficient.

| Suite target | Required observations |
| --- | --- |
| nominal-values | Every case, nested Result/Option, Unit, tactical nil/false payloads, unrelated vectors/maps/user types, equality/hash map keys, printing/read-back and malformed tag rejection in Macro and Runtime |
| nominal-fragments | Construct in one fragment, consume in another, redefine unrelated types/names, retain old closures, force GC, reject forged descriptors/case arity; prove no writable field escape |
| outcome-composition | Exact-once source-order traces, first-failure short circuit, empty/nested bindings, eager defaults, lazy matching, wrong nominal types, exhaustive syntax and callback return checks |
| outcome-scopes | Nested closures/macros, destructuring, dynamic binding restoration, tail-position diagnostics, normal/Err/None cleanup, suspended rooted payloads and resource ownership rejection |
| wit-nominal | Independent generated Rust producer/Suss consumer and Suss producer/Rust consumer, all nested cases, payloadless Unit, schema-invalid payloads/discriminants and no hidden imports; synchronous and asynchronous selected worlds |
| compatibility-adapters | Present nil/false versus missing, explicitly lossy round trips, arbitrary thrown payload preservation, ExceptionInfo provenance, no panic/trap/fuel/cancellation capture; unchanged Clojure/jank cases |
| panic-boundaries | Panic versus language exception/Err/trap/fuel/cancellation, catch cannot resume, ordered cleanup faults, failed-definition nonpublication and retained prior effects, poisoned-store reset |
| async-lifetimes | Suspend/resume/cancel races, at-most-once cleanup, rooted values, resource move/borrow/close, EOF versus element None/Unit, terminal callbacks and bounded cleanup abort |
| target-outcomes | Actual command exit/status, component typed Err versus execution failure and browser resolve/reject/cancel observations under each supported feature profile |
| profile-migration | Nil/throw/catch locations and fixes, native core matrix, profile mismatch before initializer effects, stale binding/bootstrap/cache rejection, unchanged-oracle coverage kept separate |

Concrete value/trace targets include:

```clojure
;; Result is Ok(Err(:domain)), not Err(:domain).
(result/result-let [] (result/err :domain))
;; Some(false), not None.
(option/option-let [x (option/some false)] x)
;; Only :a and :b are appended, once each; returns the original Err.
(result/result-let [x (do (swap! trace conj :a) (result/ok 1))
                    y (do (swap! trace conj :b) failure)
                    z (do (swap! trace conj :c) (result/ok 3))]
  (swap! trace conj :body))
```

Fixtures must also execute invalid inputs, not blanket-skip unsupported targets.
Report passing, failing, skipped and unexecuted cases separately, and document
blocked targets as blocked. Focused regressions precede the full
`cargo test --workspace --locked -- --test-threads=2` baseline. Adoption also
requires independent review, successful final-head CI and reviewed migration
evidence. A rejected or deferred stage is a disposition, never a completed
implementation. No merge is authorized by accepting this document.
