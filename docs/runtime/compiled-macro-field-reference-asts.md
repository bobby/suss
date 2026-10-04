# Source field reference ASTs

A source symbol resolving to a type field now supplies a portable local AST:
`:op :local`, `:local :field`, its original declaration name and the actual
lexical field record as `:info`. It does not classify a computed invocation or
read the current runtime field value to invent source information.

Field records retain raw `:mutable`, `:unsynchronized-mutable`,
`:volatile-mutable` and `:tag` metadata, including present nil and false. These
are separate from the compiler's combined physical mutability boolean. Unique
field declarations retain present nil `:shadow`; duplicate declarations remain
explicitly rejected. Source line/column come from the original symbol position.
Top AST tag inference keeps nil hints absent and false hints present. Within one
source environment graph, initializer `:info` shares identity with the lexical
field declaration, including fields shadowed by local bindings.

The original development probe executes the pinned ClojureScript analyzer at
`c4295f303100bbf5afac449242d30bca1126f1a1`, then executes its output in Node.
Nine exact observations cover plain and three mutable flags, false mutable,
number/nil/false hints and a local shadow. Native execution compares the same
corpus in both caller phases after GC. The projection omits only private `suss/*`
diagnostic keys from nested records; all observed portable fields, values,
presence bits and identities remain exact. No analyzer implementation is copied.
JVM and Node remain development oracles, with no shipped dependency.

The unchanged parent failed the first plain field assertion: source AST local
classification and `:info` were absent. The repaired focused regression passes.
Complete source AST schemas, identity across separate macro invocations and the
remaining macro bootstrap/cache/evaluator/lifecycle requirements are not certified
by this bounded evidence. This is partial progress on issue #14.

Commands:

```sh
sh scripts/test-field-reference-asts-oracle.sh
python3 -m unittest discover -s scripts -p 'test_field_reference_asts_oracle.py'
CARGO_TARGET_DIR=/private/tmp/suss-m3-pr143-target CARGO_BUILD_JOBS=2 cargo test -p suss-cli --locked --test compiled_macro_field_reference_asts -- --test-threads=2
```

Full unfiltered baseline, independent PR review and exact final-head CI are still
required before this increment is ready.
