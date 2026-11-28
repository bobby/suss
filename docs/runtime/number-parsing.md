# Exact decimal text parsing

`parse-long` accepts a string of decimal digits with an optional ASCII `+` or
`-` sign and returns an exact signed 64-bit long. It is an ordinary first-class
core function in Runtime and Macro, including the usual qualified core aliases.

```clojure
(parse-long "9007199254740993") ; => 9007199254740993
(parse-long "-9223372036854775808") ; => -9223372036854775808
(parse-long "１７") ; => 17
(parse-long "9223372036854775808") ; => nil
(parse-long "bad") ; => nil
```

Empty strings, whitespace, bare signs, malformed digits and overflow return nil.
Non-string values raise a language exception without coercion. Decimal digits
follow the pinned JVM's UTF-16 `Character.digit(char,10)` domain, including the
37 BMP digit families. Supplementary digits and unpaired surrogates do not parse.
The accepted platform represents character values as strings; a one-unit source
character therefore has the same boundary value as that string.

Core `parse-long` returns nil where JVM `Long/parseLong` throws. A form-input
adapter should preserve nil/empty input separately, and raise an explicit error
for invalid nonempty text before persistence. The executing component control
uses `ex-info`, increments its change counter before parsing, catches its
structured parse error and returns a typed WIT result. It preserves exact IDs,
including values above 2^53, through an `option<s64>` boundary.

The native implementation uses checked integer magnitude accumulation and the
already tested UTF-16 digit primitive. It does not pass through binary64 or
change the reader's separate radix/literal parser. The core import recipe,
review overlay and generated EPL provenance identify the adapted pinned public
form. Clojure 1.12.6 supplies the semantic oracle. No Java dependency is shipped.

`clojure_parse_long` executes the 783-case recorded JVM corpus in both phases and
in an actual component, plus non-string/alias/GC controls, explicit character-
as-string platform controls and 10,000 varied form calls. Canonical transfers
remain within one 64 KiB page; numeric/managed memory has a separate bounded
allowance. A second component uses a real typed storage callback: malformed or
overflowing strings cause no write; exact IDs reach storage intact; the change
counter advances before parse and typed host failures. These are bounded form
controls, separate from the complete UserManager application and persistence.

The vendored parse-long namespace now executes. Its character-input assertion
#28 remains an explicit JVM mismatch under the accepted character-as-string
platform; it is recorded as a failing verdict, not skipped. The full suite's
other existing failures/blocked namespaces retain their reviewed baseline.
Full workspace, independent review and passing final-head CI remain separate
acceptance gates.
