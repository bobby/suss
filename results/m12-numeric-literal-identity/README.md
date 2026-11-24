# Exact-number literal lifetime controls

The full34-control primary recording is retained unchanged under primary. It
covers source boxing, NaN and primitive identity as well as exact-number literal
sites. It is research evidence; no native pass for all34 is claimed.

The focused regression selects all8 explicit BigInt/Ratio/Decimal site controls
(4 function sites and4 distinct-read pairs), executes them in both Runtime and
Macro, and separately compares actual returned references across calls, source
fragments and GC. The exact before command returns101 in192.74s with source/HEAD
unchanged. All4 function-site controls fail in each phase, and the independent
retained-root check fails for1N. Distinct exact literal pairs correctly differ.
The missing behavior is retained literal lifetime, not numeric value equality.

Do not globally intern numeric source text or change persistent numeric fields.
Keep object-site identity separate from Clojure's source-dependent primitive
boxing/NaN rules. Original eq/not-eq strict142-body validation remains failing on
3 NaN cases; primitive identity and complete numeric acceptance require their own
checks. No source baseline, original suite or primary rule is changed here.


## Literal roots retained through compiler inlining

Exact literals now have lazy private cache globals in their owning Wasm module
instance. Successful construction publishes the immutable object once; a failure
leaves null. Separate original constant sites remain distinct. No external
binding history, numeric storage field, encoding, kernel or Java dependency is
added. Source lowering attaches a compiler-local shared site token to the original
BigInt/Ratio/Decimal instruction. IR cloning preserves it through inlining and
continuation transformations; synthetic instructions have none. Emission uses
first-traversal deterministic indices, never serialized pointer values. Derived
or manually authored instructions without a source site keep fresh construction.

The first cache attempt roots returned objects across calls/fragments/GC but
fails the four within-expression function-site controls in each phase and their
actual AOT equivalents: closure inlining copied the instruction and duplicated
its cache. Its terminal101 receipt, successful prefix and verified exact source
overlay are preserved under exact-literal-roots-after-module-caches. Ratio tests
after the failed target did not execute in that command. This is not a pass.

After preserving original site tokens, the unchanged-source expanded command
returns0 in537.85s for33 complete-target tests: analyzer18, BigInteger2, BigInt4,
Decimal2, exact literal identity3 and Ratio4. All8 selected primary exact-number
site cases match in Runtime and Macro, actual returned references survive GC and
fragment boundaries, and an actual AOT component matches all8 primary cases.
The original34-control recording remains intact; primitive boxing/NaN cases in
that research set are not established by these8 exact-number controls.

IR resource/pipeline/continuation consumers, artifact determinism, broader
module/inline/bootstrap/workspace checks, strict complete numeric bodies,
independent review and final-head CI retain separate gates. The three NaN
failures in the142-assertion equality bodies remain unresolved and strict.


## Source artifact cache preserves original-site alias partitions

A standalone probe linked against the frozen f1a011e3 compiler confirms an
artifact-key collision: distinct and shared literal sites emit different fresh
Wasm (1056/1051 bytes), but source reuse returns the first artifact with one
miss and one hit. Stable rustc build returns0 and the actual probe returns101;
source, HEAD and rlib hashes remain unchanged. The cache-site-partition-before
receipt and complete probe preserve the counterexample.

The running IR/artifact command was stopped for that confirmed same-source bug,
not a timeout. Its actual cargo exit101 records SIGTERM of portable_pipeline
after2044.14s with source/HEAD unchanged. Five complete groups pass37 tests and
21 other pipeline tests report success; the397-case compiler corpus has no
terminal result and is not certified. Stop/resource-adjustment receipts and
compressed actual output remain beside that command.

The cache key now additionally hashes the original literal-site alias partition
in deterministic traversal order, including nested function and continuation
bodies. Process addresses remain temporary map keys; only ordinal indices are
hashed. Regression checks distinguish alias partitions and require cache hits
for independently allocated copies with the same partition. Validation results
for this correction must be retained separately from the canceled command.


The first cache-unit command returns101 solely because its newly added nested
fixture discards an unused function: fresh distinct/shared graphs then both
contain one surviving literal. Its11 other unit tests pass; later commands do
not execute. The exact source file, frozen hashes and output are retained under
site-partition-cache-and-number-before. The corrected fixture retains two
reachable literal branches in a nested function.

With that fixture correction, all12 source-cache unit tests return0 in148.01s
(including compilation). The selected IR/API/artifact/cache and exact-number
identity targets return0 for40 tests in400.27s. Both commands share frozen
source/HEAD through terminal. This does not certify the canceled397-case corpus
or the full workspace. Their actual logs and per-command receipts are retained
under site-partition-cache-and-number-before-corrected-fixture.

The subsequent NaN before-regression returns101 in91.27s: independent storage
decoding in Runtime/Macro and actual AOT both fail. The two decoder adversarial
controls pass. That failed command is preserved separately from the52 passing
cache/compiler/native checks. No NaN fix or numeric acceptance is inferred.
