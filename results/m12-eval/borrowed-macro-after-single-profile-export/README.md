The original recorder returns 0 with unchanged inputs. The old research probe
re-entered through the observation hook, whose documented contract forbids
re-entry. That fixture is unsuitable as evaluation-capability validation and
is replaced by an explicit typed evaluation probe in the next source commit.
These original logs are preserved, not used to claim public eval acceptance.
The same gate also executes the original macro and nine inline-module checks.
