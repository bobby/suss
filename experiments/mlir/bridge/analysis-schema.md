# Bounded source analysis transport (draft)

Status: uncompiled, unexecuted, unattached module. Only `src/analysis.rs` and this
file belong to this lane. No Cargo/network command, lock/dependency change,
C++ change, execution evidence or full-analyzer claim. This does not certify M4.
Original transport code; no upstream analyzer implementation copied. Semantic
reference is portable ClojureScript at c4295f303100bbf5afac449242d30bca1126f1a1.

## Entry points and integration gap

`analyze(source)` calls actual `portable::analyze`, then projects genuine HIR
source facts. A compiler-only fragment Do wrapper is accepted only when it has
exactly one genuine child, which is selected without fabricating source; ambiguous
wrappers are rejected. `selected_facts(&Hir)` accepts already analyzed HIR. `analyze_with`
takes source, Environment, Phase and an actual ExpansionHost. The isolated
checkout has no public `portable::analyze_with` or analysis-only preparation API.
Its public preparation API normally emits Wasm. The draft uses an outer `let*`
with a transport-only local and final expansion sentinel. It captures the actual
analyzed initializer through ExpansionContext.locals, then deliberately returns a
Diagnostic before IR lowering. A defensive emit_fragment override forbids emission.
The original selected expression Form is moved into the wrapper, preserving all
its spans and metadata; it is never printed/reread. Probe spellings are reserved.

This bounded probe accepts exactly one selected expression and supplies initializer
Expression context, unlike top-level Statement context. It is not a substitute
for the eventual public analysis-only API; namespace directives, macro imports,
loading policies and artifact dependency forwarding are outside its contract.
The supplied expander must be configured for the selected expression itself.
Host expansion/runtime work is not bounded by the transport budget. A future
production implementation must use a public analysis-only API with explicit
origin and namespace preparation rather than retain this probe.

The bridge now uses relative dependencies on this isolated checkout's suss-compile
and suss-reader crates. The source-analysis module still needs to be wired into
the driver and compiled; no dependency resolution or native pass is claimed.

## Envelope, identities and admitted source operations

Version is exactly `suss.source-analysis.draft.v1`. Envelope has exactly `schema`
and `selectedFacts`; facts are a recursively nested node tree. Attachments are
source observations, never replacements for the graph in schema.json. Do not
attach this payload to a C++ operation until explicit correspondence with its
source node is available. Proposed admitted source op tags are `let`, `closure`,
`vector`, `invoke`, `do`, `local`, `global`, `literal`. They are derived from actual
SourceNode/SourceCallable/SourceBinding or scalar source form, never relabeled
lowered vector constructors. If C++ lacks an admitted operation, reject the
attachment rather than attaching it to an unrelated constructor op.

`bindingId` is `binding:N`, allocated by first deterministic encounter within ONE
envelope, from LocalBinding.identity Arc identity. Allocation sorts visible
local map names; arrays retain source order. Pointer values never leave Rust.
These IDs preserve declaration identity through snapshot clones/shadow chains;
they are reproducible for identical traversal, not persistent cross-fragment IDs.
`hirBindingId` is the genuine numeric HIR BindingId; it is not an SSA ValueId.
SSA IDs live solely in the existing graph envelope. No conversion/equality
between these domains is permitted. Capture arrays retain actual HIR capture
order and include both identity domains, requiring a source local mapping.
There are no node-reference IDs: children and initializer nodes are embedded.
Repeated declarations can appear in snapshots; compare complete embedded facts.

Every node has: op, originalForm, span, metadata, physicalType, isBody, context,
phase, namespace, scopeNamespace, snapshotNamespace, tag, inferred,
quotedConstTag, inferredReturn, resolved, locals, declarations, children,
callable, captures. Only namespace names are selected from SourceNamespace;
its catalogs are not projected. A resolved global projects selected declaration
revision syntax/metadata explicitly, as described below.
This is an explicit selected-facts schema, not complete SourceAnalysis coverage.

