# Root and metadata validation archive

Run `python3 results/m12-eval/root-metadata-checkpoint/verify.py --current` to reconstruct all recorded inputs and verify current source plus the explicit documentation publication. Omit `--current` for historical verification after later implementation.

Fifteen successful command groups across two frozen batches return actual0: complete-public-eval=103, ci-macro-and-stream-targets=14, ownership-library=138, source-frontends=58, runtime-abi-and-structural-guards=71, named-calling-hash-sealing-and-ci-display=33, persistent-repl-display-controls=8, java-node-free-bootstrap=4. Only the REPL display test and its provenance verification files change between the two batches; production inputs are identical. The original display-target101 remains in its receipt. Python tests, original/source/core/sequence provenance and Java/Node-free bootstrap also return0. These local results do not replace fresh complete native conformance, the exact two-thread workspace baseline, issue acceptance, independent review or final-head CI.

The display correction checks every output byte while deriving only valid identity hex values, checks matching array identity spellings and distinct IDs, and adds repeated-value identity stability. Allocation order of compiler-owned identities is not a Clojure contract.

The archive retains every failed attempt, including missing constructors, misplaced root annotation, lost compiler policy, fuel exhaustion, a bad provenance assumption and unit compilation errors. Two deliberate stops preserve their exact source/runner/log inputs and stop reasons; their Cargo101 results remain failures and are not coverage passes. Original fixtures, count/skip/opaque rules and operation allowances remain unchanged.
