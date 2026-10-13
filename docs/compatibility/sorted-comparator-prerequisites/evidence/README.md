# Sorted prerequisite reference evidence

`current-primary/` retains a fresh pinned compile/Node execution of all 47 exact
ordered Boolean checks, terminal exit 0. Its shared corpus, driver, protocol
probe, actual log/raw observations and command receipt are packaged unchanged.
Private-binding compiler warnings are retained. This is development reference
execution; no Suss/native result is supplied.

`initial-authoring/` retains the initial 40-case corpus/driver, raw observations
and failed strict comparison (exit 1). The live-deref test also read its counter
with the rebound deref, so its own assertion was false. The corrected assertion
uses the saved deref to observe that counter; the expected true result remains.

`second-authoring/` retains the 42-case corpus/driver and failed Node execution
(exit 1, original thrown 73). Two additional probes incorrectly expected public
-count/-nth rebinding to redirect upstream ci-reduce. The pin's not-native
argument emits direct protocol method calls. Final probes instead verify those
public dispatcher replacements are not entered, and the adaptation retains
canonical dispatchers while preserving dynamic method lookup. The intended
values/effects are explicit; no unsupported failure is accepted as success.

`python-suite.log` records 246 Python tests passing in 7.049s. Five focused new
harness tests pass. These tests validate corpus closure and fail-closed checking,
not native behavior. `files.json` hashes every evidence leaf except itself.
Bootstrap regeneration, native compilation/execution and full/final-head gates
remain pending.
