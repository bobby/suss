# Checked Java-pattern source

`pattern.rs` is original Rust syntax-admission code. It preserves UTF-16 source
and the Java flags of a successfully checked pattern. The compiler reader uses
this data for source Pattern literals and their compiled Runtime and Macro
Pattern objects. This module performs syntax admission; matching APIs and
runtime regex reading remain separate.

Behavior comes from pinned Clojure 1.12.6 development observations in
`results/m12-pattern-values`, with the Java 21
[Pattern syntax specification](https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/util/regex/Pattern.html)
as the grammar reference. No Java implementation source is copied or shipped.
The 76 original and 116 additional adversarial observations cover admission,
exact UTF-16 source and flags. Error text is not claimed byte-for-byte equal to
Java's platform-specific exception messages.

Unicode character-name/codepoint, script, block and property-name facts are
observed from OpenJDK 21.0.13+9-LTS (Unicode 15.0.0) and pinned with source/output
hashes. The development oracle uses public Character methods and enumerates
script/block alias registries; its reflective access is development only.
`unicode-names.bin` front-codes every one of 288,767 observed name/codepoint
pairs in blocks of 32. It is an exact lookup table, not a probabilistic filter.
Names retain their codepoints for character-class range checking. The generator
independently decodes the entire table and compares it with the raw observations.
`properties.rs` contains the observed property-name registries and casing rules.
The associated [Unicode notice](UNICODE-LICENSE.md) is retained verbatim.

Regenerate the data from its retained compressed observations with:

```sh
python3 results/m12-pattern-values/unicode-primary/build-data.py
```

The raw observations and development-only Java command are recorded in the
adjacent evidence. The Rust implementation needs neither Java nor an external
Unicode service. The parser grows its stack and bounds group/class nesting at
1024, reporting budget exhaustion rather than overflowing the host stack.
