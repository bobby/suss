# Multiplication crossover and namespace membership refinement

The GC multiplication kernel caches its row limb and result index. On the same
512x512-limb input its measured instruction count falls from23.1M to17.36M;
the copied no_std helper uses5.36M. The dispatcher uses both logical lengths:
choose the helper only when each operand has at least2 limbs and
(la-1)*(lb-1) >=8. Zero/one-limb products and small products stay in GC. This
conservative boundary keeps2x8 in GC and sends2x9,3x5 and4x4 to the helper.
The separate boundary probe retains both operand orders near the crossover.

commands.json is terminal with all commands0 and frozen inputs unchanged:
2 sort tests,67 compiler/ABI/storage tests,26 CLI numeric/catalog/record tests,
and4 bootstrap tests (99 Rust tests). The pinned1.98.0 helper rebuild is byte
identical, Java-free bootstrap generation is reproducible in both phases, and
the core manifest passes. The rebuild command clears RUSTUP_TOOLCHAIN only;
developer RUSTFLAGS are preserved. Notices correctly identify Clojure/jank as
the numeric behavior oracles and ClojureScript as an implementation reference.

The final wrapper profile verifies320 complete observations across auto/null
collectors,512 instruction samples and20 allocation samples. Every invocation
checks its complete GC result against independent arithmetic. Both observed
slopes, raw commands and source/module/executable hashes remain. Allocation
controls measure approximately40+4*capacity bytes for the complete value and
array, with page-granularity variation. No walltime claim is made.

run-compiled-bench.py invokes unchanged bench.py measurement functions for four
bounded, compiled Suss multiplications. Actual AOT validation checks the same
products against the fresh primary fixture. The instruction/byte columns are
retained in compiled-bench.csv, with source/component hashes and every command.
Representative gross measurements are2434 instructions/72.5 bytes for4x4,
95124/674.8 for32x128,36298/2090.2 for1x512 and56494/2098.8 for2x512. Byte
measurements are whole-loop RSS slopes. Operand controls remain below RSS
resolution; their cells and all unavailable net-byte values stay empty. JVM
performance was not measured. The first driver read its executable wholesale
for hashing, raising its inherited RSS floor; that complete affected run stays
under inherited-floor-*. Streaming hashing fixes the sampler overhead.
The preliminary CLI-e validation exhausted its default fuel on these large
printed values. That failure remains under initial-eval-*; the same values are
validated through the compiled unmetered component path used by bench.py.
Production fuel limits are unchanged.

Record membership transport still emits every namespace name and preserves
UTF-16 ordering and duplicates. Its merge sort now skips an ordered boundary
after recursively sorting both halves, avoiding unnecessary copies and their
work charges. The original unchanged record budget test now measures
940200/951046 against the strict one-million gate, versus992401/1003928 before.
Source-order/UTF-16 and bounded-work regressions execute, as do compiler catalog
and record tests. The earlier scalar fallback shortcut had no effect on this
span and is removed; its failed attempt and exact patch remain in the refined
receipts. No acceptance threshold, data projection or skip changes.

The historical full workspace retries retain their terminal failures; they do
not certify this later source. A new unfiltered workspace run and final-head CI
remain required. Complete BigInt/Ratio API, inline, tooling and numeric-suite
acceptance, and BigDecimal, are separate outstanding requirements.
