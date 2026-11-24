# Complete native workspace evidence

Frozen source: `61397661ddba8d0022b876d1163dd170ea3ef0ea`.
Exact command: `cargo test --workspace --locked -- --test-threads=2`.

The durable recorder captured Cargo exit 0 after 11,089.347 seconds. All
recorded inputs and HEAD stayed unchanged through native termination. The
archive-only target audit accounts for all 375 expected native/doc targets:
2,496 passed, 15 explicitly ignored, zero failed and zero filtered. One ignored
Binaryen optimizer test was separately executed on the same frozen source
and passed; its receipt is in `../../optimized-after-final-baseline`. The
remaining ignores retain their original unsupported/manual/doc qualifications.

Run `python3 replay-targets.py` here to verify every target/result from the
archived log, Cargo metadata and fingerprint snapshots, without live build
outputs. The recorder supplies the actual exit and original source-boundary
observations; this replay does not independently repeat that past boundary.
Archive hashes are recorded in archive-manifest.json. The archive writer is
retained for provenance; use the replay after cleaning or changing checkouts.

Parent merge qualification records exact preservation of every pre-existing
implementation, test, fixture, manifest and evidence blob, except handoff prose.
Earlier lost-exit probes and failed attempts remain preserved separately.
