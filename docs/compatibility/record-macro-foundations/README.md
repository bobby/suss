# Complete record macro prerequisites

Twelve complete pinned forms are retained with source ranges, declaration and
file hashes. These are planning inputs, not imported or executing macros.

`reify` captures every lexical local, generates IMeta and IWithMeta methods,
transfers reader metadata after elision and guards type publication with exists?.
Its namespace-derived generated name requires actual munging behavior. A named
type or metadata-free substitute would omit the accepted behavior.

`defrecord` requires all emit-defrecord methods, declared fields plus metadata,
extension map and mutable hash, protocol masks, full method annotation and
positional/map factories. Factory extmap processing uses not-empty and cond->>;
record input conversion and metadata handling remain required. Full source
transport and nominal record storage must preserve all branches.

Compiler/analyzer dependencies include resolve-var, namespace/local facts,
reader-meta elision and munging. The exists? symbol path emits JS typeof checks;
portable adaptation needs equivalent defined-cell semantics and nested property
existence, not a constant true/false. No source branch is excluded by staging.

Next: execute iterator prerequisites, then implement these complete helper
contracts and generic nominal record construction. Prove captured-local GC,
metadata, type redefinition, factories, ordered effects, equality/hash and
persistence against fresh pinned execution. Original M4 acceptance stays open.

## Runtime field-substitution dependencies — 2026-10-12

The retained `emit-defrecord` calls public `replace` to substitute field values
when generating clone/metadata/assoc constructors. The complete pinned `replace`
form also has lazy sequence and transducer arities; its vector branch calls live
`take` and `iterate`. These dependencies are now selected in the reviewed runtime
import, rather than replaced with a record-only substitution loop.

Five complete originals are retained as `runtime/core-import/extracted/0424.cljs`
through `0428.cljs`: take, UNREALIZED-SEED, Iterate, iterate and replace. Their
source ranges/hashes, upstream EPL notice and exact extraction/patch hashes are
in the core-import manifest. Iterate is unchanged, including its five fields,
IPending, metadata/cache behavior and both infinite reduction arities. Four
explicit `record-dependency-*` patches expand defn/shorthand forms, use an owned
empty object for the private seed sentinel, and adapt take's private state cell,
typed numeric preconditions and memoized LazySeq constructor. Neither replace
arity nor a sequence/transducer/type branch is omitted. Its live find/val/second,
assoc/reduce/take/iterate calls remain live.

`python3 scripts/core_import.py` wrote 433 generated files. This was source and
provenance generation, not a test run or compatibility result. The three
two-phase native regressions in
`crates/suss-cli/tests/portable_record_sequence_dependencies.rs` are authored but
uncompiled/unexecuted. They check nil/false substitutions, unchanged vector
identity and metadata, lazy demand, Iterate seed/next caching and reduction
effects, take arities/preconditions and retained Reduced identity. Fresh pinned
oracle execution and native execution remain required.

The user's test-suite hold remains in place. Both shipped bootstrap artifacts
predate these additions and must be regenerated and reproduced before validation.
Public defrecord is still missing: compiled macro/helper installation, low-level
record type lowering with hidden fields and accurate declaration facts, factories,
protocol methods and public record equality/hash/metadata/persistence evidence
remain necessary. These runtime dependencies establish none of that acceptance.

## Intrinsic type boundary — authored 2026-10-12

The compiler now has authored lowering for the exact low-level declaration
shape consumed by the pinned record emitter: `defrecord* name fields masks body`.
`deftype*` uses the same body path without record storage fields. The field schema
adds `__meta`, `__extmap` and mutable `__hash` for records, while declaration facts
retain the count of user-declared fields and the intrinsic record kind. The
compiled macro catalog projects that genuine record default; ordinary type
redefinition resets it. Existing source metadata overrides remain explicit.

This original native lowering follows the already retained pinned analyzer
`parse-type` (analyzer.cljc 3614–3649) and compiler `:defrecord` emitter
(compiler.cljc 1463–1480). Protocol-mask source data stays in the retained form;
the shared native protocol descriptor/method table supplies dispatch and markers.
The type value is published before its source body executes. Enclosing lexical
locals are removed for body/method analysis, and the owning type's physical field
declarations are supplied to inline `extend-type` methods, including grouped
arities. Analysis restores the caller's scope and field context on failure.

