# Printing options

The printer's dynamic options follow Clojure's `clojure/core_print.clj`
(#302). They apply to `pr-str`, `prn-str`, `print-str`, `println-str` and to
`str` of a collection, which prints as Clojure's `toString` does, in both
phases and in AOT components. Printing adds no import to a component (spec
§8).

| Option | Root | Behaviour |
| --- | --- | --- |
| `*print-length*` | nil | Items of each collection before `...`. |
| `*print-level*` | nil | Collection depth before `#`. |
| `*print-meta*` | false | Prints metadata as `^m` before a value. |
| `*print-readably*` | true | Strings and characters print as literals. |
| `*print-namespace-maps*` | false | A map whose keys share a namespace prints as `#:ns{...}`. |
| `*print-dup*` | false | Prints values so they read back as they are. |
| `*verbose-defrecords*` | false | Under `*print-dup*`, records print as maps. |
| `*flush-on-newline*` | true | Declared for Clojure compatibility; see below. |

## Length and level

- **Logical false** for `*print-length*` or `*print-level*` means no limit,
  and `*print-dup*` turns both off.
- **Every collection** counts as one level and limits its own items: vectors,
  lists, seqs, maps, sets, queues, records and `#js` values. A map's entries
  are not levels or items of their own, so
  `(binding [*print-level* 1] (pr-str {:a [1]}))` is `"{:a #}"`.
- **Atoms** such as numbers, strings and keywords print at any level.
- **Records:** as in Clojure, the record's name is written before the level
  check, so a record below the level prints as `#user.R#`.
- **Metadata** prints at the level of the value it belongs to.

## Lazy seqs

Printing realizes a seq only as far as Clojure's printer does. Each step
takes the next element before printing the current one, as Clojure's
`[x & xs]` destructuring does. So under `*print-length*` n, an unchunked seq
is realized to n + 2 elements, and a seq hidden by `*print-level*` is not
realized at all. A chunked seq is realized a chunk at a time, as in Clojure.
An infinite seq prints under either option.

## Metadata

Metadata prints under `*print-dup*`, or under `*print-meta*` when printing
readably, so `print-str` and `println-str` never print it.

- Empty metadata prints nothing.
- Metadata that is only a `:tag` prints as a type hint: `^long x`.
- A keyword `:type` is `print-method`'s dispatch value in Clojure, so it is
  removed before printing: `^{:type :t :a 1} [1]` prints as `^{:a 1} [1]`.
- Output printed with `*print-meta*` reads back with `read-string`
  (`docs/runtime/read-string.md`) to equal values with equal metadata.

## `*print-dup*`

`*print-dup*` prints every value without length or level limits, prints all
metadata, and prints strings readably, as Clojure does.

- A record prints by its values, as `#user.R[1, 2]`, which `read-string`
  constructs with the record's positional factory. With
  `*verbose-defrecords*`, it prints as a map, `#user.R{:a 1, :b 2}`.
- Clojure prints some values as `#=(...)` constructor calls: array maps,
  sorted collections, functions and boxed number types other than `Long` and
  `Double`. Suss never evaluates while reading, so it prints these as
  `print-method` would. Their output reads back to an equal value, but not
  necessarily of the same concrete type.

## Not yet supported

- **`print-method`**, Clojure's extension point for user types, needs
  multimethods (#30). Until then, `deftype` and `defrecord` customize printing
  with `IPrintWithWriter`.
- **`pr`, `prn`, `print`, `println`, `newline`, `flush` and `*out*`** need an
  output stream. Suss has no stream yet: its I/O belongs to the WASI command
  world (#27), and printing must not add an import to every component.
  `*flush-on-newline*` exists so code that binds it compiles, and it has no
  effect until these functions do.

## Implementation

`pr-sequential-writer`, `print-meta?`, `pr-writer-impl` and `pr-str*` are
imported from the pinned ClojureScript source with patches that follow
Clojure's printer (`docs/compatibility/patches/printing-*.json`). Records
print through `pr-record-writer` in core's appendix
(`docs/compatibility/bootstrap/exact-integers.sus`), which the generated
`defrecord` printer calls with the record's name. `*flush-on-newline*` is
imported unchanged.

The tests are in `crates/suss-cli/tests/printer_options.rs`.
