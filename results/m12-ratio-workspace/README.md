# Ratio workspace execution and corpus loader repair

The required unfiltered command at 5ee8a0d7 exits101 after606.88 seconds.
All frozen tracked inputs are unchanged. The first integration target reports
8 pass and1 failure: analyzer_edn::conformance_corpus_round_trips uses the
native EDN reader to load a corpus whose corrected expected reciprocal is1/2.
Its numeric model rejects that expectation before extracting any expressions.
The workspace run stops there; later targets are unexecuted by this invocation.

The repair reads the fixture through the forms-to-tooling boundary already
used by the analyzer. It still analyzes every unskipped expression and setup,
and preserves the at-least200-case assertion and complete round-trip check.
No numeric expected value or skip is changed. The native tooling EDN numeric
gap remains; this repair does not claim to implement its exact number model.
The complete analyzer_edn target then passes9 tests (55.58 seconds).

command.json/run.log retain the original terminal workspace failure.
analyzer-loader-repair-command.json/log retain the later focused result.
run.py can be copied to a new results directory for a separate frozen retry.
