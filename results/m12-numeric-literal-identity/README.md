# Exact-number literal lifetime controls

The full34-control primary recording is retained unchanged under primary. It
covers source boxing, NaN and primitive identity as well as exact-number literal
sites. It is research evidence; no native pass for all34 is claimed.

The focused regression selects all8 explicit BigInt/Ratio/Decimal site controls
(4 function sites and4 distinct-read pairs), executes them in both Runtime and
Macro, and separately compares actual returned references across calls, source
fragments and GC. The exact before command returns101 in192.74s with source/HEAD
unchanged. All4 function-site controls fail in each phase, and the independent
retained-root check fails for1N. Distinct exact literal pairs correctly differ.
The missing behavior is retained literal lifetime, not numeric value equality.

Do not globally intern numeric source text or change persistent numeric fields.
Keep object-site identity separate from Clojure's source-dependent primitive
boxing/NaN rules. Original eq/not-eq strict142-body validation remains failing on
3 NaN cases; primitive identity and complete numeric acceptance require their own
checks. No source baseline, original suite or primary rule is changed here.
