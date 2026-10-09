# Whole source helpers for the record/reify dependency graph

The import appends complete pinned `vary-meta:4171`, `fnil:4526`,
`update-in:5530` and `group-by:11244` declarations after all 336 existing
selections. Extracted forms retain their upstream copyright and EPL notice.
Source-hash-bound patches expand defn to def/fn while retaining documentation,
every fixed/variadic arity and every algorithm branch. `fnil`'s nil macro guards
use the immutable `suss.bootstrap/nil?` adapter; all other helper calls remain
live. No transient path is replaced with a persistent-only algorithm.

The 33 closed ordered Boolean cases cover all outer helper arities, all returned
fnil arities (including the three-default/two-argument branch), false values,
metadata and original-object preservation, argument/callback order and throws,
missing/empty nested paths, nil/false grouping keys and transient map promotion.
The upstream runner freshly compiles against the clean pinned checkout and uses
strict JSON decoding; numbers cannot impersonate Boolean observations. All 33
matched the pin. Raw observations and the generated reference fixture are retained
under `evidence/`. No source fixture is substituted for a native result.

Two authored native regressions consume that exact corpus in Runtime and Macro
phases with GC before decoding, and retain a fnil callback/captured vector/default
through only a host callable handle across GC and independent invocations. They
are UNCOMPILED/UNEXECUTED. Bootstrap artifacts remain unchanged and stale for this
authoring worktree. Existing seven reify/fourteen iterator cases and twelve
retained macro declarations are unchanged.

Commands:

```
sh scripts/test-record-helper-oracle.sh
python3 -B scripts/core_import.py --check
python3 -B scripts/check_record_helper_provenance.py
python3 -B scripts/cljs_reviews.py
python3 -B -m unittest discover -s scripts -p 'test_*.py'
```

These complete helpers are dependencies, not an implementation of exists?, reify,
deftype's full source helper graph or record factories. Source analyzer state
adapters, anonymous captured class publication, protocol-mask/property integration
and the real nil-iter reify path remain required. Full record/reify and original
issue acceptance stay open.
