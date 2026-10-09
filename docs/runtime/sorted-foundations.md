# Retained sorted protocol prerequisite


## Current-base refresh after #226

The draft now uses main `4e0f00d`, preserving all 325 existing recipe selections
and 426 reviews, including queue provenance. Three additional whole protocol/type
forms give 332 generated artifacts and 429 reviews. All 23 licensed sequence
setup forms verify. A fresh pinned run matches all 27 interface observations.
The five production Rust patches and two new runtime modules are byte-identical
to reviewed checkpoint `8429c26`; this refresh changes their base, not behavior.

Bootstrap regeneration and every authored native gate remain pending. Array/
function/native-object constructor properties and object ToPrimitive remain
unsupported by the bounded type prerequisite; full public comparators, sorted
collections and nominal records are not implemented by these protocol probes.
Older counts, extracted indexes and process handles below describe their earlier
snapshots. Current commands/results are recorded in the handoff.

## Earlier prerequisite record

Historical sections below describe the original frozen baf134d draft, including
its old extraction indexes and Cargo owners. The current-main refresh section
at the end is historical; the 4e0f00d snapshot above governs the current candidate.


This bounded original #18 prerequisite imports complete unchanged pinned
`ISorted` and `IComparable` declarations. It does not implement sorted trees,
comparators, nominal records or the complete original issue acceptance criteria.
Both reviews remain `:in-progress`; native execution is pending.

The upstream checkout is ClojureScript 1.12.134 at
`c4295f303100bbf5afac449242d30bca1126f1a1`. The source is
`clojurescript/src/main/cljs/cljs/core.cljs`:

| Declaration ID | Source range | SHA-256 |
| --- | --- | --- |
| `runtime:ISorted:782` | 782–797 | `b1e207d553f428b54033a078b2093aa2b18babcd27020f13607938b070b0c7d1` |
| `runtime:IComparable:865` | 865–869 | `88f0015b03c6b1abe54df3846a812e146076e59cb95105c6fe39bd1d2678ed6f` |

Both recipe entries use `patch: null`, classification `:portable`, public
visibility and nil declaration arities. Their executable prerequisite is the
existing bounded nominal protocol adapter; upstream `macro:defprotocol:2041`
compilation remains unfinished. Protocol methods have these public fixed arities,
including their receiver:

| Protocol | Method | Arity |
| --- | --- | --- |
| ISorted | `-sorted-seq` | 2 |
| ISorted | `-sorted-seq-from` | 3 |
| ISorted | `-entry-key` | 2 |
| ISorted | `-comparator` | 1 |
| IComparable | `-compare` | 2 |

The importer retains every docstring, `^clj`/`^number` annotation and signature.
Selections append to the existing recipe so existing extracted paths remain
stable. New originals are `runtime/core-import/extracted/0319.cljs` and
`0320.cljs`; the manifest records exact byte ranges, pin, source/extracted/adapted
hashes, review dependencies and test references. Existing EPL-1.0 notices,
upstream LICENSE and epl-v10.html remain in the shipped artifact directory.
There is no source patch, omitted branch or replacement host implementation.

## Original probe and evidence

`tests/oracle/fixtures/sorted-interface-probe.sus` is an original shared nominal
fixture. Its scalar method results expose receiver values, key arguments,
direction and comparator invocation. It is not a sorted collection: the scalar
`-sorted-seq` results test dispatch only, not the sorted-sequence contract.
An unrelated type with identical fields supplies negative nominal checks.

`tests/oracle/sorted-interface-cases.json` contains 27 reviewed scalar
observations. The same fixture and case expressions feed both the pinned runner
and `crates/suss-cli/tests/core_sorted_interfaces.rs`. Checks cover all five
signatures, both directions, all comparison signs, returned comparator identity
and invocation, captured functions, nominal markers and once-only callee/operand/
method effect order. Independent native inspection reads Boolean sentinels and
f64 bits; it does not use guest equality or printing to judge results.

Two new native tests are authored but not executed. Both construct a real nominal
instance in Runtime and Macro Sessions, force GC before inspection, and retain
instances and functions across fragments. Additional native assertions cover
complete-source reload, stable protocol bindings, `cljs.core`/`suss.core` aliases,
public method arity errors, nonimplementor errors, recovery, public redefinition
and old captured function behavior. Error assertions inspect the actual rooted
payload after GC: wrong arity requires descriptor ID 1 and exact UTF-16
`Wrong arity`; the unrelated receiver requires descriptor ID 7 and exact UTF-16
`Invalid nominal operation`. ID 7 is verified from numeric.rs `ERROR_GLOBALS = 6`
and runtime_abi.rs's nominal error descriptor initialization (`+ 1`). These
strengthened assertions remain uncompiled and unexecuted. This uses the existing bounded compiler
adapter, not evidence that the upstream defprotocol macro has been compiled.

Executed commands/results are recorded in the handoff. The development-only
runner `sh scripts/test-sorted-interface-oracle.sh` checks the pinned inventory,
compiles the original fixture with Java/ClojureScript, executes Node, and compares
strict tagged observations. It deliberately has no Cargo invocation. Generated
oracle outputs remain ignored; JVM/Node are not shipped dependencies.

