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
frozen command returns101 in76.94s including compilation: each native phase
records five discrepancies (two direct arity stages, two NaN inline identities,
and direct-call num rebinding). The actual AOT reports the same value differences
and accepts both bad-arity units, which should fail compilation. Decoder guards
pass. All20 primary controls are retained; source and HEAD remain unchanged.
