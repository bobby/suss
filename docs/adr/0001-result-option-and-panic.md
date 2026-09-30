# ADR-0001: Nominal results and options, explicit absence, and panic

Status: **Proposed**. Neither stage is accepted or implemented by this ADR.
Date: 2026-09-29.

## Context

The [accepted design](../design/suss-0.3.1.md) targets portable ClojureScript
semantics, including `nil`, truthiness, and language exceptions. Its WIT boundary
maps results and options to tagged vectors. The [roadmap](../../ROADMAP.md),
[compatibility inventory](../compatibility/README.md), and
[handoff](../roadmap/handoff.md) remain the implementation/evidence authorities.
Existing shared-runtime code uses nil/Boolean sentinels and language exception
tags; this proposal does not establish a replacement runtime.

Recoverable operational failures should be explicit return values that callers
can inspect, transform, or propagate. Absence should be distinct from failure,
successful completion without a meaningful value, sequence exhaustion, and an
external null. Rust's Result/Option approach and WIT's corresponding boundary
types provide a useful foundation for a Clojure-family language.

Structural conventions such as `[:err payload]` and `[:none]` are insufficient:
they collide with legitimate application data, including event vectors in systems
such as pedestal-app. Recognizing a shape must never assign propagation semantics
to ordinary domain data.

This proposal has two adoption stages. The strategic target deliberately departs
from ClojureScript semantics. The tactical stage introduces useful nominal types
without first requiring that language migration.

## Proposed shared types

Names and examples below are illustrative API designs, not executable features.

| Type | Cases / meaning |
| --- | --- |
| `Result` | Closed nominal cases `Ok(value)` and `Err(error)` |
| `Option` | Closed nominal cases `Some(value)` and `None` |
| `Unit` | One nominal value representing completion without a meaningful payload |
| `Error` | Optional protocol for diagnostic inspection of error payloads |

`Result` and `Option` are types, not open protocols. User implementations cannot
acquire propagation semantics by supplying methods or imitating storage layouts.
Constructors, predicates, explicit matching, and combinators operate on nominal
identity. A consumer expecting a result or option rejects other values with a
located contract diagnostic; it never treats them as implicit success or absence.

```clojure
(result/ok [:err :domain-event]) ; successful vector, not a failure
(result/err (->InvalidPort "eighty"))
(option/some false)             ; present false, not absence
(option/some (option/none))     ; distinct from outer None
```

Cases participate in equality and hashing by nominal type, case, and payload,
following the language's payload equality/hash rules. They never equal vectors
or maps with analogous fields. Printing and independent observation decoding
identify each type/case explicitly. Reader syntax, if added, must preserve these
distinctions and reject malformed values. Resource payloads retain their existing
ownership restrictions; wrapping a handle does not make it copyable.

Descriptors must remain stable across fragments, redefinition, and GC. Layouts,
intrinsics, and incompatible changes require runtime ABI/version gates. This ADR
does not prescribe a Wasm encoding or Rust-style allocation optimization.

### Error payloads and diagnostics

`Err` may contain any payload permitted by its API or WIT schema. Implementing
`Error` is optional, particularly for WIT enums, strings, and record-shaped data.
The protocol provides a human-readable message, structured diagnostic data, and
an optional cause expressed as `Option`, rather than introducing another nil
convention. Exact method names and data requirements remain to be settled.

Recovery dispatches on documented domain types/codes, not message text. A value
implementing `Error` can occur inside `Ok` as ordinary data. Protocol membership
does not imply failure, and there is no mandatory universal error hierarchy.
Diagnostics need a fallback for payloads without the protocol. Converting to
legacy ExceptionInfo must preserve the original payload in documented data.

## Strategic target: complete native adoption

### Recoverable failure and propagation

Native fallible APIs return `Result`; expected I/O, parsing, validation, and domain
failures do not throw. Use `Option` when absence alone describes the outcome,
and `Result` when a reason for failure matters. Operations can combine them:
`Result<Option<T>, E>` distinguishes failure, successful absence, and a found value.

Provide exhaustive case matching and compositional operations such as mapping,
error mapping, binding, defaults, and explicit Option-to-Result conversion.
A lexical propagation form avoids repeated nested conditionals:

```clojure
(result-let [text   (fs/read-text path)
             config (config/parse text)
             port   (config/validate-port config)]
  {:config config :port port})
```

Each initializer returns a nominal result and evaluates exactly once, left to
right. The first `Err` exits this lexical form unchanged; later initializers and
the body are not evaluated. Success bindings receive unwrapped values; the body
value is wrapped in `Ok`. A body returning a result therefore produces a nested
result, unless an explicit flattening/binding operation is used. An analogous
option form short-circuits on `None`, wraps its body in `Some`, and preserves false.
Early exit runs applicable lexical cleanup and restores dynamic bindings.