Two authored two-phase regressions in
`crates/suss-cli/tests/compiled_record_type_boundary.rs` cover the five-slot
physical layout of a two-field record, declared field count/record flag before
body analysis and through the actual compiled macro catalog, field indexes/hash
mutability, body effects, enclosing-local capture rejection and ordinary-type
redefinition. Their probe protocol tests this compiler boundary; it does not
stand in for the public generated record protocols or acceptance of record
equality/hash semantics. No tests or builds ran. Rustfmt syntax parsing and
whitespace checks are not native validation; bootstrap remains stale for both
source and compiler identity.

The public defrecord macro/helper installation and positional/map factories
remain missing. Complete generated protocols, constructor behavior (including
omitted internal fields), metadata/extmap handling, nominal equality/hash,
persistent updates, protocol customizations and retained values across GC still
need pinned/native execution. Source-level type-body analyzer facts beyond the
method field binding must be audited against the complete compiled helper graph.

## Compiled factory helpers — authored 2026-10-12

`portable_record_factories.rs` now contains complete adaptations of the retained
build-positional-factory and build-map-factory forms as compiled Suss functions.
The macro bootstrap installs them in `suss.internal.record-factories` after defn
bootstrap and reloads them through the same initialization path after reset.
Every macro artifact dependency list includes the complete helper source hash.
The [adaptation record](factory-adaptations.json) binds original, retained,
adapted declaration, Rust-file and compiled-source identities and EPL provenance.

The positional helper keeps both ordinary-type and internal-record constructor
argument branches. The map helper keeps keyword getters, extension-field removal,
record-input conversion with into, not-empty extension-map normalization and nil
metadata/hash arguments. The generated cond->> is expanded explicitly: dissoc
executes once before record? is tested on the original input, then only record
input converts the resulting extension map. Generated factory names, name
metadata and docs remain intact. These are native compiled helper functions,
not a Rust evaluator that constructs a restricted factory template.

Two authored two-phase native tests in `compiled_record_factories` consume the
actual helpers through compiled wrapper macros and intrinsic type declarations.
They check constructor arities/hidden nil fields, plain-type positional factories,
factory attributes/docs, nil/false/missing/extra map entries and zero-field cases.
They do not replace the missing full public record expansion with a hand-written
IRecord/equality fixture. The genuine record-input conversion branch still needs
execution after the full emitter/protocol graph lands.

The existing macro provenance checker now covers these adaptation identities,
bootstrap installation and cache dependency wiring; the import gate invokes it.
Four authored Python regression methods reject stale hashes, altered identity
sets/order, removed bootstrap installation and removed artifact dependencies.
Neither the new checker nor tests were executed under the user's hold. Rustfmt,
static source scanning and Python AST parsing establish syntax only. Independent
static review of the helpers/loader/native test source and added provenance
checks found no actionable issues. That review did not invoke any checks or tests.

Next: wire the complete compiled defrecord/emit-defrecord and compiler helper
adapters to these functions, preserving every generated protocol method and
custom implementation. Bootstrap regeneration, actual helper initialization,
fresh pinned observations, native execution, existing macro/cache/reset checks,
full workspace gates and final-head CI remain pending. Public defrecord and
original #18/#19 acceptance remain unfinished.

## Emitter dependencies and hash labels — authored 2026-10-12

The runtime import now retains complete complement/remove,
preserving-reduced/cat/mapcat and drop-while forms, including every fixed and
variadic clause, lazy/transducer modes, and nested Reduced propagation. The
existing complete list* import is reused. Six explicit record-emitter patches
retain their pinned hashes and EPL originals; source generation wrote 439 files.
The two authored two-phase native regressions in
`portable_record_emitter_dependencies` cover multi-input mapcat, every relevant
mode, complement arities, list* spread, nested/outer stop effects and drop-while
predicate state. No execution result is claimed.

The compiled `suss.internal.record-label/type-label` helper implements the full
scalar string/symbol naming path needed for the record hash seed: non-overlapping
double-dot replacement, division before a non-line-terminator, per-segment reserved
suffixes and UTF-16 character mapping. It retains the entire pinned character and
reserved tables, including multi-code-unit keys that charAt cannot match. This
private type-label interface does not claim analyzer-map lexical munging.
Its [adaptation record](label-adaptations.json) binds the retained inputs, native
source file and compiled source identities. The macro bootstrap source installs
the helper and includes its source identity in every artifact dependency list.

`compiled_record_labels` is authored for ordinary/qualified/punctuation/reserved
names, empty segments, trailing division, line terminators, astral and lone
surrogate inputs. Three authored provenance regression methods reject stale or
missing identities and absent loader/cache wiring. The existing macro provenance
gate covers these inputs; none of these checks or regressions has been executed.
Static Rust/source/Python parsing and whitespace checks are not compatibility
evidence. Independent static review remains pending for these new changes.

