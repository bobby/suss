# Observed completion of the unchanged full workspace run

Source head: `9f1cf7269dfc531eff77859a61f2b42e509eb170`.
Command: `cargo test --workspace --locked -- --test-threads=2`.

The native Cargo process survived after its original outer recorder detached.
The posthoc observer watched that original process terminate. The frozen log
contains all 369 expected native and doctest targets, with 2,468 passing tests,
14 explicitly ignored tests, zero failed tests and zero filtered tests. All
recorded input hashes and HEAD remained unchanged.

**Cargo's exit status was lost and remains unknown.** The original incomplete
receipt and first observer are retained. This is complete native target coverage
and passing test-result evidence, not a recovered Cargo exit code of zero.

The first observer audit incorrectly omitted explicit `rlib` library targets
and could not disambiguate same-named integration tests in different packages.
The corrected auditor includes `rlib` and uses each executable's Cargo
fingerprint package identity. It finds exactly 369 unique targets, with none
missing or unexpected. Both audit reports are preserved; the correction changes
no test result, source, fixture, budget or ignore rule.

The independent promotion review must assess this qualification together with
the passing final-source-head CI before any ready-for-review decision.