`span` is a two-element half-open UTF-8 byte range, including genuine 0..0
synthetic spans; never replace an absent source record with a zero-span record.
Compiler-only HIR lacking source is rejected. context is statement/expression/
return; phase is runtime/macro. physicalType tags are nil, bool, number, string,
value, closure (with arity). Physical storage types remain independent of source
inference: actual tags are optional lossless Forms from SourceTags, not inferred
from these physicalType tags or from MLIR signature types.

## Source forms and optionality

originalForm is the actual SourceAnalysis.form (selected/expanded syntax, not
necessarily pre-expansion text). A Form always has span, metadata (ordered Forms)
and data. Tagged data: nil; bool/value; f64/bits (16 lowercase hex digits);
utf16/units (u16 array, preserving lone surrogates); symbol or keyword with
namespace (null or string) and name; list/vector/set/map/conditional with ordered
items; discard/target; prefix/operator/target. Map/set syntax order and duplicate
entries are preserved. Float bits preserve negative zero, infinity and NaN payload.
No decimal float transport and no printed EDN. Reader-prefix variants are lossless
syntax serialization capabilities, not promises those variants reach analysis.

A tag or inferred field is `{present:false}` or `{present:true,value:Form}`.
quotedConstTag and inferredReturn additionally allow `{present:true,value:null}`.
Absent, present nil, present false remain different. Method recurs is absent or
present boolean, so false is not absence. Optional resolved/callable/initializer/
shadow use null for absence; a language nil is always a tagged Form/node.

resolved is null or `{tag:local,binding:...}`. locals is a name-sorted array of
{name,binding}. A binding contains bindingId, hirBindingId, physicalType,
declaration Form, kind (let or fixed argument with index/rest:false), context,
initializer node or null, shadow binding or null. Plain roles only. Shadows
retain previous source declaration identity; names are not identities.

children is an ordered array of {role,node}, with bindingId additionally on init:
let: each init in binding order, then body; vector: each item; invoke: callee then
arguments; do: statements then result. declarations retains let LocalBindings in
source order. Callable has actual source method form, declarations, parameter
HIR IDs/names/spans/metadata, variadic:false, recurs, entryLocals, entryContext,
and body. Only an anonymous single fixed method is admitted. callable body is
separate from executable child operands. Named/self/variadic functions, field or
adapted roles, loops, if, recur, exceptions, assignment, metadata
wrappers, maps/sets and every unlisted source node/leaf reject explicitly.

## Genuine global SourceBinding facts

`resolved` is null, `{tag:"local", binding:...}`, or the exact global record
`{tag:"global", global:{phase,namespace,name}, declaration:...}`. Global identity
comes from the public `resolve::Global` phase/namespace/name tuple; it never uses
local `binding:N`, HIR BindingId or SSA IDs. The declaration is an explicit
presence wrapper. When present its value has exactly: `definitionForm`,
`analysisCompleted`, `declaration`, `docstring`, `origin`, `initializerForm`,
`initializerPresent`, `typeFields`, `once`.

Definition/declaration/initializer syntax uses the existing lossless Form schema.
Docstring uses absent/present UTF-16 units; typeFields uses absent/present unsigned
count (zero is present). Origin uses absent/present `{sourceBytes,path}`;
sourceBytes is actual public SourceOrigin text UTF-8 length, and path is an
absent/present UTF-16-unit array. Non-Unicode filesystem paths reject explicitly.
The private origin caches and entire origin text are not selected. Initializer
HIR is not recursively traversed: `initializerPresent` records its actual
presence, independently of optional `initializerForm`. This deliberate projection
avoids unbounded global declaration initializer graphs. It is not complete
DefinitionInfo transport. Definition form and all metadata remain retained.

Actual HIR resolution is captured from an original symbol or a list's symbol
head. Thus let/fn source heads may carry global facts, and an invocation may carry
its local callee's facts, while their source op tags remain unchanged. The original
vector kind does not resolve to PersistentVector; the real vector SourceNode and
ordered item children remain the evidence. `global` is admitted as a leaf op only
for an original symbol with genuine global SourceBinding. Fields and unsupported
source-node variants remain rejected. No fact is synthesized from constructor
lowering, function names or physical ABI storage.