The public record emitter and its complete generated methods are still missing.
Do not treat dependency imports or a hash-label helper as record acceptance.
The prior test hold remains in force pending clarification after Cargo cleanup
and goal reactivation. Bootstrap source/dependency/compiler identities remain
stale; validation must begin with regeneration and focused checks once permitted.

## Executed helper evidence — 2026-10-12

The dated source-only notes above describe their authored snapshots. The test
hold was lifted before the recorded focused and full-baseline attempts. Factory,
label and intrinsic record-boundary execution results, regenerated bootstrap
identities and remaining limitations are recorded in the latest
[handoff evidence](../../roadmap/handoff.md). Those helper results establish
those interfaces; the public record emitter, wrapper and complete generated
protocol graph remain separate implementation and acceptance work.

The genuine opt-in compiler catalog now includes `:native-interfaces`, keyed by
`js/Object` only when the actual caller-phase binding is the exact Core
`object-constructor` export. Its value records `:global`, `:phase`,
`:binding-kind :environment-core`, `:export`, and
`:extension-kind :native-object`. No namespace, definition metadata or protocol
signature is synthesized. Producer tests execute positive and replacement-cell
omission cases in both phases; the compiled-wrapper test consumes the actual
serialized catalog in both caller phases after GC with default budgets.
See [producer tests](../../../crates/suss-compile/src/portable/compiler_facts.rs)
and [transport execution](../../../crates/suss-cli/tests/compiled_macro_compiler_catalog.rs).
This transport alone is not public `defrecord` acceptance.


### Explicit record binding transport

The compiled public record helper requests `:suss/compiler-catalog :record-bindings`.
This projection carries the genuine caller snapshot and live namespace resolution
views, each with name, aliases, refers, exclusions, and `:suss/binding-names` from
actual phase-specific nonhidden identities. It carries actual protocol signatures,
namespace/global indexes, resolved unqualified bindings, and the typed native Object
fact. It does not create placeholder definition records or serialized ASTs.

Boolean `true` retains the full compiler catalog and ordinary analysis graph;
false/nil retain ordinary macro transport. The selected mode is checkpointed with
the source macro binding and included through its declaration source identity.
The record resolver accepts membership sets and the existing full definition maps,
while preserving caller phase, own shadows, refers, exclusions, qualified aliases,
and provisional protocol declarations. Numeric graph and fuel limits are unchanged.

Focused Full/RecordBindings execution passes in both caller phases, including own
shadowing, exclusions, renamed refers, qualified lookup and failed-staging mode
restoration. The complete two-record CLI component compiles under unchanged
graph/fuel limits and passes all nine exported checks. Full M4 acceptance remains open.


## Executed public record integration — 2026-10-13

The dated authoring notes above are historical snapshots. Public `defrecord` is
installed through compiled Suss helpers, retaining all 15 generated protocols,
16 methods and 31 flattened entries. User implementations precede generated
methods. Nominal equality uses immutable type descriptor identity. Both factories
are integrated; grouped keyword case lowering supports generated membership.

`compiled_public_records` executes two ordinary three-field records through the
CLI component path. All nine exported checks pass: lookup; adapted Jank dissoc
and portable empty; nominal equality/equal hashes; metadata/persistent updates;
missing/nil/false/extension factory values; Reduced stopping; warmed hash
invalidation; and constructor effect order. A separate independent ABI decoder
regression passes in both phases after GC, observing fields, metadata, extension
values and persistent updates directly. Unordered decoded maps are compared
without iteration-order assumptions. See [provenance](public-contracts-provenance.json).

The latest successful full baseline predates this integration. Provenance/import,
bootstrap reproduction, integration baseline, independent final review and final-head
CI remain gates. These bounded results do not establish complete record
customizations, whole Jank namespaces, all collection/reduction paths, or M4 completion.


Public custom protocol follow-up: the [fresh pinned observation](public-custom-observations.json)
and matching independent native decoding establish unique user methods, generated
method precedence for duplicate ICounted, and genuine record-input map factory
conversion. The extra user protocol initially exhausted the unchanged macro budget.
Mask preparation now resolves every symbol in source order, reuses the complete
default data only after a matching resolved suffix, and processes every user
protocol generically. Changed default bindings retain the full generic path.
The [public adaptation ledger](public-adaptations.json) binds all three integrated
helper files, compiled sources and retained originals; mutation tests reject stale
or missing identities. Broader record customizations and M4 acceptance remain open.
