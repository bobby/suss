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
| `0001-when-var-exists-macro-phase.patch` | ClojureScript oracle | Guard `when-var-exists` with `#?(:clj ...)` so stock ClojureScript does not analyze the macro body as runtime code (upstream CI uses shadow-cljs, which tolerates it). |

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
