Guarded canonical long chunk reduction experiment, 2026-11-20.

The measured snapshot is the exact dirty patch in measured-source.patch, based
on11b57332; candidate.json identifies its executable/core/source hashes. Thirty
focused tests execute both phases, and the public cases match fresh JVM Clojure
1.12.6 in jvm.clj/jvm.log. The Java-free release-profile bootstrap check reproduces
both phases byte-for-byte and executes four bootstrap tests. Foundation runs358
Python tests and all CI source/provenance checks. Tests use stable Rust under
systemd user scopes with MemoryMax; all temporary data stays in target/probe.

The unchanged seven bench.py expressions use a warm-up first, followed by median
three instructions:u counts at1000 minus0. The lazy-pipeline count is984,880.429,
versus1,059,416.537 for the published predecessor (7.035581% fewer). The prior
bounded-index/split-loop experiment measured1,024,654.703; adding the guarded sum
reduces that by3.881725%. Other program counts are retained in instructions.json.
These instruction counts do not establish the quiet5× timing criterion.

The helper checks the exact owned canonical + value and a bounded Object[] window,
accumulates only long values with checked prefix overflow, and boxes once per
chunk. Unsupported domains retain the complete source loop. A later unsupported
value can replay only a pure long prefix; no callbacks or writes are duplicated.
Overflow uses the existing number-add error path at the same element, before
reading any later element. Custom/redefined reducers, mixed numbers and reduced
termination retain their source behavior. Only proved bounded counters/offsets
use the new unchecked raw addition; ordinary source core keeps checked arithmetic.

An earlier loop-versioning experiment passed semantics but did not reduce the
lazy instruction count and is not part of this candidate. Its isolated source and
raw measurements remain under target/probe/m15-array-view-version-*.

The full local probe directories and frozen executable remain under target/probe.
The next commit adds documentation/evidence only after the measured snapshot;
quiet final-head CI must independently establish acceptance. Prior valid failed
quiet results remain on issues482/487/446 and are not replaced by this experiment.
