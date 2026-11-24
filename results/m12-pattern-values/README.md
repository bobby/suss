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
