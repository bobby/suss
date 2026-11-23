# Ratio suite capture and lossless primary review

The frozen84fece33 source executes the unchanged pinned jank corpus with
independent integer/fraction storage decoding. The raw capture is retained
losslessly, with executable/source identities and terminal status in command.json.
The original101 is the comparison against the preceding integer baseline.

`python3 results/m12-ratio-suite/review.py` reproduces every changed assertion
and newly enabled namespace. It verifies94 recovered failures and32 additional
primary matches:2512 host-judged passes,54 guest-judged observations,266 failures,
4107 not executed and121 namespace failures. No previously passing assertion
regresses, no opaque/guest-only result is promoted, and test/skip failures are
unchanged. Only numerator, denominator and rationalize namespace failures are
removed; their36 unchanged assertions are all inspected. A fresh JVM1.12.6 run
of those three original namespaces passes all36 assertions.

Four newly executed rationalize assertions remain errors on Decimal literals.
They are recorded as failures for the remaining Decimal implementation. Two
thrown assertions use Decimal arguments: agreement on throwing does not prove
the callee executes or establish Decimal semantics. Each such limitation is
attached to its row in review.json. This review accepts actual observations;
it does not establish full numeric conformance or milestone acceptance.

Original CI logs are retained in ci. They expose old floating quotient
expectations, missing exact-number observation variants and the stale ABI9
fixture. The corresponding focused repairs and primary evidence are recorded
separately in results/m12-ratio-ci. No failed run is relabeled as a pass.
