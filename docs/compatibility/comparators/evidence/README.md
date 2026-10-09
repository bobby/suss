# Comparator authoring evidence

`current-primary/` records a fresh pinned compile/Node run of all 49 exact ordered
Boolean candidate checks, terminal0. The strict Python comparator checks exact
IDs/count/schema/Boolean types and fails on any false/missing observation. Its
original corpus, driver, shared probe, actual warnings/log, raw observations and
command receipt are retained. This is development reference execution only.
No Suss/native result is supplied for this draft.

`historical-64/` preserves the previous plan's 64 cases, original driver, tagged
observations and actual available logs byte-for-byte. They are historical
reference evidence, not native results or execution of the newly authored code.
The initial upstream Node-working-directory failure was described in the old
plan; these copied corrected-run records do not relabel that failure as a pass.

The new failures are retained explicitly:

* `initial-authoring/`: pinned compile rejected two malformed test expressions.
* `second-authoring/`: compile succeeded but Node rejected the single-arity
  replacement for multi-arity public `=`; missing arity$2 property. Fixed by
  matching the existing multi-arity shape, preserving its tested mutation.
* `third-authoring/`: the nth tracer used vector conj, which reentered nth.
* `fourth-authoring/`: a numeric tracer still reentered nth through the captured
  original dispatcher, whose generated body follows the live arity$2 property.
  The final vector-only tracing proxy delegates to protocol -nth directly,
  records both index0 reads as trace11, and restores the original nth in finally.

Those are authoring/reference setup failures, not upstream bugs, native failures
or passes. Initial corpora and drivers are retained without rewriting them.
No case was dropped, skipped or converted to a failure-as-success expectation.
`python-suite.log` contains actual Python harness execution. Synthetic valid JSON
inside harness tests is not CLJS or native execution evidence. Every evidence
leaf except hash receipts has a SHA256/byte-size entry in `files.json`.
