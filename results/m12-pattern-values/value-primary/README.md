# Pattern value prerequisites for complete numeric source bodies

The unchanged eq and not-eq bodies contain two pattern literals; num tests reject a pattern as nonnumeric. Their namespaces currently cannot load because regex reader dispatch is unsupported. This evidence does not replace, slice, skip, or certify those bodies.

A fresh pinned Clojure 1.12.6 run returns actual0 for all 26 observations. Sources, commands, JAR hashes and raw outputs are retained here. `observations.json` is derived only by pairing those actual output lines with their input expressions.

Separate pattern literals are unequal, while a binding alias is equal and identical. Crucially, a function or loop literal site retains the same Pattern instance across evaluations: allocating a new marker for each call would be wrong. Separately reading/printing the same text constructs a fresh instance. A Pattern directly supplied to eval evaluates to itself, while quoted/generated-container constant compilation constructs a fresh pattern. Pattern metadata is nil, hashing uses object identity, and numeric predicates are false. Invalid patterns fail during reading, even under discard. Backslashes belong to regex syntax and must survive literal reading/printing.

These observations define required behavior for an actual Pattern literal/value implementation. They do not provide a regex engine, prove Suss regex support, or waive the numeric issues' complete original suite requirements. General matching APIs remain a separate compatibility scope.
