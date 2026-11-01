# Reading data at runtime

Running programs read text into data with `read-string` in `suss.core` and
`read-string` in `suss.edn`, following `clojure.core/read-string` and
`clojure.edn/read-string` (#300). Both are available in the Runtime and Macro
phases, in source files, the REPL and AOT components. No compiler or host
reader is linked into a component: the reader is Suss code in core.

## `read-string`

`(read-string s)` and `(read-string opts s)` read one form from `s` and ignore
the rest, as Clojure's do. They never evaluate code.

| Option | Behaviour |
| --- | --- |
| `:eof` | The value returned when `s` has no form. Without it, such input fails with "EOF while reading". |
| `:read-cond` | `:allow` reads reader conditionals; `:preserve` returns them as `reader-conditional` values. Without it, a conditional fails with "Conditional read not allowed". |
| `:features` | Keywords enabled besides the platform features `:suss` and `:cljs`. `:default` always matches. |

Code forms read as Clojure's reader reads them:

- `'x` is `(quote x)`, `@x` is `(clojure.core/deref x)`, `~x` and `~@x` are
  `(clojure.core/unquote x)` and `(clojure.core/unquote-splicing x)`, and
  `#'x` is `(var x)`.
- Syntax quote expands at read time, exactly as the compiler expands it: the
  tools.reader shape, symbols resolved against `*ns*` (as
  `docs/runtime/runtime-namespaces.md` describes), and `x#` auto-gensyms. The
  only difference is that the compiler also quotes source positions in a
  template's metadata, and `read-string` has none.
- `::k` and `::alias/k` resolve against `*ns*`: an alias, or else a loaded
  namespace of that name. An export called by the host runs with `*ns*` at its
  root, `suss.core`.
- `#(...)` expands as the compiler expands it, to `(fn* [p1__N# ...] ...)`.
- `#js {...}` reads as the compiler reads it, as a `js-obj` call.
- Tags use `*data-readers*`, then the built-in `#queue`, then
  `*default-data-reader-fn*`; otherwise reading fails with "No reader function
  for tag ...". Tags in unselected conditional branches, and every tag with
  `:read-cond :preserve`, read as `tagged-literal` values.
- A tag whose name contains a dot is a record literal. `#user.R{:a 1}` builds
  the record with its map factory and `#user.R[1 2]` with its positional
  factory, while `*read-eval*` is true. Otherwise reading fails with Clojure's
  "Record construction syntax can only be used when *read-eval* == true".
  `defrecord` registers each record type under the name the printer writes.
- Duplicate map keys and set elements fail with "Duplicate key: ...".
- No position metadata is attached, as Clojure's `read-string` attaches none.

## `suss.edn/read-string`

`(suss.edn/read-string s)` reads one value and returns nil for nil or empty
input. `(suss.edn/read-string opts s)` takes `clojure.edn`'s options:

- `:eof`: without it, input with no value fails;
- `:readers`: a map from tag symbols to functions, tried before `#queue`;
- `:default`: a function of the tag and the value, for any other tag.

It reads data only. Quote, syntax quote, unquote and deref sugar, `#()`, `#'`,
reader conditionals and `::k` fail, as in the tooling EDN contract
(`docs/runtime/tooling-edn.md`). A record literal is an ordinary tag: an error
unless `:readers` or `:default` handles it.

## Errors and bounds

Malformed input throws `ex-info` whose message is the compiler reader's
message, or the EDN parser's in `suss.edn`, with this data:

```clojure
{:type :reader-exception, :ex-kind :reader-error, :offset 7, :line 2, :col 5}
```

- `:offset` is the 0-based UTF-16 offset of the failing form or escape;
  `:line` and `:col` are 1-based.
- `:ex-kind` is `:eof` when the input ended too early, so a caller can ask for
  more input.

Reading is charged to the caller's fuel like any Suss code, so the size of the
input is bounded by the caller's budget. Nesting is bounded at 128 levels of
collections, prefixes, tags and metadata. Deeper input fails with "Reader
nesting exceeds the depth limit" ("EDN nesting exceeds the depth limit" in
`suss.edn`), a catchable error and never a stack overflow. The bound is
`serde_json`'s default; each level uses about 2 KB of Wasm stack (see #313),
so reading at the bound uses about half of Wasmtime's default 512 KiB. Core's
printer, `=` and `hash` are themselves recursive and fail at a few hundred
levels.

## Agreement with the compiler's reader

