# Observed completion of the exact min-key workspace baseline

Source head: `33382c1cd8b4c70f5ef8811809441364bace8c49`.
Command: `cargo test --workspace --locked -- --test-threads=2`.

The original native process completed all370 expected native/doc targets:
2,473 tests passed,14 were explicitly ignored,zero failed andzero were filtered.
All recorded input hashes and HEAD remained unchanged until termination.
**Cargo's exit status is unknown:** its original outer recorder detached.
Neither the observer's exit code nor a passing result summary replaces it.

The first observer omitted explicit rlib targets and could not disambiguate
same-named integration targets in different packages. The corrected audit uses
Cargo fingerprint package identities and accounts for every expected target.
Original incomplete receipt, native log and both audits remain preserved.

`python3 replay-targets.py` rechecks target/result coverage using only the
archived log, metadata and fingerprint snapshots, without live target files.
It verifies370 unique targets and the2,473/14 result counts. It cannot recover
Cargo's status or repeat the original source-boundary observation. That separate
post-termination check is retained in corrected-target-audit.json. The older
original-audit-completed.py intentionally retains its original dependencies;
use replay-targets.py after cleaning build outputs.

The independent promotion review must assess the lost-exit qualification,
functional acceptance and final-head CI before ready status. No merge is claimed.
