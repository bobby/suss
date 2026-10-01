# Numeric boundaries for retained public hashing

Pinned core.cljs1054–1090 dispatches public hash through IHash, finite numeric
values, Number.isSafeInteger, Math.floor and JavaScript remainder before the
existing hash-double/string algorithms. Collection hashing at1406/1420 calls that
public hash. This dependency work implements four original private scalar
adapters; it does not select or certify public hash or collection hashing.

`f64-finite` models isFinite over the existing scalar ToNumber domain.
`f64-safe-integer` models Number.isSafeInteger without coercion: only finite
integral Numbers within +/-9007199254740991 return true. `f64-floor` preserves
binary64 floor, including negative zero/subnormals/infinities. The private
`safe-integer-remainder` requires integral safe scalars and a nonzero divisor;
exact signed i64 remainder followed by the numerator sign preserves JavaScript
remainder in that domain, including negative zero. Guards precede all integer
conversions/remainder instructions. It is not general public js-mod: fractional,
nonfinite, unsafe and zero-divisor inputs raise typed errors in this adapter.

All operands evaluate once in source order before runtime checks. Private calls
are unavailable as first-class globals, with located arity diagnostics and
compile-atomic recovery. HIR/IR verify numeric and Boolean result contracts;
shared GC layouts, ABI version and automatic core cells remain unchanged. Four
runtime exports are additive. No JVM/Node dependency ships; the reference runner
uses the pinned development compiler and direct host numeric operations.

75 fresh primary/native observations cover scalar coercion, NaN/infinities,
signed zero, subnormals, safe-integer endpoints, large positive/negative remainders
and noncoercing predicates. Shared #? fixtures use :suss private adapters and
:cljs reference operations, with :suss first to preserve reader feature order.
Strict transport/independent raw decoding compare exact bits and Booleans, with
forced GC. Native error checks preserve operand effects and session recovery;
compiler HIR/IR and direct ABI guards reject malformed storage without traps.
Original reviewed parent corpora remain unchanged.

Fresh81729 ended1: the new provisional floor-negative-zero expectation used a
Python floor conversion that erased its sign. Actual pinned observation was
8000000000000000; corrected only that new unverified expectation. Fresh24694
ended0, /private/tmp/suss-hash-numeric-boundaries-primary-retry.log:68 exact
primary/native1. Focused8853 ended0 native2; compiler/ABI65505 ended0 with one
focused test each, /private/tmp/suss-hash-numeric-boundaries-compiler-abi.log.
Python80/import96/setup4/reviews185partial+880unassessed/diff checks pass.
New both-phase test compilation57845 ended101 because PreparedFragment has no
Debug bound for unwrap_err; corrected the test to explicit Err matching.
Focused84046 running; required full baseline/review/final-head CI remain pending.

Object ToPrimitive, complete public hash/default object identity, host Date
interoperability, ordered/unordered collection hashing and remaining M2–M9
acceptance remain unfinished. No inventory item is completed or excluded by
these private prerequisites. Issue98's alternative algorithm evaluation stays
deferred. Next retain public hash and collection composition with their source
algorithms and complete provenance, then execute their actual artifacts.

Focused84046 terminal0: native3, both-phase artifact/arity checks pass;
/private/tmp/suss-hash-numeric-boundaries-focused-final-retry.log. Required
full baseline now running /private/tmp/suss-hash-numeric-boundaries-full.log;
root owns exclusive heavy slot. No full acceptance claim until terminal.

Required full73818 terminal0; inspected /private/tmp/suss-hash-numeric-boundaries-full.log
through final reader doc tests. All enabled required suites pass, including new
native3/68, numeric HIR/IR1 and direct ABI1, plus reviewed parent indexing3/62
and source corpora. Manual ignored suites and diagnostic9passes/7knownfailures
remain explicit. Required command: CARGO_TARGET_DIR=/Users/bobby/code/github/
bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked --
--test-threads=2, no RUSTFLAGS. Draft PR106 candidatead73ca9 is open; independent
reviewer106 prepared additional probes while root held heavy slot. Root now
releases slot after evidence push; reviewer baseline and exact final-head CI
remain mandatory. No merge, issue or milestone completion claim.


Independent PR106 review inspected the scalar coercion domain, noncoercing
safe-integer predicate, HIR/IR types and private arities/phases, additive imports
and range/nonzero checks. No significant production defect found. Seven review
observations preserve all original68 and parent corpus bytes, adding negative
modulus zero signs, large divisors, scalar coercion, negative safe endpoints and
once-only floor effects. Fresh85831 ended0:75 exact primary/native4, in
/private/tmp/suss-pr106-review-oracle75.log. The fourth native test checks both
operand ranges, denominator zero/nonfinite/opaque failures, effects before numeric
checks and throw preventing the later operand; GC recovery retains exact signed
zero. No provisional review failures, changed original expectations or skips.
Python68129 terminal0:80 tests; import96/setup4/reviews185+880/diff checks pass.
Required independent full68749 running in /private/tmp/suss-pr106-review-full.log;
reviewer owns exclusive heavy slot until terminal output is inspected. Final
reviewed-head CI remains required before readiness; no merge or milestone claim.


Independent full68749 ended0; inspected /private/tmp/suss-pr106-review-full.log
through all final reader doc tests. Required shared-target/build2 workspace
--locked/--test-threads=2 baseline passes, including numeric boundaries4/75,
HIR/IR contracts, runtime ABI42 and unchanged reviewed parent corpora. Existing
manual ignores and explicit diagnostic9passing/7knownfailures remain unchanged.
All reviewer handles68129/85831/68749 terminal; slot released to root after
review commit/push. Exact final reviewed-head CI still gates readiness. No merge,
issue closure or milestone acceptance; public/collection hashing remains next.
