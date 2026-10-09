# Independent frozen publication-cell kernel review

Candidate51533b4d547c5212ee85a034dd57e1dbbb34f122 against491703306cc439404dd3f507d4d8d5eee587bf25 in /private/tmp/suss-m4-record-reify-implementation. Committed objects only; dirty source dependencies ignored. Reviewer ran no Cargo/bootstrap and made no candidate edits.

## Material finding

P2 — Check a hidden reader reservation before declaring a fresh publication cell. crates/suss-compile/src/portable/hir.rs:1866–1873.

Environment::resolve deliberately filters ReaderCell/InternalCell. When a probe addresses a hidden ReaderCell in the actual current namespace, resolution fails and the new fallback calls declare_cell before is_hidden_cell. declare_cell rejects InternalCell but deliberately promotes ReaderCell to Cell (resolve.rs:703–708). The subsequent hidden check therefore sees a public Cell and accepts the probe. This also makes the previously hidden reader-support identity visible to captured source namespace identities, despite no source declaration occurring. The existing syntax_quote/tests.rs private_reader_cell_is_absent_from_source_namespace_identities_until_promoted demonstrates this intentional distinction: promotion is meant for a genuine source declaration, not an existence query.

Concrete path: an Environment in current namespace suss.core with reader_sequence_cells already reserving hidden suss.core/sequence, then analyze (suss.compiler/cell-defined? suss.core/sequence). The source query silently promotes that reservation. Such a reservation is created by the genuine reader sequence initializer path in hir.rs:907–908; it need not be a fabricated foreign namespace. This violates the README's compiler-owned-cell rejection and the new hidden-cell guard's intended boundary.

Before the fresh current-namespace fallback mutates the Environment, reject is_hidden_cell for that canonical current namespace/name. Preserve legitimate declaration promotion elsewhere. Add a focused compiler/native regression that starts with an actual hidden reader reservation, checks probe rejection and retained hidden catalog state, then confirms an explicit source declaration can still promote it. Internal protocol/reader fallback reservations must remain rejected too. This is a source-boundary repair; do not bypass the separate destructuring/bootstrap blocker.

## Otherwise audited

The runtime function correctly consumes dynamic-find's two results in reverse stack order (index local2, entries local1), checks selected dynamic value first, and uses binding-bound before reading an unbound root. Exact undefined sentinel6 alone counts absent; nil0, false2 and zero Number/object values remain present. A dynamic override can supply nil for an unbound root. It compares raw identity, with no coercion, callbacks, property traversal or replay. Dynamic frame guards and existing physical ABI layouts remain unchanged.

HIR requires one explicitly qualified syntactic symbol, resolves the caller phase, emits GlobalCell rather than an eager GlobalRead, and rejects ordinary bootstrap bindings/evaluated operands. Emission imports the VALUE->i32 signature, invokes the private function and boxes its i32 result to Boolean sentinel2/4. Function locals and Wasm control/result stack are consistent statically. New tests compiled according to the parent's later focused run; source-level API inspection identified no additional compile API issue. Both tests retain failure/recovery checks and independent FormBridge Boolean/zero observations after Runtime GC; they do not certify public exists?, dotted paths, anonymous reify or records.

Verified all10 hashes actually listed in the frozen receipt (8 files,2 retained prior reviews) against committed bytes, plus all9 exact ordered retained reference outcomes false,true,true,true,false,true,false,true,0. The delta has13 changed files; the receipt does not list13 hashes. This counting distinction does not invalidate the matching recorded hashes. Source fixture uses genuine pinned exists? and raw counter; initial misplaced-fixture failure is retained. No independently rerun pinned compilation by this review. git diff --check491703351533b4 passed. Prior source helper tests/corpora are unchanged.

## Execution status

Frozen documentation records authored tests UNCOMPILED/UNEXECUTED at commit time. Subsequent parent report: the focused test target compiled, but both tests failed Session initialization with stale bootstrap manifest identity; no kernel execution occurred. Parent regeneration then exited1: Parameter destructuring is not lowered yet, span177827..177835. Reviewed /private/tmp/suss-m4-publication-cells-51533-bootstrap.log confirms that failure. Hilbert owns the complete genuine lowering response. No source subset, manifest bypass, native pass or bootstrap completion is accepted or inferred. This independent authored-source review remains separate from eventual native validation. Parent owns the focused native lane.
