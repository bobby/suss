# Compiler method binding roles

This is partial issue #14 preparation for rich compiled macro `&env`. The native
expansion host still inspects compiler facts; source macros do not yet receive the
rich environment. Physical argument slots and lowered binding IDs are retained.
A separate `SourceRole` records method adaptations without replacing those IDs.

Pinned Object methods exclude their implicit receiver when numbering user
arguments. Protocol methods include their explicit receiver at argument zero.
Both adapters expose the receiver through `this-as`, a source local binding.
Protocol receiver bindings shadow their original argument; Object receivers have
no original argument. Nested parameters can shadow that receiver and method exit
restores the enclosing bindings. Parameter/field shadows retain actual field
metadata and access records.

The role record retains the original target type declaration and actual lowered
receiver access. Inspecting access does not read the receiver or fields. It does
not invent a separate lowered ID, evaluate an initializer or claim that a lowered
access expression is the pinned JavaScript initializer AST. Canonical rich
transport and remaining source AST/inference facts still need implementation.

The fresh development-only pinned ClojureScript oracle captured six ordered role
observations and four executed count results. The strict comparator checks exact
roles, argument positions, field mutability and immediate shadows. It rejects
malformed/missing/unknown facts and is not a general runtime value decoder. The
executing native regression compares actual compiler records, executes the
resulting artifacts in both Stores, inspects real IDs and independently decodes
binary64 results after GC. Before the fix, all six observed receiver/argument
role sets differed from the pinned corpus. After recording method roles, 23
focused tests passed with no failures or ignores, including exception, object,
field, function and method-scope runtime regressions.

```sh
CARGO_BUILD_JOBS=2 scripts/test-method-role-oracle.sh
cargo test -p suss-cli --locked --test compiled_macro_method_roles \
  --test compiled_macro_field_records --test compiled_macro_function_scopes \
  --test portable_type_method_scopes -- --test-threads=2
```

The compiler and fixture additions are original. No upstream core source import,
license count or shipped Java/JavaScript dependency changes. The pinned adapters
in core.cljc1498–1521 and compiler binding shapes inform these facts; no source
algorithm is copied. Independent review, full baseline and exact-head CI remain
required. This work does not close issue #14 or complete M3.
