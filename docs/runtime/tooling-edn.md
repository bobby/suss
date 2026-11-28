# Tooling data as canonical EDN

What the compiler writes about a program is EDN that Suss itself can read
(#299). The format is the model for later tooling output: `suss analyze`
([analyzer output](analyzer-edn.md)) and the persisted core analysis. There is no second encoding such as Transit or JSON.
The implementation is `crates/suss-reader/src/edn.rs`.

## Data model

`suss_reader::edn::Value` has nil, booleans, doubles, signed 64-bit longs,
BigInt, BigInteger, Ratio, Decimal, strings, UUIDs, keywords, symbols, lists,
vectors, maps, sets, tagged values, and metadata. Decimal holds an arbitrary-
precision unscaled integer and signed 32-bit scale; it preserves trailing zeros.

- Characters read as one-character strings, as Suss represents them.
- Strings are Unicode. A lone surrogate escape (`"\ud800"`), which Suss
  language strings allow, is an error here.
- Equality and order are structural and total.
  - Doubles compare by `f64::total_cmp`, so `-0.0` differs from `0.0`. Every
    NaN is one tooling value, the `##NaN` that prints and reads.
  - Exact numeric categories remain distinct. Decimal structural identity
    includes both coefficient and scale, so `1.0M` and `1.00M` remain different
    tooling values even though Clojure numeric equality considers them equal.
  - Lists and vectors are different values.
  - Metadata is ignored, as Clojure's `=` ignores it, however deeply it is
    nested.
  - Values are ordered by kind: nil, boolean, number, string, keyword, symbol,
    list, vector, map, set, tagged, BigInt, Ratio, BigInteger, UUID, Decimal.
    Within a kind they are ordered by content, with names compared as
    (namespace, name). Decimal order compares coefficient, then scale; this is
    deterministic storage order, not the runtime numeric comparison operation.

## Canonical printing

`edn::print` writes one line. `edn::print_pretty` breaks any collection wider
than 80 columns into one entry per line. Both layouts depend only on the value,
so values that are equal, including their metadata, print byte-identically in
every process.

- **Map and set entries** appear in the value order above. Map entries are
  separated by `, `, as Clojure's `pr` separates them.
- **Numbers:**
  - longs print as integers, BigInts with `N`, normalized non-integral Ratios
    as `numerator/denominator`, and Decimals with `M`;
  - Decimal printing follows Clojure's plain/scientific threshold, retains its
    coefficient and scale, and uses scientific notation at extreme scales
    without allocating billions of padding zeros;
  - BigInteger uses `#suss/biginteger "digits"` so emitted source preserves that
    projection rather than reading the token as Long or BigInt;
  - doubles always include a fraction or exponent, preserving their category
    and signed zero, with `##Inf`, `##-Inf` and `##NaN` for nonfinite values.
- **Strings** escape `"`, `\`, newline, tab, return, backspace and form feed by
  name. Other control characters use `\uXXXX`; everything else prints as is.
- **Metadata** prints as a full map: `^{:k true} x`.
- **Unreadable values** are refused rather than misprinted. These are symbols or
  keywords that no reader token spells (such as `nil`, `1a` or a name with a
  space), tags that do not start with a letter, `#queue` around anything but a
  vector, and metadata on a value that cannot carry it.

Printing a value and reading it back gives an equal value, with equal metadata,
and printing that value again gives the same bytes. A generated property test
checks this, and also checks that output is identical across processes.

## Reading: the `suss.edn` contract

`edn::read_all` and `edn::read_one` follow `clojure.edn`. They parse EDN
directly, without intermediate forms. Number, symbol and delimiter rules are
shared with the compiler's forms reader, and a conformance test checks that both
readers read a shared corpus to equal data. It also checks that they reject
malformed text with the same message and span.

- **No code:**
  - quote, syntax quote, unquote and deref sugar (`'`, `` ` ``, `~`, `@`) are
    errors;
  - so are `#()`, `#'` and reader conditionals, each at its span;
  - auto-resolved keywords (`::k`) are errors, as in Clojure's EDN reader;
  - nothing is evaluated.

  Clojure's EDN reader reads a leading `'` or `@` as part of a symbol. Suss
  rejects them instead, so code is never mistaken for data.
- **Tags:**
  - a tag is read by its entry in `Options::readers`, if it has one;
  - otherwise built-in tags apply: `#queue` requires a vector, `#uuid` a UUID
    string, and `#suss/biginteger` an exact decimal integer string;
  - otherwise `Options::default` applies, if set;
  - otherwise reading fails with Clojure's "No reader function for tag …".

  `#js` is an ordinary tag in EDN. A discarded form is still read, so its tags
  are checked, as `clojure.edn` checks them. `Options::preserving_tags()` keeps
  unknown tags as `Value::Tagged`.
- **Collections:**
  - duplicate map keys and set elements fail with "Duplicate key: …";
  - a map with an odd number of forms fails.
- **Metadata** follows Clojure's shorthand: `^:k` becomes `{:k true}`, and
  `^T` or `^"T"` becomes `{:tag T}`. When prefixes repeat, the outermost wins.
  Metadata on the metadata map itself is dropped.
- **Numbers** follow the compiler's reader, which supports decimal, hex, octal,
  radix, ratios, `N`/`M` suffixes and `##` forms. Numeric categories and Decimal
  scale remain exact through native value/form conversion and source emission.
- **Bounds:**
  - input size is charged to `Options::work_allowance`, one unit per byte up
    front and one per form;
  - nesting is bounded by `Options::max_depth` (default 1024);
  - the reader grows its stack, so deep input fails with a diagnostic and never
    overflows the host stack.

  Every error carries a byte span.

### Differences from `clojure.edn`

- `#queue` and `#suss/biginteger` are Suss additions matching the language
  reader's built-in tags. `#inst` needs a caller-supplied reader.
- Namespaced maps (`#:ns{...}`) are not supported, as in the compiler's reader.
- Duplicate numeric keys use reader identities in addition to structural
  order. Long/BigInt/BigInteger integers coalesce by value, and Decimal keys
  coalesce across scales, including inside nested keys. `#{1 1.0}` and
  `#{1 1M}` retain distinct categories, while `#{1 1N}` and `#{1.0M 1.00M}`
  fail. These rules also prevent printing a manually constructed value with
  duplicate numeric keys. The existing structural differences remain:
  `#{0.0 -0.0}` and `{[1] :a (1) :b}` are accepted, though Clojure rejects both.
- Keyword and symbol tokens follow the compiler's reader. For example, `:a/1`
  is rejected.

`read_one` requires exactly one value. Suss programs read the same format with
`suss.edn/read-string` (#300, `docs/runtime/read-string.md`), which reads one
value and follows `clojure.edn`'s `:eof`. It accepts a lone surrogate escape,
which Suss strings can hold.

## Rust types

`edn::to_string`, `to_string_pretty`, `to_value`, `from_str` and `from_value`
bridge serde types to EDN:

- structs and maps become maps, and struct fields become keywords. A field
  named `a/b` becomes a namespaced keyword;
- sequences and tuples become vectors;
- `None` and `()` become nil;
- unit variants become keywords. Every other variant becomes a one-entry map
  from its keyword to its content;
- signed and unsigned integers retain their exact bits through 128 bits;
- a Ratio or Decimal cannot be projected into a Serde primitive without an
  exact representation. Those conversions fail explicitly; callers can retain
  the typed EDN value instead of silently rounding to a double.

Persisted types use `#[serde(rename_all = "kebab-case")]` and
`deny_unknown_fields`. As in serde_json, `Some(None)` and `None` both print as
nil. When deserializing, keywords, symbols and strings are interchangeable
wherever Rust expects a string, a map key or a variant name.

## Converted formats

Each format carries `:format-version 2`. Version 1 was JSON.

- **Build-cache manifests** (`docs/runtime/build-cache.md`). The format version
  is part of the manifest key, so old JSON entries are never consulted; they
  are misses. An entry that does not parse or has another version is also a
  miss.
- **Bootstrap manifests** `runtime.edn` and `macro.edn`, in the pretty layout,
  generated at build time into suss-compile's `OUT_DIR` (#383). A JSON or otherwise mismatched manifest is rejected, as any
  identity change is.
- **The `suss.source-artifact` Wasm custom section**, on one line. An artifact
  whose section is JSON fails identity verification. Caches treat that as a
  miss and recompile.

## Performance

`cargo run --release -p suss-reader --example edn_bench -- <roots>` measures
printing and reading of a generated document shaped like analyzer output.
Results for both profiles are recorded on #299. The target is that reading the
persisted core analysis (step 3) costs at most 10% of the analysis it replaces.
