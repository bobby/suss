# Public num source inline and ordinary alias boundaries

The pinned Clojure1.12.6 primary retains all14 initial API controls, including
12 values/effect observations and2 direct-call compilation-stage failures. A
separate six-control primary records number?/num rebinding and lexical shadowing;
it does not replace or alter the original14. JAR, source, stdout and actual
terminal receipts are retained.

Two independent frozen native CLI probes expose the current source discrepancy.
The four literal controls return [true true true true], against the actual
[false false true true]. The six binding controls retain five matches, while
a direct num call under num rebinding returns :rebound instead of1. Both CLI
commands themselves return0 and their comparisons return101; source/HEAD/binary
hashes remain unchanged. These CLI probes are not complete native/AOT results.

Run clojure_num_api against the unchanged implementation before the inline
correction. It preserves all20 controls in each native phase, independently
decodes values after GC, and checks18 actual AOT values plus2 compilation-stage
failures. Owned language catches replace only the unavailable Java catch classes;
alias arity checks retain the actual diagnostic and evaluation effects.

The first native/AOT test attempt fails setup because public subs is not loaded.
Its exact test and actual101 are retained; it is not the semantic counterexample.
Using the owned UTF-16 slice only for the diagnostic prefix fixes setup. The next
frozen command returns101 in75.66s including compilation: each native phase
records five discrepancies (two direct arity stages, two NaN inline identities,
and direct-call num rebinding). The actual AOT reports the same value differences
and accepts both bad-arity units, which should fail compilation. Decoder guards
pass. All20 primary controls are retained; source and HEAD remain unchanged.


### Public num inline and alias boundaries (2026-11-24)

The complete14-control Clojure1.12.6 API primary and separate6 binding controls
retain two direct-call compilation failures and18 value/effect observations.
The native/AOT negative command returns101 in75.659s, with source/HEAD unchanged: 
five discrepancies per native phase, plus actual AOT value and arity-stage
failures. An earlier fixture setup failure on unloaded subs is retained separately.

Authenticated direct source calls now implement the original num :inline rule:
bad arities fail analysis; primitive floating inputs box freshly; existing Object
inputs retain their roots. Ordinary aliases keep the original function's runtime
arity, evaluation effects and live bindings. Compiler-owned NumCheck admits nil
and actual Numbers or throws an owned error. The first emitter attempt passes
Rust checking but fails generated Wasm validation because number-test inherited
the generic import signature. Its exact source overlay and terminal101 are saved.
Declaring the existing predicate as (eqref)->i32 fixes the import; no runtime
export, Java dependency, original core form/notice or numeric kernel changes.

The frozen complete-native-aot-after-import-signature-fix gate returns0 for all
three commands: source check24.753s; all4 complete API/decoder tests271.810s;
strict original numeric bodies157.948s. Runtime/Macro retain all20 controls and
actual AOT checks18 values plus2 compile-stage failures. Original eq/not-eq keeps
142 host matches; complete default-num retains26 rows with23 host matches and
the same3 explicit narrow-number platform differences. Source/HEAD unchanged.
Broader compiler/hint/WIT/cache/budget guards, complete capture, exact workspace,
independent review and final-head CI remain separate requirements.


### Source floating Objects retain boxed replacement dispatch (2026-11-24)

The broader source-hint command returns101 at clean5b0a9726: original ADR-0004
raw-core Runtime/Macro and actual AOT controls trap when raw-double is replaced
by a boxed Long function. These controls are explicitly Suss's primitive-entry
fallback contract; no primary or test expectation changes. Three earlier source
hint/overload/method targets pass; later command stages do not execute.

The physical floating Object materializer now tests the actual value before
copying Double fields. Definite primitive Doubles retain the raw path; actual
boxed Doubles get a fresh Object; other boxed replacements retain their actual
value for ordinary numeric dispatch or language errors. The previous blind
Double cast caused the trap. The frozen focused gate returns0 for all three
commands: original raw-core, complete Num/Object native and AOT targets, and
strict complete numeric bodies. All20 Num API and16 Object-context controls,
142 equality host matches and26 default-num rows retain their earlier results.
Source/HEAD unchanged; individual seconds and receipts are retained. Wider
compiler/resource/WIT/cache/budget/bootstrap and final acceptance remain separate.


### Broader compiler validation after boxed fallback correction (2026-11-24)

At45733e74 all9 durable command stages return0, source and HEAD unchanged:
source hints/primitive methods/overloads/raw-core/parameter/type guards; raw
locals/captures, actual WIT, HIR resource and expression-artifact guards; source
artifact/identity, default budgets and built-in authority guards; compiler
library guards; Java/Node-free double bootstrap; and core-import, sequence,
numeric and unchanged upstream contract checks. Full per-command logs/times
are retained under results/m12-num-api/broader-compiler-after-boxed-replacement-fix.
This checkpoint does not replace the full captured numeric acceptance, exact
workspace, prerequisite eval review fixes, independent review or final-head CI.
