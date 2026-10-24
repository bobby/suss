# Reader task layout correction

CI39269589242 on d512ad15ce977f0b788aee3af3172e1437ff682e failed foundation
job118118246282 with Rust E0080: the new ReaderMap task's inline borrowed Form
plus origin slice enlarged the native task enum, violating its explicit old-layout
assertion. No bootstrap generator ran in that failed job. This actual compiler
failure supersedes the earlier parsing-only source checks; no prior native pass
is attributed to the reader wiring.

ReaderMap now holds one private Box of its borrowed Form/origin-slice payload.
Before allocation, the Session pays twice the actual payload size plus two native
words and six controls for allocation, transfer/read and retirement. Only this
metadata root task allocates the payload; normal task routes and visitation order
are unchanged. The old queue-slot layout assertion remains, so native compilation
must still verify the actual integrated representation on its target.

A standalone std-only Rust ABI model using the same pointer/slice/enum payloads
reports old24, inline32, boxed24 and payload24 bytes on the local target. That
model executes no Suss code and is not native compiler/typechecking, bootstrap,
macro or conformance evidence. Parsing of the changed Rust path also passes.
Final-head CI and actual focused tests remain required. No fuel rate/default,
Runtime allowance, assertion or source program was reduced.

Bounded independent source review verified the exact correction, prepayments,
lifetimes, traversal and retained assertion, returning GO SOURCE-ONLY. This does
not substitute for actual compilation and execution on the integrated head.
