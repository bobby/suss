# BigInt value integration receipts (#338)

The compiler emits exact BigInt literals into the shared GC runtime. ABI8 adds
an immutable struct containing sign (−1,0,1), a GC-owned little-endian u32
magnitude array and its significant length. Arithmetic writes fresh result
arrays; negation can share the immutable magnitude. The published value never
owns linear memory. Normalization gives zero sign0 and length0.

The original GC kernels provide signed addition, subtraction and multiplication.
Ordinary long-only arithmetic retains its overflow checks. A mixed BigInt/long
result remains BigInt. The GC multiplication kernel is a functional reference;
connecting the measured larger-product helper path remains separate work.
The placement probe's selected magnitude storage is used here, but these value
wrappers have not yet been profiled for a production crossover.

Decimal formatting uses repeated division of a private copy. str omits N;
readable printing appends N through the owned printer patch and regenerated
core import. The binary64 conversion extracts the leading53 bits, checks every
discarded bit and rounds once, ties to even. It does not accumulate rounded
floating limb sums. Integer arithmetic never passes through binary64.

Macro datum import/decoding preserves real BigInt values, rather than using the
old namespaced-symbol placeholder for them. Ratios and decimals retain their
existing explicit unsupported boundaries. The runtime read-string integer path,
promotion functions, exact division/Ratio, numeric equality/comparison/hash,
conversion and WIT acceptance still require integration; this receipt does not
certify the complete numeric tower.

## Reproduction and evidence

Run the locked focused command with two build jobs:

    cargo +stable test -p suss-cli --locked --test clojure_bigints -- --test-threads=2

The retained local executable target is target/probe/m12-bigint-target beneath
the enclosing worktree. No RUSTFLAGS override is used. The wide-value matrix
explicitly raises Session operation fuel to1,000,000,000; its16,384-bit product
and decimal printing exceed the ordinary interactive operation budget.

cases.json contains35 fresh JVM Clojure1.12.6 expected results. They include
radix/overflow literals, carry/borrow, signed zero/cancellation, mixed long
operands, closures, source evaluation order, operand widths through512 limbs
and both sides of binary64 midpoints. The AOT export returns a unique failure
index or the case count, so a success requires reaching every case.

oracle-command.json retains commands, terminal status, source and JVM JAR hashes.
The initial radix expectation was wrong: a trailing N is a base36 digit. Its
failed oracle fixture/log remain as initial-oracle.*; the corrected expectations
match the primary oracle. reduced-ratio-oracle.log separately records JVM's
BigInt for the reader-normalized literal -9223372036854775808/1.

language-before.log records the initial unsupported-literal failure before the
HIR/runtime wiring. focused.log retains the first runtime validation failure;
focused-nullability.log retains the fragment import-signature failure. The
constructor now accepts nullable scratch locals and proves its array non-null
at publication, and compiled fragments import its exact typed signature.
Final results and source identities are recorded alongside these attempts.

The executed `focused-import.log` run passes3 tests:35 values in each of
Runtime/Macro and35 in the real AOT export (105 matrix observations), plus
real BigInt macro arguments/results in both caller phases. The final source
adds retained-magnitude GC tests and clarifies comments. run-focused.py is
running that expanded four-test target followed by the nearby numeric and ABI
controls. Its live commands.json/logs retain terminal statuses as they arrive;
no broader or unfiltered baseline pass is inferred here.