These are dynamic nominal types, not a proposal for a Rust static type system.
Known malformed matches can be diagnosed at compilation; dynamically wrong types
require runtime checks. Tooling should diagnose discarded results where justified,
but static prevention of all ignored failures is not an acceptance claim.

### Panic, cleanup, and execution boundaries

Native application code has no general `throw`/`catch` mechanism. `panic` terminates
the current supervised execution for violated invariants or bugs. It is not a
routine recoverable-error transport. Known source errors remain compiler
diagnostics; invalid dynamic arity/type usage is a programmer fault. APIs provide
checked Result/Option alternatives where callers can reasonably recover.

Panic unwinds through language-managed lexical cleanup and dynamic scopes to a
documented execution boundary (REPL evaluation, supervised task, or command).
Application code cannot resume a panicking computation through catch. A supervisor
may report failure and continue other work only where isolation permits it; prior
effects are not rolled back. A failed definition initializer must not publish a
replacement binding.

Wasm EH may implement internal unwinding; removing source throw/catch does not
require removing engine exception machinery. Raw Wasm traps, fuel exhaustion,
host aborts, and other failures that prevent unwinding remain distinct. They have
no blanket guest-cleanup guarantee. Abort during cleanup, multiple cleanup failures,
resource-close failure precedence, and supervisor reset policy require explicit
decisions and executing tests before shipping panic semantics.

Cleanup remains a language feature even when try/catch is removed. Lexical resource
scopes and unconditional cleanup must cover normal return, propagated Err/None,
panic, and cancellation. Finalization is not a resource lifetime mechanism.

### Replacing nil's separate roles

Native Suss has no general-purpose `nil` value or literal. An ordinary expression
cannot silently acquire absence through a missing result.

| Current use | Native replacement |
| --- | --- |
| Missing key, element, or optional argument | `Option` |
| Effect-only completion, empty body, omitted conditional branch | `Unit` |
| Sequence exhaustion | Explicit exhaustion, surfaced through an Option-returning sequence API |
| External JSON/JS null | Explicit boundary null value, separate from None and Unit |
| Uninitialized var/internal empty slot | Dedicated internal state, not an application value |

Collection APIs must be redesigned consistently: lookup and first return options;
lookup-with-default remains convenient and explicit; empty collections remain
collections. Sequence traversal exposes exhaustion without conflating it with an
element. Destructuring represents missing bindings explicitly, with documented
defaults. Keyword invocation, `get-in`, `next`/`seq`, `if-let`/`if-some`, validators,
delays, and macros need individual migration contracts. Unit must not inherit
nil-as-empty-collection behavior. Explicit null is data, never automatic absence.

Proposed native truthiness preserves Lisp conditional convenience: only `false`
is falsey. `None`, `Some(false)`, `Err`, Unit, and boundary null are truthy and do
not unwrap in conditions. Presence/success tests and binding/matching forms are
explicit. Boolean-only conditions are an alternative requiring a separate decision.
This rule must be visible in migration diagnostics and documentation.

### Async, WIT, CLI, and browser behavior

Awaiting `Future<Result<T, E>>` yields a result. An `Err` payload is successful
task completion with a failed operation; it is not panic or scheduler failure.
Nested futures/options/results are not implicitly flattened. Cancellation is a
separate scheduler outcome with explicit adapters when an API represents it as
data. Stream EOF remains distinct from an element, including None or Unit.

Generated WIT adapters map results/options nominally and validate payloads against
the resolved schema. Payloadless result cases map to Unit; the schema determines
whether that case has a payload on the wire. Unit is not a new WIT primitive.
`None` differs from `Some(None)` for nested options. Protocols, traces, and arbitrary
error objects do not cross WIT unless the declared payload schema represents them.
A panic or trap must never be silently fabricated into a declared WIT error.

Command entrypoints need an explicit success/error/status policy, rather than
interpreting any result-looking data or number as an exit code. Browser bindings
preserve resolved Err versus rejected execution. Execution-boundary failure
reporting must obey the selected world's actual contract; boundary policy is not
inferred from a Rust convention.

## Tactical adoption: nominal WIT types alongside compatibility semantics

Introduce the same Result/Option/Unit types, predicates, matching/composition, and
explicit adapters while retaining the accepted ClojureScript exception/nil rules.

1. Generated WIT result/option bindings use nominal values instead of the accepted
   design's tagged vectors. This limited boundary-contract change requires its own
   dated acceptance entry, versioned binding change, and interoperability evidence.
2. Existing compatible core functions preserve return values, nil behavior,
   truthiness, throw/catch/finally, and ExceptionInfo behavior. New Suss APIs can
   offer Result/Option operations in explicit namespaces without changing callers.
