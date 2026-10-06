# Suss roadmap

The accepted target is [the design specification](docs/design/suss-0.3.1.md).
Work is dependency ordered and is not a calendar promise.

**Status lives only in GitHub.** Open/closed [milestones](https://github.com/bobby/suss/milestones)
and [issues](https://github.com/bobby/suss/issues) are the single source of
completion state; progress and evidence are recorded as issue comments and linked
PRs. This file holds the plan, not status; do not add per-package progress here.

[ADR-0001](docs/adr/0001-result-option-and-panic.md) proposes separate tactical and
strategic adoption of Result/Option/panic semantics. Its
[dedicated milestone and work packages](docs/adr/README.md#adr-0001-work-tracking)
track decisions, design, implementation, and acceptance separately from M0–M9.
The proposal has not changed this roadmap's accepted semantics or release gates.

| Milestone | Depends on | Exit gate |
| --- | --- | --- |
| [M0: Contract and feasibility](https://github.com/bobby/suss/milestone/1) | — | All M0 acceptance criteria pass |
| [M1: Trustworthy evidence](https://github.com/bobby/suss/milestone/2) | M0 | All M1 acceptance criteria pass |
| [M2: Compiler and runtime foundation](https://github.com/bobby/suss/milestone/3) | M0, M1 | All M2 acceptance criteria pass |
| [M3: Persistent development environment](https://github.com/bobby/suss/milestone/4) | M2 | All M3 acceptance criteria pass |
| [M4: Portable persistent collections](https://github.com/bobby/suss/milestone/5) | M2 | All M4 acceptance criteria pass |
| [M5: Generic WIT interoperability](https://github.com/bobby/suss/milestone/6) | M2, M4 | All M5 acceptance criteria pass |
| [M6: WASI 0.3.1 alpha](https://github.com/bobby/suss/milestone/7) | M3, M4, M5 | All M6 acceptance criteria pass |
| [M7: Portable compatibility beta](https://github.com/bobby/suss/milestone/8) | M6 | All M7 acceptance criteria pass |
| [M8: Browser beta](https://github.com/bobby/suss/milestone/9) | M6 | All M8 acceptance criteria pass |
| [M9: CSP extension](https://github.com/bobby/suss/milestone/10) | M6, M7 | All M9 acceptance criteria pass |

M4 and M5 can overlap after their foundations; M8 feasibility happens in M0,
with browser product delivery after the WASI alpha. CSP is deliberately last.

## Work packages

Each work package is one GitHub issue in its milestone. Its title begins with a
stable ID such as `[M3-05]`; never renumber or reuse an ID. The issue body holds
the objective, specification sections, dependencies, acceptance criteria, scope
and proposed validation command. Commands for planned suites are **acceptance
targets**, not existing runnable scripts. Create new packages with the
[work package issue form](.github/ISSUE_TEMPLATE/work-package.yml) and assign the
milestone.

Status and evidence recorded in this repository before 2026-10-06 were moved
verbatim into a `suss-roadmap-archive` comment on each affected issue.
Criterion-by-criterion acceptance audits remain under [docs/roadmap](docs/roadmap).

## Completion discipline

Every implementation session adds a failing regression, makes the smallest coherent
change, executes its relevant checks, and records evidence on the relevant issue.
A package is complete only when its acceptance criteria pass; close the issue
(and its milestone, once every issue is closed) only then. A known-failure
baseline does not certify compatibility. Toolchain limitations remain explicit
blockers, not reasons to silently weaken the contract.
