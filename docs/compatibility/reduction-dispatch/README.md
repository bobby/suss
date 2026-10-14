# Public reduction dispatch evidence

This package records the #245 public `reduce`/`reduce-kv` contract. The original seven expressions and their fixture remain pinned; three additional expressions capture public-call operand evaluation order. The corpus keeps the pinned ClojureScript raw values separate from host-side validation rules.

Run the lightweight evidence and adversarial checks with:

```sh
python3 scripts/reduction_dispatch_oracle.py --check-references
python3 scripts/test_reduction_dispatch_oracle.py
```

The native acceptance integration test is:

```sh
cargo test -p suss-cli --test portable_reduction_dispatch --locked -- --nocapture --test-threads=2
```

On 2026-10-14, the focused working-tree run also included the record factory and label preservation tests. It completed successfully: reduction dispatch 4/4, 20/20 case/phase matches (seven original plus three source-order cases in Runtime and Macro), and 16 canonical descriptor checks; factory preservation 1/1; label preservation 4/4. The exact command, source hashes, elapsed times, and output/capture hashes are in [focused-native-receipt.json](evidence/focused-native-receipt.json). The raw ten-case native result is [native-acceptance.json](evidence/native-acceptance.json).

The original seven-case raw native capture is retained separately at [native-original-seven.json](evidence/native-original-seven.json) and SHA-pinned by the validator. It is historical evidence from the candidate run; it does not replace the current ten-case capture. Pinned ClojureScript raw captures and source-order execution details are retained alongside them. The source-order supplement is no longer pending: both native phases passed in the focused run.

The focused receipt preserves its original working-tree scope. The subsequent full locked workspace baseline exited 0 on committed head `caac7d61`: 1,796 passed, zero failed, and 22 explicit ignores across 253 completed suites including doctests. The rebase onto merged main retained the identical tested source tree. Commands, exact identities, log hash, and unchanged Jank capture are recorded in [validation-41231.json](evidence/validation-41231.json). Independent code review found no actionable findings. Final-head CI remains pending; the Jank harness is failure-baseline evidence, not whole-suite conformance.