The coordinator's third bridge native run compiled and executed 13 tests:
12 passed, one representative failure with `global/field resolution unsupported`.
Log `/private/tmp/suss-m4-mlir-bridge-third-tests.log`. This supersedes earlier
blanket uncompiled statements for that pre-repair revision. The new global repair
and two added tests remain uncompiled/unexecuted: both-phase real source def
metadata/docstring/initializer/origin/redefinition facts, and a builtin's absent
declaration. The original representative vector fixture is unchanged, with new
assertions for unresolved vector identity and ordered x/y/false/nil source items.
Rust 2024 parsing via rustfmt `--emit stdout` passes; no Cargo was run by this
repair. Erdos owns the corresponding strict C++ global/list-head schema update.

## Comparison, bounds and pending tests

`compare_transport` parses JSON and compares the ENTIRE value to independent
native selected facts. Array order, spans, metadata, tags, optionality, bindings,
captures, children, extra/missing fields and scalar bits participate. Object key
order does not. This is a comparison helper, not a standalone schema validator;
The authored Rust parser uses a recursive unique-key serde visitor before
comparison, rejecting duplicate decoded keys even inside arrays and for escaped
equivalents. Three parser-only unit tests cover duplicate keys, exact values,
trailing input, size and recursion guards. This Rust change remains uncompiled
and its tests unexecuted; it is not external acceptance evidence.
C++ must independently reject unknown keys, wrong types, dangling/conflicting
binding references and unsupported attachment mappings; a version string alone
is not verified provenance.

Bounds: source/received JSON 1 MiB, depth 64, aggregate 32768 form/node/binding
visits per encoder. Repeated snapshots consume the budget; oversize facts fail
rather than disappear. Form strings/arrays are constrained by source size for
reader entry points; selected_facts on arbitrary HIR is not a hardened byte-limit
service. JSON parser default recursion limit also applies.

Unexecuted Rust tests include a metadata/closure/shadow/vector/invoke fixture,
complete JSON equality plus altered type/extra field rejection, lossless scalar
bits/UTF-16/presence and unsupported control rejection. SourceControl representative
cases reference the existing pinned control-source-ast-observations.json and
runner: let-local, let-sequential, fixed-function. They pin op correspondence
(:let/:fn) and source child counts, and perform full transport equality. This is
partial upstream observation coverage, not equality with the full SourceControl
corpus. Existing native provenance: portable_control_source_analysis.rs (genuine
binding declarations/body scopes), portable_collection_source_analysis.rs
(collection children), plus the pinned control-source-ast runner/oracle scripts.
Pre-repair native results and the new pending repair are distinguished below.
No native success is claimed for this repair.

## Concrete coordinator sequence

1. After the baseline ends, align the bridge dependency path with the actual
   source-aware HIR, add direct reader dependency, and establish the isolated lock offline under coordinator ownership. The reader dependency and `mod analysis` driver integration are authored; `--analyze-source` is uncompiled.
2. Compile the focused module tests and resolve real API/fixture failures. Add
   expansion-host trace assertions, exact initializer/body/resolved/shadow identity
   assertions, capture checks, and full pinned selected observation projections.
   Replace the diagnostic probe when an analysis-only expander API is exposed.
3. Compile and execute the authored strict duplicate-key units and malformed/
   limit tests; C++ schema-shape validation is executed, but native transport
   correspondence remains pending. Keep
   native expected facts independent of incoming JSON. Persist JSON fixtures only
   after native test execution; do not call this hand draft generated evidence.
4. Implement C++ attachment mappings from real analyzed facts, keeping binding
   and SSA IDs distinct. Roundtrip complete facts; mutate metadata, source child
   order, shadows, inferred tag, false/nil/absence, span and binding references;
   require mismatch/rejection. Do not interpret attachment payload as runtime IR.
5. Record commands, terminal results, limitations and next task in the handoff
   under coordinator ownership. Baseline/full milestone acceptance remains separate.