## Deferred execution and next prerequisite

No Cargo command was run while baseline session 50155 was reserved. Import source,
recipe and review changes affect the compiler fingerprint. Shipped Runtime/Macro
bootstrap artifacts are intentionally unchanged; fresh bootstrap regeneration,
Java/Node-free identity verification and native execution remain pending.

When the coordinator releases the Cargo slot, regenerate bootstrap first, execute
`cargo test -p suss-cli --test core_sorted_interfaces --locked -- --test-threads=2`,
then the existing interface/import tests and required provenance/bootstrap gates.
The required full baseline remains
`cargo test --workspace --locked -- --test-threads=2`; independent PR review and
final-head CI remain required for readiness. Do not reinterpret a stale-bootstrap
rejection as a semantic result.

The next implementation prerequisite is a complete reviewed comparator slice:
`runtime:compare:2499` and `runtime:fn->comparator:2541`, including an explicit
portable adaptation for `garray/defaultCompare`, `IComparable` dispatch, nil/type
checks and typed errors. Preserve the array branch and unsupported coercion
limitations. PersistentTreeMap/TreeSet algorithms and nominal defrecord behavior
remain separate original #18 obligations.

## Independent P2 static review — 2026-10-08

The coordinator reported Meitner's final independent static review with no
material findings. The review confirmed descriptor IDs 1/7, exact UTF-16 messages,
rooted payload inspection after GC, retained recovery checks and coverage of both
Runtime/Macro phases. The P2 static finding is resolved; this does not establish
Rust compilation or native execution. The coordinator also reproduced the 325-file
recipe with exit 0. Native execution and bootstrap regeneration remain pending.
Vector full-baseline PID 65986 owns the sole Cargo lane; no sorted Cargo command
is authorized until the coordinator releases and schedules that lane.

## Comparator prerequisites authored — native and regeneration pending

The next bounded source changes add immutable private `suss.bootstrap/number?`
and `suss.bootstrap/string?` call-site lowering. They invoke the existing runtime
`predicate-number`/`predicate-string` factories directly, without looking up public
predicate cells or adding predicate algorithms/runtime exports. Existing public
functions remain live. Operands are analyzed once before invoking the factory.

`runtime/comparator-foundations.sus` is original Suss source, currently loaded
explicitly by the new regression rather than wired into generated artifacts.
Its private scalar helper captures its scalar guard and uses immutable qualified
comparison lowering for `>` followed by `<`. Scalar numbers, strings, booleans,
nil and undefined are admitted; arrays and objects raise an explicit language
error. This is a scalar helper prerequisite, not a replacement for full Closure
`defaultCompare` or the pinned `compare` declaration. The original implementation
plan and all 64 strict primary oracle cases/results are retained at
`/private/tmp/suss-m4-comparator-plan.md` and
`/private/tmp/suss-m4-comparator-oracle/`. Rechecking saved observations is not a
fresh oracle execution of the new Suss code.

The recipe now stages `runtime:type:347`, source lines 347–351, SHA-256
`e489b1c6c1c98957c2f71d370ed7fb9bd825c8bf6e94d676b0e7ba2e53304aa7`, with
`docs/compatibility/patches/sorted-type.json` and an in-progress adapted review.
The whole form retains its documentation, one-argument signature and both the
nil and constructor paths. Its dependencies include `macro:defn:3364`,
`macro:when-not:427`, `macro:nil?:923` and the private constructor adapter.
The original pinned bytes and EPL packaging will be emitted by the existing
importer once regeneration is authorized. The staged recipe has 322 selections;
the generated import still has the prior 321 selections/325 files. No generated
file was rewritten for this comparator prerequisite.

The private `value-constructor` adapter now supports numbers, strings, booleans
and source nominal instances, including the existing ExceptionInfo class. Each descriptor owns its
actual class value through a private descriptor key in its existing table.
Class factories return that retained value; instance lookup neither allocates a
fresh constructor nor reads a public name. The key is an appended runtime global,
with no global owner registry, recursive GC layout change or source binding hook
change. This is authored code, not validated native behavior.

**Material design gap:** source-array, ordinary-function and native-object
constructor lookup remains unsupported and raises a typed nominal
language error. They cannot be replaced by kind markers or newly allocated class
values. The next adapter must establish canonical builtin constructor values and
preserve owned/prototype `constructor` lookup, including overrides and null
prototypes. The current nominal path does not resolve that larger source/host
binding boundary. The array path of full compare therefore remains blocked;
no whole compare form has been imported, shortened or marked implemented.
Ordered array ToPrimitive, repeated coercion for `>` then `<`, UTF-16 comparison,
error `str_` evaluation/formatting and forward `compare-indexed` remain required.

`core_comparator_foundations.rs` authors three both-phase native regressions:
public predicate/comparison redefinition before helper loading and later,
captured functions across GC, UTF-16/NaN/negative-zero/boolean/scalar comparisons,
once-only operand order and throw suppression; and nominal type identity across
public class/type/nil-predicate redefinition, alias lookup, GC, arity errors and
recovery. Errors decode the rooted actual descriptor and UTF-16 message.
`portable_comparator_foundations.rs` adds wrong-arity/non-first-class compiler
guards for the private operations. None of these Rust tests has been compiled or run.

