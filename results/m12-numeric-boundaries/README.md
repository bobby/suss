# Raw/boxed numeric and primitive-boundary observations

Sixteen fresh JVM Clojure1.12.6 cases execute in a real AOT component and match
all expected results. They cover raw/generic integer expressions meeting BigInt
and Ratio operands, hash/equality/order, promotion, primitive parameters/results,
numeric captures and case constants under public-var replacement. The same
forms are included in the104-case standard both-phase/AOT matrix.

probe.py retains primary JAR/source hashes, the actual compiler/component hashes
and every invocation status/value. The first fixture attempted println, which
is not available in this CLI route; its source and failure remain. The corrected
fixture exports indexed strings through WIT and compares every returned value.
These are behavior observations, not allocation or quiet walltime measurements.
