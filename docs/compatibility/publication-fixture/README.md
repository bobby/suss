# Publication undefined fixture correction

At 677340c the two native publication tests failed after reaching the kernel:
expected false, actual true for literal `(make-array 1)` slot zero. Whole pinned
macro make-array2644–2660 expands a numeric literal into `(array nil ...)`; that
is a present nil value. Dynamic size/explicit `js/Array` construction creates
holes/undefined. No runtime or binding-defined defect is inferred from the wrong
fixture. Whole source witness and hashes are retained.

Fresh pinned four-case raw fixture distinguishes undefined?/identical nil/own
presence and exists?; it also observes global undefined→nil publication and
dynamic undefined restoration to nil. All four ordered tagged Boolean vectors
match actual fresh compilation/Node exit0. Original native literal source is
retained as a true nil-publication case and a genuine js/Array hole false case
is added. Both dynamic undefined fixtures now use the same genuine hole; their
false expectations, restoration, class/late publication, nil/false/object/no-hook,
malformed/hidden target and GC assertions remain. No production/fuel/bootstrap
change. Corrected native execution pending coordinated parent lane.

Run `sh scripts/test-publication-fixture-oracle.sh`. This repairs a fixture,
not the complete exists? macro or reify/records. Earlier 0/2 failure logs remain
diagnostic evidence, not passing checks.

Three fail-closed harness tests pass; core-import --check360 passes without any
recipe/review/manifest edit. Rustfmt/diff checks pass. Stale top test comments
refer to authoring history; corrected tests still need exact native execution.
