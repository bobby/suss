The complete Clojure1.12.6 assert macro retains all original branches, arities,
documentation, metadata and message expressions. The exact patch replaces two
JVM AssertionError constructor expressions with the existing catchable portable
Error adapter used by compiled core macros. JVM AssertionError class identity is
not provided by that Error family and is not claimed as a match.

Fresh primary data covers19 actual control-flow/message/effect observations and
compile-time flag behavior. The initial metadata-field assumption and its driver
failure remain archived separately; the actual Var.isDynamic query supplies the
corrected flag data. No initial result is overwritten with success.

The immutable-library before probe compiles the unchanged original body targets
against the frozen some-fn producer, without mutating its Cargo target. Both
namespaces fail to load: some-fn at assert and constantly at rand-int. Zero
assertions execute. Build0/body101 is a native counterexample, not a Cargo pass.

Native regressions require Runtime/Macro/actual AOT data, lexical shadowing,
fresh error identity, effects/messages, compiler-phase disabling before operand
analysis and real wrong-arity errors. The original16+31 bodies remain unchanged.
Rand-int is a separate genuine dependency; a constant or skipped randomized
assertion does not satisfy it. All native results require their own source-frozen
receipts. Source/patch/license checks do not establish native compatibility.
