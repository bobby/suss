# Genuine Error and owned prototype prerequisite authoring

Authored candidate based on `36d27750c041e707c0fd08c38b2b8725161d997c`. Bootstrap and native compilation/execution are pending coordinated validation. This prerequisite group does not establish full sorted collections acceptance.

Error, RangeError and TypeError use canonical rooted constructors and actual owned prototypes. RangeError/TypeError prototypes inherit Error.prototype. Source instances retain their actual constructor prototype reference; ancestry walks checked live references, including detached chains, rather than source names or descriptor numbers. Source fields, identity UIDs and recursive GC type layouts retain their existing contracts. Typed immutable function roots resolve build-order dependencies without host registries.

Constructor-created Errors retain mutable native property backing in an owner-held carrier. Message conversion precedes ordered cause presence/read operations. Missing cause, present nil, false and undefined remain distinct. Error.prototype.toString reads and converts name completely before message, preserves physical accessor receivers, defaults absent values correctly and propagates original throws. Live message/cause/name/constructor mutation, deletion, nonconfigurable/readonly descriptors, SameValue including NaN/signed zero, actual prototype mutation and cycle rejection are authored. Owned Object descriptor entry points perform ordered attribute reads and publish changes after validation. These are original runtime implementations; whole pinned ClojureScript source parents and their EPL notices are separately retained.

Fresh pinned compilation/Node at `c4295f303100bbf5afac449242d30bca1126f1a1` passes all151 ordered raw observations. Original87 observations are an unchanged prefix and separately retained. New64 cover mutation, toString, physical getters/setters, conversion/attribute order, original throws, descriptors, deletion and cycle errors. No guest equality or printer supplies the host observation. Authored native tests independently decode canonical owners/UTF16/raw payloads/traces after GC, compare actual constructor/instance prototype references and Error backing identity, and reject invalid internal masks with explicit restoration/recovery. They are UNCOMPILED/UNEXECUTED.

The source adaptation mechanically lowers all23 retained sorted defn declarations to def/fn, retaining docs, arglists, privacy, signatures and every body. All382 selections retain their original order and licensed originals. The complete ExceptionInfo Error-prototype initializer is retained and mechanically expanded in the source loader. The full nil-iter/reify body remains; Hilbert owns compiler lowering and shared graph integration.

General Function/foreign JS object reflection, Object boxing/Symbol property keys and the full analyzer are not established by this group. Full ES6 symbol/prototype setup, all36 printer targets and exact missing-protocol messages remain subsequent whole-source prerequisites. No collection marker, failure expectation or source branch is removed. Bootstrap artifacts are unchanged/stale and must be regenerated against the entire graph; genuine graph blockers must be repaired rather than bypassed.

Authoring commands:

```sh
sh scripts/test-error-constructor-oracle.sh
python3 -B -m unittest discover -s scripts -p 'test_*.py'
python3 -B scripts/check_error_constructor_provenance.py
python3 -B scripts/check_sorted_tree_provenance.py
python3 -B scripts/core_import.py --check
git diff --check
```

Parent native commands, in an isolated exact-head validation checkout after coordination:

```sh
cargo run --profile test -p suss-cli --bin suss-bootstrap --locked -- /private/tmp/suss-error-validation-bootstrap
# Install all four regenerated outputs in that validation checkout only.
cargo test -p suss-cli --locked --test portable_error_constructors -- --test-threads=2
cargo test -p suss-cli --locked --test portable_named_properties -- --test-threads=2
cargo test -p suss-cli --locked --test portable_native_objects -- --test-threads=2
cargo test -p suss-cli --locked --test persistent_session -- --test-threads=2
sh scripts/verify-bootstrap.sh
cargo test --workspace --locked -- --test-threads=2
```

Commands above are required validation, not reported results. Preserve bootstrap identity checks and exact existing negative expectations. A failed graph bootstrap means runtime execution remains blocked.
