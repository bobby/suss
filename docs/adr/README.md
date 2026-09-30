# Architecture decision proposals

Proposals do not change the [accepted design](../design/suss-0.3.1.md) or certify
implementation. Acceptance requires a dated design decision and aligned roadmap,
compatibility contracts, and executing evidence.

| ID | Proposal | Status |
| --- | --- | --- |
| ADR-0001 | [Nominal results and options, explicit absence, and panic](0001-result-option-and-panic.md) | Proposed |

## ADR-0001 work tracking

[Dedicated milestone](https://github.com/bobby/suss/milestone/11).
[Machine-readable links and dependencies](0001-tracking.json) use separate
`ADR1-*` IDs; existing M0–M9 work-package IDs and acceptance states are unchanged.
Creating these issues does not accept either proposal stage. Implementation is
gated by the relevant decisions and design, and native migration additionally
requires explicit strategic acceptance.

| ID | Work package | Dependencies |
| --- | --- | --- |
| ADR1-01 | [Decide tactical and strategic adoption](https://github.com/bobby/suss/issues/50) | None |
| ADR1-02 | [Specify types, propagation, and migration](https://github.com/bobby/suss/issues/51) | ADR1-01 |
| ADR1-03 | [Implement nominal types and error inspection](https://github.com/bobby/suss/issues/52) | ADR1-02 |
| ADR1-04 | [Adopt nominal WIT types and compatibility adapters](https://github.com/bobby/suss/issues/53) | ADR1-03 |
| ADR1-05 | [Implement matching and lexical propagation](https://github.com/bobby/suss/issues/54) | ADR1-03 |
| ADR1-06 | [Implement panic, cleanup, and async supervision](https://github.com/bobby/suss/issues/55) | ADR1-02, ADR1-03 |
| ADR1-07 | [Migrate native core from nil and throw/catch](https://github.com/bobby/suss/issues/56) | ADR1-04, ADR1-05, ADR1-06 |
| ADR1-08 | [Audit adoption and publish migration evidence](https://github.com/bobby/suss/issues/57) | ADR1-04, ADR1-05, ADR1-06, ADR1-07 |
