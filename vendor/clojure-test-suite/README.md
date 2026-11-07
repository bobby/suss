# jank-lang/clojure-test-suite source lock

`upstream/` is the unmodified content of
[jank-lang/clojure-test-suite](https://github.com/jank-lang/clojure-test-suite)
at commit `95d4a9112cfc5fe6629be3d14bb99080ea00af2f` (2026-08-18): its `test/`
tree, `README.md` and `LICENSE`. The files are licensed under the
[Mozilla Public License 2.0](upstream/LICENSE); they are test inputs, are not
shipped in any Suss artifact, and are not relabeled as Suss MIT/Apache code.
[The source lock](../../docs/compatibility/clojure-test-suite-lock.json) records
the SHA-256 of every vendored file.

Vendored bytes are never edited. Local adaptations are separate MPL-2.0 patch
files in `patches/`, applied only to generated copies:

| Patch | Applied to | Purpose |
| --- | --- | --- |
| `0003-clojure-numeric-operand-suss-branches.patch` | Suss harness | `:suss` branches for Clojure's non-number operand assertions where the suite writes them as `:clj` or not at all (#286). |
| `0004-atom-clojure-references-suss-branches.patch` | Suss harness | `:suss` branches for Clojure's atom option assertions (#30). |
| `0005-hierarchy-type-tags-suss-branches.patch` | Suss harness | `:suss` branches naming a core type where the hierarchy tests name a host class (#30). |

The JVM Clojure oracle (#334) runs the suite unpatched.

```sh
python3 scripts/clojure_test_suite.py                 # verify the lock offline
python3 scripts/clojure_test_suite.py --git /path/to/clojure-test-suite
```

The conformance harness, oracle and baseline are described in
[docs/compatibility/clojure-test-suite.md](../../docs/compatibility/clojure-test-suite.md).
Updating the pin: replace `upstream/` with the new commit's files, update
`COMMIT` in `scripts/clojure_test_suite.py`, run
`python3 scripts/clojure_test_suite.py lock --write`, confirm the patches still
apply, regenerate the oracle reference and review every changed observation.
