# Actual analyzed source records

`Hir::source` retains an immutable `SourceAnalysis` for syntax actually analyzed
through the source pipeline: the reader or expansion form, three-way analysis
context, phase, namespace at entry and optional supplied source origin. This is
original compiler code; it imports no upstream implementation and adds no shipped
Java or JavaScript dependency.

Compiler-only lowering nodes explicitly have no source record. A function's
physical recurrence loop, receiver access, generated protocol helpers and private
runtime operations must not be presented as user syntax. A macro expansion keeps
the expanded form's record rather than overwriting it with the original call.
Expansion spans remain call-site provenance; they do not certify that the generated
syntax occurs in the original source text. Owned forms without supplied source
retain unknown origin. No source is evaluated to construct these records.

Executing binding regressions inspect actual `do` and arithmetic initializer
syntax and child records while independently checking that initializer effects
run once in Runtime and Macro Stores. Another regression executes an expanded
initializer, collects GC and decodes its number, then checks that the source record
contains the actual numeric expansion rather than the original macro call.

Validation: 9 passed, zero failures/ignores across binding, context and function
scope regressions; 38 passed, zero failures/ignores across compiler pipeline,
closures, nominal and exception tests. Full validation of this new branch remains
pending. These records are preparation for rich compiled `&env`; they do not yet
provide a complete source AST schema, inference, canonical environment transport
or source macro `&env`. All original M3 acceptance remains open.
