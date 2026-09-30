# Direct protocol implementation predicate

The portable bootstrap now lowers `implements?` with the pinned distinction from
`satisfies?`: it tests the resolved protocol's direct nominal marker and never
consults native-kind or default fallback tables. A Number/nil/Boolean/String with
an `extend-type` native implementation can satisfy a protocol while implements?
remains false. An existing nominal object observes a later direct extension.
Empty protocols and stable markers after protocol redeclaration retain that
distinction. This is a prerequisite for source seq/first/rest/next dispatch.

The protocol operand is a syntactically resolved name. The value operand lowers
once through explicit HIR/IR before the marker test; exceptions stop the test and
finally cleanup executes normally. Existing descriptor-backed marker identity is
reused. No runtime helper/global, recursive layout, ABI version, bootstrap runtime
cell or shipped dependency is added. Macro/runtime namespace identities and
protocol keys remain phase-isolated; this does not introduce compiled macros.

Lexical bindings and own runtime definitions hide the automatic macro. An
explicit user runtime refer retains the pin's automatic unqualified implements?
macro; use the provider's qualified alias to invoke its runtime function. Qualified cljs.core/suss.core calls and aliases remain independent
of that user var; exclusions suppress unqualified lookup. The macro is not a
first-class runtime function. Wrong arities, expression/missing protocol names
and unsupported first-class macro references produce located diagnostics.

Source provenance: ClojureScript c4295f303100bbf5afac449242d30bca1126f1a1,
core.cljc2227–2251, macro implements?. Original Rust bootstrap code copies no
upstream form. The pinned source retains copyright/EPL notices. One hash-bound
partial review records dependencies on protocol-prefix1326,
fast-path-protocols813 and bool-expr893. Protocol masks are represented by existing
portable direct markers; arbitrary host prototype/property interoperability,
metadata protocol implementations, full builtin/source core and compiled upstream
macro expansion remain unfinished. The declaration stays in progress.

Twenty-four initial independently encoded primary observations passed while the
native regression failed unresolved implements? before implementation. Three
additional probes certify throw/finally order and own-var/qualified shadowing;
all 33 fresh pinned observations match actual scalar-decoded Wasm execution.
The original 27 remain unchanged; two additional probes distinguish syntactic
protocol names from local shadowing and runtime protocol aliases. Four independent
review probes cover direct extension during operand evaluation, protocol var
rebinding, finally-returned objects and caught operand throws. A separate
pinned provider/referrer fixture verifies automatic macro lookup versus runtime
provider aliases before printing the observations.
Three native tests additionally cover aliased/excluded/referred bindings, retained
objects/functions after GC, malformed calls and recovery after throw. Compiler
phase/diagnostic tests supplement existing nominal HIR/IR guards.

Commands:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-implements-oracle.sh`
and focused `cargo test -p suss-cli --test portable_implements --test portable_native_protocols --test portable_comparisons -p suss-compile --test portable_nominal --test runtime_abi --locked -- --test-threads=2`.
Full baseline, independent review and final-head CI must pass before PR readiness.
This foundation does not implement persistent sequences/lists, variadic rest/apply
or complete M2/M4/M7 acceptance. Next port the source sequence/list dependencies,
preserving the separately certified48-case sequence corpus and its genuine native
unresolved-seq regression. Partial progress uses Refs #11/#17, not closure claims.