`crates/suss-reader/tests/reader-conformance.edn` is one corpus for every
reader. Its valid entries cover nil, booleans, numbers in every radix and
their diagnostics, strings and escapes, characters, symbols, keywords,
collections, metadata, comments, discard and `#queue`. Both the compiler's
reader and the EDN parser read them to equal data, and both runtime readers
read them to data equal to the compiler's, metadata included
(`edn_conformance`, `read_string`). Every malformed entry fails in every
reader with the same message at the same position, except `::a`, which
`read-string` resolves against `*ns*`.

## Differences from Clojure

- **Not supported:** `#=` (Suss never evaluates while reading), regular
  expressions `#"..."` and namespaced maps `#:ns{}`, as in the compiler's
  reader. Each fails with "Unsupported reader dispatch".
- **Tags:** `#queue` is built in, as in ClojureScript. `#inst` and `#uuid` are
  not, because Suss has no instant or UUID types yet.
- **Records:** only `defrecord` types are readable. Clojure can construct any
  class with a matching constructor. An unregistered name fails with "No
  record type for tag ...".
- **Data:** characters are one-unit strings and numbers are binary64, so
  ratios, `N` and `M` fail. Duplicate keys are found by Suss's `=`, so
  `#{1 1.0}` is a duplicate.
- **Errors:** errors are `ex-info` values with reader data, not
  `ReaderException`s, and a non-string argument fails with "read-string
  requires a string".
- **Depth:** nesting is bounded, as above; Clojure reads until the JVM stack
  overflows.
- **Lone surrogates:** `suss.edn` accepts a lone surrogate escape such as
  `"\ud800"`, because Suss strings may hold one, as Clojure's may. The tooling
  EDN parser in Rust rejects it.

## Implementation

The reader is original Suss code in `docs/compatibility/bootstrap/
exact-integers.sus`, generated into core. It works on UTF-16 units with a
character class table and builds values with transients. `read-form` and
`reader-items` are the two frames of each nesting level and stay small,
because a Wasm frame holds a slot for every value its function keeps across a
call. Other forms dispatch through tables of handlers indexed by the next unit.

Numbers follow the compiler's reader: decimal through the runtime's correctly
rounded parser, and other radixes exactly. Digits beyond 2^53 in a radix that
is not a power of two are converted exactly to hexadecimal, which the parser
rounds once. A `suss.bootstrap/string-from-unit` primitive builds the
one-unit strings of escapes and characters.

`TaggedLiteral`, `tagged-literal?` and `tagged-literal` are imported from the
pinned ClojureScript source. `ReaderConditional`, `reader-conditional` and
`reader-conditional?`, `*read-eval*`, `*data-readers*` and
`*default-data-reader-fn*` are original, following Clojure.

## Costs

Measured on 2026-11-01 against `main` (bdbdbee), test profile.

| | `main` | With the reader |
| --- | --- | --- |
| Bootstrap `runtime.wasm` | 2,284,678 B | 2,568,855 B (+284 KB, +12.4%) |
| Bootstrap `macro.wasm` | 2,281,594 B | 2,565,365 B (+284 KB, +12.4%) |
| AOT component, one small namespace | 2,571,207 B | 2,865,705 B (+294 KB, +11.5%) |
| AOT component, public records fixture | 2,727,964 B | 3,023,609 B (+296 KB, +10.8%) |

| Core initialization fuel | 1,728,197 | 1,776,048 (+2.8%) |

The reader is about 1,000 lines. Its size is dominated by call sites, each of
which compiles to about 280 bytes (#313).

### Throughput

`cargo run --release -p suss-cli --example read_string_bench -- <roots>`
reads the analyzer-shaped document of suss-reader's `edn_bench`. Release
profile, 2026-11-01:

| Roots | UTF-16 units | `read-string` | `suss.edn/read-string` |
| --- | --- | --- | --- |
| 20 | 0.12 M | 0.67 s (0.185 M units/s) | 0.66 s (0.186 M units/s) |
| 40 | 0.25 M | 1.34 s (0.185 M units/s) | 1.45 s (0.172 M units/s) |
| 80 | 0.49 M | 2.72 s (0.185 M units/s) | 2.68 s (0.187 M units/s) |

Reading is linear in the input. As the measurements on #300 showed, building
values dominates the time: Suss calls and transient collection operations
bound it, as they bound `pr-str`. For reference, `pr-str` of the 20-root
document takes 5.9 s, and its time grows quadratically (#315). The Rust EDN
parser reads 41–45 MB/s in release.
