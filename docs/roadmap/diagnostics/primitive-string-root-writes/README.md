# Primitive String root-write diagnostics

These exploratory fixtures are retained outside Cargo test discovery. They are
not passing compatibility regressions and do not establish issue acceptance.

- `portable_primitive_string_access.rs` preserves the entire original file from
  `crates/suss-compile/tests/portable_primitive_string_access.rs` verbatim.
  `reserved_root_write_test.rs` is the exact extracted third test,
  `primitive_string_member_keeps_selected_body_when_index_mutates_root_properties`.
  Reserved `call`/`apply` assignments fail on both unchanged `8fda18a` and the
  candidate, with a language exception and a secondary error-display layout error.
  This portable compatibility boundary remains unresolved.
- `portable_primitive_string_root_writes.rs` preserves the original test target
  of the same name verbatim. Its supplemental observer exits 101 on both versions
  because its expectation incorrectly assumes rejection of ordinary custom-field
  writes. Both phases instead produce
  `[66 66 true true false 17 [10 20 30 41]]`.

Raw before/candidate failures, original sources, snapshots and comparisons remain
in `../../evidence/m4-primitive-string-root-matched-2026-10-16.tar.gz`, SHA256
`a40321c83b0bbd9d3b383cdf19716c6a87562b82dac20db5a9e98d894c811b8b`.
The original whole-file SHA256 is
`e4795d3adb7cb5f83fffd563544f29eb0ae291c825dc9c2e82a3e7e48017fd62`;
the supplemental target SHA256 is
`18e7b821f1f5467173d39ebda1212a2e8f0b1b3fc64fa66b30d38c8421952594`.

The first two access tests remain active, along with the separate pinned
`primitive_string_root_custom_field_mutation_during_index_preserves_selected_method`
regression. Earlier filtered access runs remain two passed and one filtered;
this relocation does not retroactively turn those runs into whole-target passes.
Classification was independently reviewed before relocation. No ignore or blanket
skip was added, and no pre-existing acceptance test was removed.
