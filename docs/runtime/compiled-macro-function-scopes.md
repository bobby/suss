# Compiler function scope facts

This is partial issue #14 work toward genuine compiled macro `&env`. Source macros
still do not receive the rich environment. The expansion host now borrows actual
named function scopes, preserving declarations, source origins, phase namespace,
parent scopes and lexical/field shadows.

An explicit self name retains its real compiler local binding. A definition name
hint names the scope without adding a local or inventing a binding ID. Explicit
names override hints. Anonymous nested functions inherit the enclosing named
scope; sibling or subsequent forms do not inherit a completed function scope.
Name hints follow macro expansion, but are not propagated into arbitrary operands
or bodies. Parent records share immutable references without cycles.

The development-only oracle force-compiles pinned ClojureScript with analysis
caching disabled. Eleven actual ordered compiler observations cover definition
hints, named/anonymous/nested functions, inherited scopes, shadowed locals,
multiple arities, rest parameters and restoration. Ten executed name vectors
were captured from Node. The strict comparator checks exact names and lexical
name sets; it is not a general runtime value decoder. The native fixture is
prepared to compare those observations and decode actual canonical vectors after
GC in both Stores, while inspecting binding IDs and structural parent/shadow
records. Native execution has not yet been validated.

```sh
CARGO_BUILD_JOBS=2 scripts/test-function-scope-oracle.sh
cargo test -p suss-cli --locked --test compiled_macro_function_scopes \
  --test compiled_macro_binding_records --test compiled_macro_analysis_context \
  --test compiled_macro_field_records -- --test-threads=2
```

Compiler and oracle additions are original code; no upstream core import or
license count changes. No Java or JavaScript dependency is added to shipped code.
Native validation, complete baseline, independent PR review and exact-head CI
remain required. Canonical rich environment transport, logical method receiver
roles, syntax quote/bootstrap/cache/evaluator removal and original M3 acceptance
remain open.
