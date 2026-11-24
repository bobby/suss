# Execution-time randomness

Retained Clojure1.12.6 forms, exact adaptations/patches and EPL provenance accompany
126 deterministic multiply/int transformations. The42 separately retained real
JVM calls and16-draw stream are nondeterministic observations, not native goldens.
The typed callback/WASI provider probe is feasibility evidence only.

The production runtime stays import-free and roots its nullable entropy callback
in an appended typed global outside the shared ABI prelude. Native sessions use
operating-system entropy. AOT accepts only an explicitly selected standard random
interface, encoded from the resolved WIT graph with the shared generic encoder;
the lowered provider is installed before source initialization. No provider means
a catchable language error, without a seed, constant or hidden world import.
Both ordinary and optimized AOT assembly preserve draw effects.

The public forms retain numeric multiplication then checked Object int-cast.
Lexical checked function values avoid core's historical coercing arithmetic
macros. Native tests require126 primary transformations per Runtime/Macro phase,
exact provider consumption, GC retention and independent host observation.
Boundary tests execute real ordinary/optimized AOT artifacts and controlled words.
These are requirements, not pass claims before new recorded executions.

Run the recorder itself inside a bounded systemd scope, with an isolated owned
SUSS_RANDOM_TARGET and disk-backed TMPDIR. run-focused.py records the frozen
input hashes, exact native commands, exit codes and terminal input recheck.
Complete original constantly/some-fn bodies, full workspace, bootstrap and actual
CLI/WASI public API coverage remain separate acceptance gates.

At97a2d235, the raw production entropy target returns0. Ordinary selected-WIT
and pure missing-provider AOT cases pass; the optimized case fails because
wasm-opt was not configured, preserving overall101 and unchanged inputs. The
public-core command was not reached. The retained Binaryen133 tools match their
original official archive/tool provenance. Fresh focused runs select the tool
explicitly and run the optional optimization test with --include-ignored. No
optimization pass is inferred from default CI's named ignore.

Public-effects retains16 fresh actual-API primary properties. The new public
target adds126 controlled transforms through one real compiled AOT component
and actual CLI/WASI streams with fresh-instance checks. Provider failure,
replacement recovery and unused draw effects have explicit boundary tests.
These new requirements need separate native receipts. run-public.py uses the
workspace feature graph and captures both complete original bodies.

The1ded898d fixed-draw comparison exposes a fixture overload mismatch at the
upper int fraction. Actual rand-int bytecode calls RT.intCast(Object), whereas
the original literal-only int expression selects the primitive overload. The
separate object-transformations-boxed corpus retains126 fresh Object controls
and a generated JVM bytecode proof with the same multiply(Object,Object) and
intCast(Object) signatures. Exactly one expected value differs:2147483647
rather than an error. Original literals/receipts remain unchanged; the initial
hint-only probe failure is also retained. Public tests select the Object corpus;
production numeric code is unchanged. Fresh native comparison remains required.

At be02889d, both public-core tests pass, including126 controlled Object
transformations in each native phase with direct raw numeric field decoding,
exact draw counts and normal provider samples (720.259s, inputs unchanged).
The next target passes Runtime/Macro and actual CLI/WASI properties but fails
before controlled AOT execution because result is a reserved WIT keyword. The
original overall101 is retained.

At1abb5843 the corrected sample-report export, component post-return lifecycle
and fresh instance per deliberately trapping scalar row pass all five public
tests (184.155s, inputs unchanged). Controlled AOT independently checks all126
reports and raw f64/s64 results, preserving exact provider consumption. The
complete original body target then passes (20.750s): all16 some-fn assertions
match on the host; all31 constantly assertions execute with its Ratio as a
strict host match and its two original guest judgments retained. No original
body, primary, count, skip or budget changed. Complete workspace/capture and
bootstrap acceptance remain separate.

At201a7bcf, the complete actual capture records5,435 rows and preserves all
prior comparable successes:5,238 host matches,93 explicit guest judgments,
105 retained failures,1,503 unexecuted,zero skips and66 failed namespaces.
The three newly loaded original bodies contribute78 rows:some-fn16 host;
constantly29 host/two guest;lazy-seq21 host/ten guest. The Ratio row remains a
strict host match. BigInt/Ratio has227/229 matches and Decimal199/200; the
remaining2+1 rows are all unresolved eval, not new numeric successes.

The native capture retains its original101 discrepancy against the preceding
baseline (664.706s). Its independent unchanged-primary/count/skip audit returns0.
Seven adversarial guard checks pass; review-random-baseline.py removes only
the three proven namespace-load failures, preserving every other baseline
entry and explicit guest judgment. The subsequent host comparison returns0;
this is not a native rerun or a full workspace pass. The old baseline, original
receipt/log, proposed result and reviewed write are retained separately.

Java/Node-free double bootstrap reproduction, checkout-independent identity
verification and all four original bootstrap execution tests return0 at the
same frozen201a7bcf (452.479s, inputs unchanged). The enclosing durable capture/
audit/bootstrap recorder returns0 with unchanged inputs. Full workspace and
independent final-head review/CI remain separate requirements.


## Frozen randomness workspace and optimizer validation (2026-11-24)

At61397661, `cargo test --workspace --locked -- --test-threads=2` returns0
after11,089.347s. The durable recorder observes unchanged tracked inputs and
HEAD. Archive-only replay verifies all375 expected native/doc targets:
2,496 passed,15 explicit ignores,zero failed andzero filtered. This reruns
the shipped some-fn, assert, public randomness and complete original body
guards along with the original numeric, record, artifact, import and size
suites. Full receipts, logs, metadata, fingerprints and replay are under
`results/m12-random/full-workspace-after-reviewed-random-baseline/completion-audit`.

The ignored `optimized_selected_random_import_preserves_draws` is separately
executed with the hash-verified official Binaryen133 tools:1pass,0failed
(1.631s including Cargo), unchanged inputs/HEAD/tools. Its receipt and log
are under `results/m12-random/optimized-after-final-baseline`. Other ignores
retain their original unsupported/manual/doc qualifications.

Reconciling the rebased min-key parent8e05f09c preserves every pre-existing
implementation, test, fixture, manifest and evidence blob exactly; only
handoff prose changes and historical parent completion archives are added.
The qualification records this comparison before the new completion notes
and archives. Real eval and the remaining numeric umbrella requirements
remain separate work; these command results establish no eval acceptance.
