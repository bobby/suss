# Tooling data as canonical EDN

What the compiler writes about a program is EDN that Suss itself can read
(#299). The format is the model for later tooling output (`suss analyze`, the
persisted core analysis). There is no second encoding such as Transit or JSON.
The implementation is `crates/suss-reader/src/edn.rs`.

## Data model

`suss_reader::edn::Value` has these variants: nil, booleans, binary64 numbers,
strings, keywords, symbols, lists, vectors, maps, sets, tagged values, and a
symbol, collection or tagged value with metadata.

- Characters read as one-character strings, as Suss represents them.
- Strings are Unicode. A lone surrogate escape (`"\ud800"`), which Suss
  language strings allow, is an error here.
- Equality and order are structural and total.
  - Numbers compare by `f64::total_cmp`, so `-0.0` differs from `0`. Every NaN
    is one value, the `##NaN` that prints and reads.
  - Lists and vectors are different values.
  - Metadata is ignored, as Clojure's `=` ignores it, however deeply it is
    nested.
  - Values are ordered by kind: nil, boolean, number, string, keyword, symbol,
    list, vector, map, set, tagged. Within a kind they are ordered by content,
    with names compared as (namespace, name).

## Canonical printing

`edn::print` writes one line. `edn::print_pretty` breaks any collection wider
than 80 columns into one entry per line. Both layouts depend only on the value,
so values that are equal, including their metadata, print byte-identically in
every process.

- **Map and set entries** appear in the value order above. Map entries are
  separated by `, `, as Clojure's `pr` separates them.
- **Numbers:**
  - an integer below 10^16 prints without a fraction;
  - `-0.0` keeps its sign;
  - infinities and NaN print as `##Inf`, `##-Inf` and `##NaN`;
  - anything else prints in Rust's shortest round-trip form, such as `0.1` or
    `1e300`.
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
  - auto-resolved keywords (`::k`) fail as they do in source;
  - nothing is evaluated.

  Clojure's EDN reader reads a leading `'` or `@` as part of a symbol. Suss
  rejects them instead, so code is never mistaken for data.
- **Tags:**
  - a tag is read by its entry in `Options::readers`, if it has one;
  - otherwise the built-in `#queue` applies, which requires a vector;
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
  radix and `##` forms. Ratios and `N`/`M` suffixes fail.
- **Bounds:**
  - input size is charged to `Options::work_allowance`, one unit per byte up
    front and one per form;
  - nesting is bounded by `Options::max_depth` (default 1024);
  - the reader grows its stack, so deep input fails with a diagnostic and never
    overflows the host stack.

  Every error carries a byte span.

### Differences from `clojure.edn`

- `#queue` is a Suss addition, matching the language reader's built-in tag.
  `#inst` and `#uuid` are not built in, because Suss has no instant or UUID
  types yet. A caller can supply readers for them.
- Namespaced maps (`#:ns{...}`) are not supported, as in the compiler's reader.
- Duplicate keys are found by the structural order above, not by Clojure's `=`:
  - `#{1 1.0}` is a duplicate, because every number is a binary64;
  - `#{0.0 -0.0}` and `{[1] :a (1) :b}` are accepted, though Clojure rejects
    both.
- Keyword and symbol tokens follow the compiler's reader. For example, `:a/1`
  is rejected.

`read_one` requires exactly one value. The language-level `read-string` and
`suss.edn/read-string`, with their `:eof` behaviour, belong to #300.

## Rust types

`edn::to_string`, `to_string_pretty`, `to_value`, `from_str` and `from_value`
bridge serde types to EDN:

- structs and maps become maps, and struct fields become keywords. A field
  named `a/b` becomes a namespaced keyword;
- sequences and tuples become vectors;
- `None` and `()` become nil;
- unit variants become keywords. Every other variant becomes a one-entry map
  from its keyword to its content;
- integers must be exact in binary64, so magnitudes above 2^53 are an error.

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
- **Bootstrap manifests** `runtime/bootstrap/runtime.edn` and `macro.edn`, in
  the pretty layout. A JSON or otherwise mismatched manifest is rejected, as any
  identity change is.
- **The `suss.source-artifact` Wasm custom section**, on one line. An artifact
  whose section is JSON fails identity verification. Caches treat that as a
  miss and recompile.

## Performance

`cargo run --release -p suss-reader --example edn_bench -- <roots>` measures
printing and reading of a generated document shaped like analyzer output.
Results for both profiles are recorded on #299. The target is that reading the
persisted core analysis (step 3) costs at most 10% of the analysis it replaces.
