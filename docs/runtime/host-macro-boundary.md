# Native host/Macro boundary and Var publication

This contract specifies the core boundary used by native compiled sessions and
isolated source macros. Its implementation and execution gates are tracked in
[#519](https://github.com/bobby/suss/issues/519) and
[#520](https://github.com/bobby/suss/issues/520). It does not certify those gates.
Public language `eval` is a consumer tracked in #515/#516; numeric categories,
arithmetic and literal Object behavior remain #338/#339 and #518.

## Ownership decision and alternatives

Retain the accepted design's separate persistent Runtime and Macro Stores.
Each phase owns its runtime, cells, namespaces and scheduler. A live value keeps
its owning Store; crossing a boundary does not move the value into another heap.
A synchronous callback borrows the actual Wasmtime Caller context or selects a
uniquely suspended ancestor. The Rust type/lifetime boundary, rather than a copied
Session or pointer, limits that authority.

A single shared Store would reduce cross-heap forwarding but would require a new
phase-isolation, nested-execution and reset contract. That is not selected by this
extraction. Serializing all values loses live cells, captures and identity;
replaying source duplicates effects and cannot reconstruct arbitrary state.
Neither is a valid substitute for live references. Worker/process isolation and
comprehensive memory admission require the separate analysis in #253; this
contract does not promise recovery from allocator abort or an OS kill.

The cost of retaining separate Stores is an explicit, tested live-value boundary.
New value categories must declare their representation and lifetime here rather
than adding implicit name-based or UID-based authority.

## Entry, borrowing and retirement

1. An owned Session owns one Store. A borrowed SessionHost uses the current
   StoreContextMut and shared compiler metadata, never an owning Session reference.
2. A host lending its Store while another Store executes is registered only for
   that synchronous scope. Selecting it removes it from the registry until the
   callback returns; only its new Caller may lend it again. Any unselected ancestor
   remains available, including a non-immediate ancestor.
3. Registry and compiler locks protect short metadata operations. Drop them before
   instantiating Wasm, invoking a guest, or entering another service. A service
   lease is returned on success, language failure, interruption and unwind.
4. Native roots/capabilities are owner-qualified. Retirement/reset invalidates old
   authority before touching its GC roots. Root reachability does not authorize
   a retired owner, and matching names or numeric UIDs never revive it.
5. Do not replay source or initializer bodies to recover a session or service.

## Boundary values

| Representation | Values and obligations |
| --- | --- |
| Plain data | nil, booleans, scalar numeric ABI values and UTF-16 strings. Preserve category, floating bits and every UTF-16 unit. Charge traversal work. |
| Authenticated live capability | Functions, classes, Vars, Namespaces, Unbound objects and mutable state arrays. Retain the actual owner root and route supported operations there; destination proxies are not replacement cells. |
| Scoped immutable graph | Supported immutable metadata/collection graphs may contain live capabilities. Preserve repeated roots, metadata sharing and callbacks; releasing the bundle releases its temporary endpoints. |
| Rejection | Wrong owners/descriptors, forged tokens, retired owners and unsupported opaque values fail explicitly. Display/syntax readers do not acquire private native admission. |

Indices and UIDs narrow candidate searches only. Authenticate exact roots and
private capabilities after selecting candidates. Returning a live object to its
source must unwrap the original root. Strings are actual i16 arrays, not lossy
UTF-8 conversions. Numeric semantics themselves are defined by ADR-0003.

## Context, errors and resources

The executing owner supplies namespace state and cells. Borrowed execution uses
its real dynamic frames; foreign execution inherits only explicitly forwarded
context and restores it on exit. Pending language exceptions, cause/data objects
and cancellation categories must survive forwarding. Cleanup may restore finite
runtime frame/cell state; it cannot execute arbitrary source while bypassing
cancellation.

Each nested operation debits existing compilation, expansion, runtime and host
traversal scopes. A callback cannot refill its parent's remaining fuel. Host
loops must charge work and observe interruption. Private scalar transport may
avoid unnecessary guest classifiers, but never skip owner/capability checks on
live objects. The original pinned case200M/map300M guards remain unchanged.
Fuel and host process limits do not establish a complete memory admission policy.

Pure components receive no hidden compiler/namespace service. Calling an
uninstalled capability reports the explicit language error; producing a valid
component does not imply it has dynamic compilation authority.

## Var definition and effect transitions

A Var is one nominal object over the canonical phase/namespace/name binding cell.
The cell stores its physical root, flags and genuine Unbound root. A captured
function remains the old function after redefinition; dereferencing a retained
Var reads the current root or applicable dynamic binding. Canonical core aliases
share a cell, while unrelated phase namespaces do not share by spelling.

| Transition | Required observable state |
| --- | --- |
| Analyze a declaration | Intern the genuine Var where required. Analysis alone does not publish the proposed initializer value or execute an untaken/uncalled definition. |
| Initializer returns | Publish that returned root and evaluated declaration metadata on the existing Var at the definition's execution boundary. |
| Initializer throws | Retain the prior root and declaration metadata; preserve effects already executed by the initializer. |
| Ordinary root write/redefinition | Change the actual root; preserve identity. Ordinary binding clears macro status according to the language contract. |
| Dynamic binding/assignment | Change the selected dynamic frame, independently of the physical root. Restore the frame on exit, including supported interruption. |
| alter-var-root | Apply to the physical root and write only after successful return. Preserve macro status and already executed callback effects. |
| Stage source macro | Record compiler-owned declarations separately from executed language writes. Publish the actual isolated transformer after the source completion/link boundary. |
| Reject staged input | Undo only still-current compiler-owned staging writes. Preserve completed imports, identical user root writes and later metadata effects. |
| Import/reload completes | Advance active declaration journals so rejecting later caller source cannot restore a superseded root. Retained Vars see replacement roots; retained old functions remain callable. |

A genuine Unbound object belongs to its Var, retains identity across GC and root
replacement/restoration, is callable with the language's unbound-function error,
and has no Fn marker. Namespace catalogs validate the registered Namespace and
return actual user/core/source-macro Vars. The documented symbol-valued ns-refers
platform contract remains separate. Validators, watches and STM are not implied.

Declaration metadata expressions execute exactly once in the live user/lexical
environment. Unbound metadata symbols are errors; fixture symbol data needs an
explicit binding or quote. Literal and quoted compiler data use original core
constructors even after user replacements. User calls inside metadata still see
live replacement bindings. Preserve raw declaration ASTs and primary projections;
correct fixture setup without changing expected observations.

## Implementation and executing controls

| Responsibility | Entry points and controls |
| --- | --- |
| Actual Store authority | portable_session/store_access.rs, reentry.rs; borrowed_session_reuses_data_and_invocation_without_refilling_or_retiring_owner |
| Suspended ancestor selection | portable_session/suspended.rs; suspended_ancestors_reenter_three_actual_stores_and_charge_guest_work_once; suspended_selection_retires_on_panic_and_preserves_roots_and_reset_identity |
| Shared compiler publication | compiler_state.rs, installation.rs; portable/resolve/publication.rs and native_effects.rs |
| Live capabilities and scoped graphs | evaluation/native_values.rs and its submodules; native_transport_uid_collisions_preserve_distinct_functions_and_repeated_aliases; native_function_capability_rejects_a_retired_source_before_touching_its_gc_root |
| Nested cancellation | suspended_parent_interrupt_stops_a_foreign_guest_and_retires_both_budgets; suspended_parent_interrupt_is_also_observed_by_foreign_host_work |
| Genuine Vars/metadata/source catalogs | portable/hir/vars.rs; CLI native_host_boundary target; runtime_evaluation capability target |
| Compiler-owned data under replacement | native_host_boundary metadata controls; portable_aot_options::option_schema_survives_core_function_and_constructor_redefinition |
| Original transport budgets | clojure_test_suite pinned_case_body_matches_all_primary_assertions and pinned_map_declaration_executes_without_transport_loss |

The extraction retains original primary source and expected observations. Existing
public-eval-dependent controls remain on #516, with exact test-body conservation
recorded separately. They do not count as independent foundation validation.
Historical receipts on #516 are historical; run this source's focused checks,
bootstrap and exact two-thread workspace baseline before claiming completion.
Independent review/fixes and final-head CI gate promotion.