Executed checks: 10 Python review-validator tests passed; overlay verification
reported 423 reviewed/642 unassessed; full-form type hash/name/doc and helper
source scanning passed; all retained 64 strict observations still match.
Rustfmt checked the new Rust files and parsed changed existing files using the
workspace's Rust 2024 edition without rewriting their unrelated formatting.
An initial Rust 2021 parse was rejected by existing let chains; corrected Rust
2024 parsing passed. `git diff --check` passed. No Cargo, artifact regeneration,
commit or push occurred. Independent review of these new prerequisites is pending.

The follow-up scalar constructor dependency is now authored in
`runtime_abi/primitive_constructors.rs`: three lazy runtime-owned singleton
closures supply actual callable Number/String/Boolean values. Number/String use
existing scalar coercion exports, accept zero/multiple arguments and convert only
the first after all argument effects. Boolean conversion explicitly treats zero,
negative zero, NaN, empty string, nil and undefined as false, and objects as true.
This conversion is **not** the truthiness to use in `fn->comparator`, where numeric
zero/NaN and empty strings are true. Constructor names and roots remain private
runtime storage; no public type gate or predicate cell is frozen or rewritten.
No source binding hook or recursive ABI layout change was needed for this slice.

Additional authored regressions independently compare rooted constructor
references with Wasmtime `Rooted::ref_eq` after GC and public redefinition; check
metadata-bearing instances, untouched descriptor metadata and protocol table
pairs/dispatch; inspect false/nil thrown payload sentinels after GC; and check
the shared recursive group remains ten types with the five-field descriptor.
Scalar constructor tests cover canonical/distinct identities, real conversions,
zero/extra-argument semantics, argument effects, UTF-16 output and exact typed
unsupported object-coercion errors. All remain uncompiled/unexecuted.

The next unblocked implementation dependency is the canonical Array constructor
and source-array constructor lookup, using existing array construction/length
exports with explicit bounded limits. Then implement ordered array/object
ToPrimitive (including repeated coercions and live toString/valueOf behavior),
error formatting and the full defaultCompare adapter. Preserve the live public
`type` gate when adapting the entire `compare` form; the private type operation is
only the body dependency of the staged public type function. Function/Object
constructor/prototype semantics remain explicit boundaries needing review.

## Frozen prerequisite draft for independent review

At the coordinator's request, the current predicate/type/scalar-helper draft is
frozen before further comparator implementation. This includes the just-authored
canonical primitive constructor dependency; it is not executing acceptance.
Array/function/native-object constructor identities, ordered array/object
ToPrimitive and full error formatting remain pending. No full compare adaptation
will proceed until this prerequisite is executing and verified.

Final read-only checks passed: 10 review-validator tests; overlay 423/642;
whole-form type provenance/doc/identity and original helper scanning; 16 literal
native-test setup sources scanned; all prior 321 generated form IDs/hashes intact;
exact local pin; 64 retained observation matches; Rust 2024 rustfmt checks for
new files and syntax parsing for changed existing files; `git diff --check`.
These are Python/static/format checks, not Rust compilation or Wasm validation.
Three both-phase native tests and two compiler/runtime-layout guards remain
uncompiled and unexecuted. Independent review of this new draft is pending.

Authoritative full worktree review diff and file-hash receipt are outside the
repository at `/private/tmp/suss-m4-sorted-foundations-review.diff` and
`/private/tmp/suss-m4-sorted-foundations-freeze.json`. No Cargo, artifact
regeneration, commit or push occurred. The coordinator owns scheduling the first
focused native lane after baseline 65986 reaches terminal status.


## Earlier main refresh (2026-10-08)

The original frozen source remains in `/private/tmp/suss-m4-sorted-foundations`.
The executing candidate is now `/private/tmp/suss-m4-sorted-current`, branch
`portable/m4-sorted-current-base`, based on main22723b4. All five reviewed Rust
source patches are byte-identical to the frozen draft; one test line was formatted.
All original main recipe selections are preserved, with three additive whole
forms for type/ISorted/IComparable. Generated imports now verify327files and the
review overlay424reviewed/641unassessed. All228Python checks pass8.954s; fresh
pinned27protocol observations pass. Rustfmt of the five new Rust files and diff
checks pass. Initial regeneration rejected the uninitialized upstream checkout;
normal submodule initialization checked out the exact pinned revision and the
rerun passed. No expectation, assertion or compatibility status was weakened.

Bootstrap artifacts are still those of main and require regeneration against
these compiler/source changes. No sorted Rust native gate has run. After the
current Cargo graph terminates, use a dedicated new target, regenerate both
bootstrap phases, then run focused comparator ABI/compiler guards and both-phase
protocol/scalar-constructor tests, followed by Java/Node-free bootstrap proof,
independent final review, full locked baseline and final-head CI. Full public type
support, array/object conversion, complete compare family and persistent trees/
records remain original M4 work; this draft does not satisfy them.
