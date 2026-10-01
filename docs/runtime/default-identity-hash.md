# Retained default identity hashing

This work retains the complete pinned default IHash extension at core.cljs
1492–1497. Its source branches still distinguish the root Object prototype from
ordinary owners. An explicit standalone patch replaces only goog/getUid with
the GC-owned identity adapter. The retained private root-obj declaration uses
a zero-argument storage adapter to the existing canonical Object prototype.
There is no new registry or runtime layout change beyond ABI2 from PR107.

The provenance verifier checks the complete source form, exact source hash,
explicit patch, retained form head/target, final loader bytes and upstream EPL
notice. Core extraction now selects97 forms/101 licensed artifacts; review
statuses remain partial (190 reviewed/875 unassessed). No public hash, collection
composition, complete metadata or milestone acceptance is claimed.

Twenty-one observations now match fresh pinned ClojureScript and independent
native decoding, preserving the original eighteen. The corpus covers root-object
zero, default identity, mutation, errors, class/protocol owners, metadata, direct
method priority and live root lookup. Native2 and compiler1 focused tests pass,
including typed scalar-owner errors, effects, GC recovery, compile atomicity and
HIR/IR rejection. The fresh oracle emits expected private root-obj access warnings
for development-only probes; no shipped Java dependency is introduced.

This branch is rebased onto independently reviewed PR107 commit7b530784. Focused
parent protocol/identity/sequence tests pass, including its unchanged exact IFn
strict-arity boundary. Python81/import101/setup5/reviews190+875/diff checks pass.
The required full workspace baseline, independent PR review/fixes and exact
reviewed-head CI remain pending. No milestone is complete from these tests.

Required validation:

```
sh scripts/test-default-hash-oracle.sh
cargo test -p suss-cli --test portable_default_hash --locked -- --test-threads=2
python3 scripts/core_import.py --check
python3 scripts/sequence_provenance.py
python3 -m unittest discover -s scripts -p 'test_*.py'
cargo test --workspace --locked -- --test-threads=2
```

Next integrate the full public scalar hash and ordered/unordered composition,
then the remaining persistent collection types, metadata/transients and release
gates. The public hash's Date branch and source case macro dependencies require
explicit handling; do not remove those branches to certify a smaller function.
Issue98 continues to defer algorithm evaluation.
