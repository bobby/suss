# Explicit native allowance for the fixed bootstrap catalog

CI39274756808 on cd31fef746ab1b63b4ac9b95102e2fa9eb99e99b compiled the
production compiler, including the boxed reader task and retained-key extension,
without Rust E-code failures. Its first normal suss-bootstrap invocation failed
at source bytes223584..223588 with `Macro expansion native work fuel exhausted`.
It produced no complete first tuple or second reproduction. This is an actual
native-analysis failure, distinct from previous compilation/identity mismatches.

The private catalog(phase) path used generic NoExpansion's10M fallback while
analyzing the fixed embedded SOURCE. It now uses a private no-expansion host with
an explicit100M native analysis allowance. Both development generation and checked
shipped-bootstrap fact restoration use that same catalog; the latter must also
be able to reconstruct the complete compiler facts. No caller text/forms/env/host
are accepted by this private path. Macro expansion stays absent and dependencies
stay empty, with the original cumulative ExpansionRequest accounting intact.

Generic NoExpansion and DEFAULT_OPERATION_FUEL remain10M. Caller compilation
options, Runtime/Macro Session fuel, deliberate zero/default/exhaustion controls,
validation gates and originally-nil authority are unchanged. This is not a Session
budget refill or a new end-to-end generation/execution cap: parsing, lowering,
emission and restoration checks keep their prior policy. It calibrates native
analysis of the known bootstrap catalog rather than suppressing work charges.

Independent source review verifies this boundary and returns GO SOURCE-ONLY.
Patch SHA-256:
`97ca61ecd6b1f670d575e6188da3365c258bf6695567ace58cd112f70bd771d1`.
Rust parsing passes.100M sufficiency remains unproven until normal generation;
no performance gain, native pass or completed acceptance is claimed. Existing
compiled_bootstrap tests already exercise full generation/restoration, phase and
identity rejection, separate Stores and real macro execution. They remain intact.
The normal verifier and its new diagnostic retention must run on this source,
then artifacts must be source/identity-verified and actually executed.
