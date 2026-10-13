# Portable `seqable?`

The complete pinned `seqable?` form (core.cljs:2347–2355) is retained by the
core importer with its EPL notice. The adaptation preserves the documented
predicate, nil, ISeqable, array, and string branches. It omits only the
`js-iterable?`/`Symbol.iterator` branch because the accepted Suss 0.3.1 design
excludes JavaScript/npm/Closure interop from portable source compatibility.
Foreign JavaScript iterables must be wrapped by a portable `ISeqable` or
`IIterable` value. This is an explicit boundary, not a claim that all pinned
`seqable?` behavior is implemented.

The 12 pinned raw observations cover nil, persistent vectors/lists/sorted maps/
sorted sets, arrays, strings, a custom ISeqable type, and non-seqable number,
false, keyword, and plain-object values. The plain object case returns false in
the pinned compiler and the portable runtime. `observations.json` is the direct
ClojureScript/Node output; the Suss regression independently decodes Boolean
ABI values after GC in both Runtime and Macro phases.

`python3 -B scripts/core_import.py --check` validates the retained source,
patch, source hashes, loader declarations, license, and generated artifacts.
Native validation is tracked separately and is not implied by the oracle pass.