3. `Some(nil)`, `Ok(nil)`, and `Err(nil)` are legal and distinct from None or Unit.
   Both Result cases and both Option cases are truthy under compatibility rules;
   wrapping false or nil does not inherit their falseyness.
4. Exception-to-Result adapters catch only declared language exception cases,
   preserve payloads, and run cleanup. They do not catch all engine/host failures.
   Result-to-exception adapters are explicit and retain diagnostic provenance.
5. Nil-to-Option adapters document the information they discard. A lookup adapter
   must check presence to distinguish a missing key from a present nil value.
   Option-to-nil loses the difference between None and Some(nil); it is a named,
   explicitly lossy compatibility conversion. Unit-to-nil is also explicit.
6. Panic can be introduced only with a defined boundary/cleanup contract. It must
   be distinguishable from legacy catchable language exceptions, even if both use
   internal EH. Tactical Result/Option adoption need not wait for panic support.

No automatic exception capture, option unwrapping, nil conversion, or structural
vector interpretation occurs at calls. Compatibility behavior has its own tests;
new semantics cannot reclassify existing failing cases as passing.

## Migration and acceptance gates

Acceptance of the strategic goal and authorization to ship either stage are
separate decisions. Suggested implementation sequence:

1. Specify and execute nominal types across shared GC fragments, with strict
   independent decoding, equality/hash, printing, nesting, and schema validation.
2. Exercise bidirectional Rust/Suss WIT fixtures and explicit compatibility
   adapters; then accept the limited tactical boundary change.
3. Implement propagation and cleanup with ordered effect traces. Establish panic
   and supervision separately, including suspension and cancellation.
4. Audit every affected core API, macro, bootstrap dependency, and source form;
   publish native contracts and conversion guidance before a native preview.
5. Move the native contract to full adoption only after source/core/target migration
   and lifecycle acceptance. Decide whether a supported compatibility surface
   remains, and document its limits and cost.

If native and compatibility contracts coexist, semantic identity must be explicit
in artifact manifests, binding generation, cache keys, and module linking policy.
Do not silently mix core bindings or reinterpret literals based on a callee.
Profiles must share one compiler/runtime architecture rather than indefinitely
maintaining two production compilers. Exact profile syntax and whether a permanent
compatibility layer is warranted remain open.

Required evidence includes domain-vector collision regressions; Some(false) and
tactical Some(nil); nested options/results and payloadless WIT cases; cross-fragment
identity after GC; malformed-boundary rejection; left-to-right/exact-once effects;
cleanup on every exit; panic versus exception versus trap; failed initializer
publication; async/cancellation/resource lifetimes; and CLI/browser outcomes.
Run focused executing tests first, then the required full baseline. Planned tests
are acceptance targets, not current passing evidence.

Adopting the strategic contract requires a dated revision to the accepted design,
roadmap/release gates (especially M7), inventory review schema as needed, and
source-level migration report. Stable issue IDs remain; affected acceptance
criteria change explicitly. The pinned ClojureScript oracle remains useful for
unchanged semantics and the compatibility surface, while native departures need
their own exact expectations. Never weaken an oracle or mark an unassessed entry
supported to accommodate a planned departure. Adapted upstream code retains its
source provenance and licenses. No shipped JVM/Node dependency is introduced.

## Consequences and alternatives

The target makes expected failure and absence explicit, avoids domain-data
collisions, and aligns native APIs with component boundaries. It changes core
idioms, requires migration of upstream forms, and gives up full ClojureScript
source compatibility in the native contract. Dynamic typing leaves less static
enforcement than Rust; syntax and runtime checks must carry that distinction.

Alternatives considered: tagged vectors (collisions), merely renaming nil to None
(preserves overloaded roles), an open Result/Option protocol (uncontrolled cases),
and permanent Result/Option libraries with unchanged native core semantics (useful
tactically, but does not achieve the strategic goal).

Open decisions: final API/reader syntax; Error protocol signatures; propagation
form names; cleanup-failure policy and panic isolation; compatibility lifetime and
profile linking; detailed core/destructuring contracts; diagnostics for unused
results; external-null representation; and command/export entrypoint policies.

## References

- [Accepted Suss design](../design/suss-0.3.1.md), especially sections 4, 6, 8, and 9.
- [Shared runtime ABI v1](../runtime/abi-v1.md): existing foundation and limitations.
- [WIT reference: options and results](https://component-model.bytecodealliance.org/design/wit.html): boundary cases and payloadless results. WIT defines interfaces, not Suss control flow.
- [Rust: Option and explicit absence](https://doc.rust-lang.org/book/ch06-01-defining-an-enum.html).
- [Rust: choosing Result or panic](https://doc.rust-lang.org/book/ch09-03-to-panic-or-not-to-panic.html).
