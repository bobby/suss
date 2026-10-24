# Type-value test source reference and actual bootstrap replay

Source `3c0ed527c85f5ef12eaf84e34b11f020de81e86a`, CI39297273041,
foundation118213134127: normal reproduction and identity pass, then all four
compiled_bootstrap execution tests pass with0fail/0ignored (40.66s). Retained
artifact12209106099 verifies two exact generations of all four shipped files,
source checkout/tree binding and compiler fingerprinta6694768.

The later workspace library-test build fails E0277 at
portable_macro_graph/type_value_regression_tests.rs70. Iteration over the
source slice yields&&str; SourceOrigin::new requires a value convertible to
Arc<str>. Add exactly one dereference at that call. Its source bytes and all
programs, expected observations, assertion checks, budgets and production code
are unchanged. The separate valid&str call at181 stays unchanged.

Independent review approves patch565e7c24/postimagef661a984. Parsing and apply
checks pass; no local Cargo/VM ran. No library tests executed in this failed
build, so their outcomes remain unproved.

[Raw native log, exact reproduction receipts, patch and independent review](m4-pr263-type-value-test-build-2026-10-19.tar.gz),
SHA256 `41fb7e634f319fc23d7325be996a07cf98e9b5dfbfc5f6ddba8970f7e9ca825b`. The original type error is preserved. Bootstrap assets still
belong to3c/a6694768. The compiler identity includes test source, so this
new source requires genuine normal regeneration and reviewed adoption of all
four outputs, followed by fresh bootstrap verification/execution and focused
library tests. No source-identity edit or validation bypass substitutes for
that process. Root66a55ce's same exact workspace producer remains untouched.
