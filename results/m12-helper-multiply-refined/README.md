# Intermediate GC-row refinement

The cached-row GC kernel passes its2 storage/ownership tests and completes the
250-observation/400-instruction/20-allocation wrapper probe. The near-boundary
probe adds192 perf samples with both operand orders. These data inform the
later weighted-area dispatch in results/m12-helper-multiply-final.

The scalar numeric fallback shortcut passes7 equality controls but leaves the
record membership fuel exactly992401/1003928 and fails its unchanged gate.
It is removed from production source. refinement-before-policy.patch restores
this exact intermediate experiment from143062ce, including its notice/manifest
state, so the frozen input hashes remain reproducible. Later sort/dispatch
results must be read from the final directory, not inferred from this attempt.
