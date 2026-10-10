# String.charAt member and complete get prerequisite

This checkpoint implements the original `String.prototype.charAt` member as an
owned portable runtime-ABI adapter over UTF16 storage. Receiver conversion
precedes index conversion; the index is truncated, a NaN index reads as +0,
and an in-range index returns exactly one UTF16 code unit as a fresh one-unit
string while an out-of-range index returns the empty string. A nil or
undefined receiver is a genuine error. The anchored method flavor treats a
missing index argument as the undefined sentinel; the detached borrow flavor
(`(.call (.-charAt "ab") 17 1)`) always errors. One canonical closure is
lazily rooted and shared for every receiver, so separate `.-charAt` lookups
are reference-identical after GC. No host JavaScript executes; general
Closure/prototype interoperability is not claimed.

The complete retained `get` declaration's string branch dispatches to this
genuine member (`(.charAt o (int k))` inside the existing `some?`/length
guard) for both its two- and three-arity calls. Its pinned expectations are
unchanged, and the suite's two string-get assertions
(`clojure.core-test.get/test-get#13`, `#28`) now execute and pass.

The closed 74-case corpus independently compares raw UTF16 units, binary64
bits, nil/Boolean results, throw payloads and ordered `valueOf`/`toString`
effect traces across ascii/UTF16/empty receivers and
negative/zero/fraction/one/two/three/past/huge/NaN/infinite/nil/false/string
indexes, object-index conversion order and thrown conversions, default and
extra arguments, ordered receiver/index evaluation, borrowed-method
receivers, three-arity default eagerness, method identity, and both suite
get forms. Fresh pinned compile/Node/compare printed `74 raw pin matches`
(`evidence/primary-compare.log`); the first oracle compile attempt failed on
a wrong namespace require and is retained verbatim
(`evidence/primary-compile.log`), the corrected final compile produced no
diagnostics (`evidence/primary-final-compile.log`), and the pinned
observations are `evidence/primary-observations.json`.

The two native tests (`portable_string_char_at.rs`) execute the whole corpus
in both REPL and Macro sessions with post-GC independent decoding of every
value, throw and effect counter plus a recovery evaluation after each case,
and check the shared canonical method identity by reference equality after
GC. They compile and pass at the integrated head. The retained
`evidence/eaec-integration1-job.log` CI job (merge
`d09b45a` of `eaec26de` into `06ac94b`) shows the full workspace baseline
failing only on the suite comparator (`fail=22 ... pass=148`) before this
checkpoint; `evidence/core-check.log` and `evidence/core-regenerate.log`
retain that lane's 360-file core verification and regeneration (the current
integrated head verifies and writes 361 files after the str/StringBuffer
retention), and `evidence/python.log` retains the 278-test Python suite run
of that lane.

The raw-pin corpus contains only source expectations and pinned observations;
it does not carry a separate native-status field that can become stale. Native
evidence is recorded in `evidence/pr231-integrated-native.log`: the five
affected integration targets (reify, record graph, record iteration, Object
methods and String.charAt) pass 11 tests with zero failures using the locked
Cargo command.

`get`'s remaining non-string branches are unchanged and still carry their
own pending dependencies. This checkpoint does not implement reify/nil-iter
or close original #18/#19.
