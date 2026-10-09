# Reactivated original compiled support tests

Based on 677340c. The PR231 integration2 failure is retained: the obsolete
unsupported comp diagnostic failed after full comp was imported. Exactly seven
original tests are reactivated with unchanged sources and result assertions:
comp/debug-comp (12), partial (15), fn keys (30), defn keys (10), fn map alias
(12), multi-arity map (20). Their corresponding unsupported rows alone are
removed; all 30 other diagnostic rows and ignored tests remain unchanged.

A closed eleven-case raw fixture independently confirms each original expression
(embedded in do only for the pinned runner), plus ordered composition/partial
effects and captures, eager parameter defaults and zero/map arities. Fresh pinned
c4295f303100bbf5afac449242d30bca1126f1a1 compilation/Node execution exited zero:
all eleven tagged binary64/ordered-vector observations match. Sources remain
compiled core forms, with no production adaptation changes. A new executing host
test consumes all eleven cases, performs GC and independently decodes exact
values/effects through the existing observation harness and existing fuel budget.
Native execution is pending the coordinated parent lane. This is not evidence of
let/loop destructuring, lazy-seq, range, missing macros or M4 completion.

Run `sh scripts/test-compiled-support-reactivation-oracle.sh`; all raw evidence
is in evidence/. Freeze receipt records hashes without inferring pending gates.

Author checks: 272 Python tests passed (22.486s); core-import --check verifies
360 files; Rustfmt parsed the full Rust test without rewriting old formatting;
diff check passed. Updating comp/partial review references requires regenerating
the import manifest's review hash/semantic-test metadata. The generated core
source and all extracted whole forms are unchanged. Four committed bootstrap
artifacts are unchanged and stale against this manifest; the parent must
regenerate before frozen native validation. Initial verification rejected the
external submodule symlink (escaping repository); corrected with a local exact-pin
clone. A later check correctly rejected stale review metadata, then regeneration
and check passed. Neither failure is a passing gate.
