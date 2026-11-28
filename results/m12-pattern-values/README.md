# Pattern source prerequisites for complete numeric bodies

The unchanged eq/not-eq bodies use real regex literals, and num rejects a pattern
as a nonnumeric value. Their original source is retained; no assertion is sliced,
skipped or replaced here. `value-primary/` records26 fresh Clojure1.12.6 controls
showing real identity, stable function/loop literal sites, eval constant behavior,
printing, hashing and errors. These observations do not establish runtime support.

The new reader `PatternSource` module validates Java regex syntax and retains exact
UTF-16 source/flags. It has no matching engine and no Java dependency. Syntax facts
come from the Java21 specification and pinned development observations, not a port
of JDK implementation code. Unicode15.0 name/codepoint and property registries are
retained with their notice and reproducible exact front-coded table. See
`crates/suss-reader/src/pattern/SOURCE.md`.

`admission-primary/` retains76 actual JVM cases. The new116 cases under
`adversarial-primary/` challenge empty quotes/flags, class ranges, escaped
supplementary characters, Unicode names and property spellings. The first native
pass admits the original76. Adding the independent116 controls returns actual101,
revealing19 discrepancies. Its exact source is retained in
`adversarial-before-source.tar.gz`; original expectations stay unchanged. Follow-up
fixes preserve empty-quote boundaries, flag-group quantifier rules, repeated-minus
errors, empty/intersected classes, category-versus-scalar ranges, quoted range
endpoints, surrogate pairs, named codepoints and case-insensitive property keys.

The full reader command `cargo test -p suss-reader --locked --offline --
--test-threads=2` returns actual0 in36.38s for all90 tests, including all192 admission
cases and the existing Decimal controls. After private checked-source accessors
and formatting, both complete admission targets return actual0 in13.90s, with
source, primary and HEAD unchanged. Receipts distinguish those exact scopes.
No primary values, original suite count/skip rules, dependencies or accepted
failure map change.

This is a syntax foundation. Source-form integration, canonical macro transfer,
real immutable compiled Pattern objects with correctly retained literal sites,
Runtime/Macro/AOT/GC/ownership controls, and execution of the complete numeric
source bodies still require implementation and validation. Matching APIs are a
separate compatibility surface. This evidence proves neither compiled Pattern
values nor completion of the numeric acceptance gates.


`macro-identity-primary/` adds eight research observations about compilation
contexts. A retained macro pattern used directly in two bindings shares identity,
while the same input object repeated in an eval vector or quoted vector is
reconstructed into distinct pattern objects. A compiled literal site remains
stable across closures, while separate literal sites are distinct. These actual
observations rule out pooling by pattern text and require care with constant
collection contexts; they are not native Pattern pass claims. Compiler-data
identity remains subject to the accepted design's guarantees.


## Checked source forms and actual compiled literal values (2026-11-24)

The reader now admits regex tokens through PatternSource, preserving raw UTF-16
source/backslashes and checking invalid patterns even in discarded or inactive
branches. Twenty fresh pinned reader controls include quotes, supplementary
characters, multiline source, metadata rejection and invalid tokens. The initial
exact regression returns101 for the missing dispatch. The first integration run
exposes an omitted conditional-selection step in the test adapter; a subsequent
compile-only lifetime error is retained too. All92 reader tests then return0.

PatternLiteral clones retain a read site's identity; distinct reads differ.
Source emission uses raw regex tokens where representable and a validated string
carrier otherwise. A real collection control exposes equal-text Pattern keys
collapsing in tooling (stable-toolchain build0, actual101; the first non-stable
compiler attempt fails to build and is not an execution failure). The exact old
reader source is reconstructed from the retained transformation and verified
against every recorded failure input hash before archiving. Per-projection
first-traversal site indices now retain distinct and shared objects without
addresses. The complete reader command returns0 for93 tests in43.28s, including
all192 syntax cases and the new collection/source/span round trips.

The compiled pipeline constructs genuine sealed immutable Pattern instances with
source and flags, identity equality/hash and ordinary Object/printing protocols.
The HIR hoists each literal into a fresh deterministic private rooted cell for
that compilation. Original core links protect construction against redefinition.
Authenticated bootstrap-class macro transfer validates exact source/flag fields;
direct eval retains the original actual Pattern root. The Unicode binary table
is now part of both compiler identities, so changes invalidate artifacts.

The generator check returns0. A first native run returns0 for18 literal/value
controls in each of Runtime and Macro, including post-GC observations. The
expanded command initially fails to compile two new test borrows; those receipts
are retained. After fixing them, the unchanged-source expanded command returns0
in503.18s for34 complete-target tests: analyzer18, sealed construction5, sealed
internals7 and Pattern4. The Pattern target executes the18 primary value controls
and all6 collection controls in both phases, independently inspects canonical
nominal storage/source/Long flags after GC and real Macro transfer, rejects
private construction/field access, and executes21 pure primary controls in a real
AOT component twice. The first native target's case repeats within this expanded
run; it is not an additional unique test.

Commands, terminal exits, frozen HEAD/input hashes and logs are retained as
compressed receipts. Reader and compiled runs use isolated targets, never the
active compiled-eval target. No matching API, runtime regex reader, Java Pattern
interop, full numeric source-body, complete workspace, review or final-head CI
pass is inferred. The unchanged original suite/primary/skip/count/opaque rules and
reviewed baseline remain intact at this checkpoint. See
[original class provenance](../../docs/compatibility/bootstrap/pattern-literals.json).
