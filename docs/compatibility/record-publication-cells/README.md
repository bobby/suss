# Anonymous publication cell-state kernel

Private `(suss.compiler/cell-defined? namespace/name)` tests a genuine source
cell in the current caller phase. It creates an unbound cell identity only for a
fresh name in the actual current namespace, without fabricating source declaration
facts. Existing qualified cells resolve through the ordinary catalog. Compiler
bootstrap bindings, missing foreign namespaces, unqualified symbols and evaluated
operands are rejected. This is a compiler adapter, not the full public exists?
macro or a replacement for owned dotted property paths.

The typed function reuses guarded dynamic-frame lookup before checking the root
bound flag. Only the selected value is compared to exact undefined; nil, false,
zero and objects are present. Unbound roots are not read. Dynamic overrides of an
unbound root count, and the source cell identity remains phase-specific across
fragments/GC. No property coercion, replay, shared recursive type or new global
root is introduced. Adding the typed private function changes runtime code
identity; bootstrap has not been regenerated.

The full retained exists? source remains intact and unimported. Its symbol
resolution, ordered prefix/path checks and once-only non-symbol branch still
need complete source adaptation before reify integration. Ordinary deftype
capture restrictions and existing publication transaction boundaries are intact.
This kernel does not establish reify, nil-iter or record completion.

Fresh pinned source compile and Node execution exited0. Exact ordered observations
are false,true,true,true,false,true,false,true,0 (last is a raw effect counter).
The initial misplaced-fixture failure is retained. Native two-phase post-GC
regressions for later class publication, independent retained probes, dynamic
overrides, unbound roots and rejection/recovery are authored UNCOMPILED/UNEXECUTED.
Rustfmt parsed changed Rust; no Cargo, native execution or bootstrap was run.
Full original seven reify/fourteen iterator and all prior helper cases remain.
