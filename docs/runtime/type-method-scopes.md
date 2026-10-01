# Type method name resolution

Methods declared by deftype resolve namespace globals, their own parameters and
physical fields. They do not capture a surrounding let, function parameter or
outer type field. This follows the pinned analyzer.cljc parse-type3614–3649,
which replaces enclosing locals with the type fields, and core.cljc deftype1778.
Suss previously captured those surrounding values, so a global11 shadowed by
local7 caused a method to return35 rather than the pinned55.

Original HIR lowering now isolates the method analysis environment, then restores
its surrounding locals and field aliases. Runtime extend-type remains a normal
expression that captures enclosing locals. Method parameters shadow fields;
functions inside method bodies capture method locals normally. No Wasm ABI change,
new shipped dependency or copied upstream form is introduced.

Fourteen fresh observations at c4295f303100bbf5afac449242d30bca1126f1a1 match independent
scalar decoding from validated Wasm after forced GC. The original ten expectations
are unchanged. Four independent review additions check qualified globals,
self type/arrow constructor references despite outer lexical shadows, and
sibling method parameter isolation. A second native test checks located unresolved names, compile-atomic publication and
session recovery. Unknown source names still have explicit compile errors rather
than inheriting undeclared JS variable behavior.

```sh
CARGO_BUILD_JOBS=2 sh scripts/test-type-method-scope-oracle.sh
```

Initial47266 ended101: eight exact primary matches, then native35 versus55.
After the method scope fix11681 ended0 for those eight. Final94267 ended0:
ten exact primary matches and both native tests pass. Logs:
/private/tmp/suss-type-method-scope-{primary-native-red,primary-fix,final-primary}.log.
This does not implement Object methods, compiled macro bootstrap, complete field
attributes or full core acceptance. Those remain separate work.

Independent PR #91 review session 8599 ended 0: fourteen fresh pinned observations match
validated native Wasm after GC; both native tests pass. Log:
/private/tmp/suss-pr91-review-primary.log. The pin's undeclared generated-arrow
warning for the self-reference probe is retained; its executed value matches.
