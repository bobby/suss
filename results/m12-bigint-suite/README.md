# Independently decoded BigInt suite observations

The raw captures execute the pinned jank suite against11610506, before and after
adding independent BigInt observation decoding. Original namespace/test/skip
failures are unchanged. The decoder captures the canonical integer storage type,
checks sign/length/normalization/display fields, charges every limb read and
formats exact signed decimal digits on the host. It never uses guest printing,
equality or arithmetic to judge an operand. Long and BigInt remain distinct
observation categories, as the primary oracle records them.

`review.json` retains every changed assertion's full primary and actual rows.
The audit checks88 changes:73 new independently verified passes and15 existing
exact-division gaps that now execute with a wrong double result. The latter
remain failing verdicts. Counts are2386 pass,54 guest-judged,356 fail,4143 not
executed and124 namespace failures. No unknown/opaque result becomes success,
no namespace/test/skip failure is removed, and the primary reference is unchanged.

Reproduce the lossless review with
`python3 results/m12-bigint-suite/review.py`. The gzip captures decompress to
original JSON bytes. `run.py` describes fresh suite/decoder execution in the
recorded isolated target; its terminal101 is the comparison against the old
baseline, not a passing execution gate. Original captures/logs remain separate.

The analyzer source round-trip exposed a different defect: exact-number
transport tags were emitted as source tags. Validated exact tokens now restore
source forms and print as numeric literals, including metadata and nested data.
Malformed tagged text retains its tag and cannot inject source. The exact
reader regression, both core analyzer phases and24 decoder tests pass;34 Python
comparison tests pass. `final-checks.json` records hashes and scoped results.
Final-head CI and a complete workspace baseline remain required.
