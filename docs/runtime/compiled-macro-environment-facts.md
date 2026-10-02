# Compiler facts for compiled macro environments

This is partial progress toward issue #14 and design section 7. Compiled source
macros still do not receive rich `&env`; the Rust expansion-host regressions below
inspect actual compiler facts and execute their resulting artifacts in both Stores.
They do not establish source-level `&env` compatibility or bootstrap acceptance.

Lexical analysis retains declarations with spans and reader metadata, actual
analyzed initializers, binding roles and shared lexical shadow chains. Lowering
remaps parameter IDs without manufacturing new source declarations. Initializers
are never executed again to obtain environment facts.

Immutable source origins retain original UTF-8 text and canonical module paths.
Inline input has text but no invented filename; forms-only APIs have no origin.
Positions use one-based UTF-16 columns and recognize LF, CRLF and standalone CR.
Invalid offsets remain unknown. A symbol position excludes metadata prefixes only
after validating the actual source syntax and symbol spelling. Generated forms
without matching source syntax do not acquire fabricated declaration locations.

Namespace facts retain phase-specific aliases, refers, exclusions and declarations:
metadata, docstrings, `defonce` role and analyzed initializer. Compiler declarations
are distinct from initialized binding values. Failed compilation does not publish
partial declarations. A failed runtime initializer preserves the old live binding;
its new compiler declaration still describes the actual attempted initializer AST.

Seven focused native tests cover lexical facts, shadow restoration, lowered IDs,
module versus inline origins, phase namespace staging and failure recovery, and
source positions. Fresh pinned ClojureScript observations verify four macro call
positions (including non-BMP text and mixed newlines) and four local token positions
(plain, tagged, chained metadata and map metadata). Each resulting position vector
is independently decoded after forced GC in both Runtime and Macro Stores.

Run the development-only JVM/Node observations and native position suite with:

```sh
CARGO_BUILD_JOBS=2 scripts/test-environment-origin-oracle.sh
cargo test -p suss-cli --locked --test compiled_macro_binding_records \
  --test compiled_macro_source_positions -- --test-threads=2
```

The oracle uses the pinned local ClojureScript checkout; it adds no shipped Java
or JavaScript dependency. New compiler and fixture code is original, with no
upstream core import or license count change.

Next retain genuine function scopes, analysis context and nominal field records,
then transport the full environment through canonical compiled data and execute
source macros that inspect it. Syntax quote/unquote/splicing, deterministic gensyms,
reproducible versioned Java-free bootstrap, complete cache invalidation and legacy
macro evaluator removal remain open. Full branch baseline, independent review and
final-head CI are required before publishing this work as a ready PR.
