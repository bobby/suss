# Printing options

The printer's dynamic options follow Clojure's `clojure/core_print.clj`
(#302). They apply to `pr-str`, `prn-str`, `print-str`, `println-str` and to
`str` of a collection, in both phases and in AOT components. `str` of a
vector, list, map or set prints it as Clojure's `toString` does, under the
current options. `str` of a lazy seq or a queue also prints its items, where
Clojure gives the object's class and hash. Printing adds no import to a component (spec
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

`*print-dup*` prints every value without length or level limits, prints
metadata, and prints strings readably, as Clojure does.

- A record prints by its values, as `#user.R[1, 2]`, which `read-string`
  constructs with the record's positional factory. With
  `*verbose-defrecords*`, it prints as a map, `#user.R{:a 1, :b 2}`.
- Clojure prints some values as `#=(...)` constructor calls: array maps
  (including metadata maps, so Clojure prints `^#=(...) [1]`), sorted
  collections, functions and some boxed numbers. Suss never evaluates while
  reading, so it prints these as `print-method` would. Their output reads
  back to an equal value, but not necessarily of the same concrete type.
- A keyword `:type` is removed from printed metadata, as `print-method` does.
  Clojure's `print-dup` keeps it for records and for the values it prints as
  `#=(...)` forms.

## Values that are not data

Values that are not data print as Clojure's default `print-method` prints them
(#304), with the type names of [type names](type-names.md).

| Value | Prints as |
| --- | --- |
| A type value, such as `(type [])` | `suss.core.PersistentVector` |
| A `deftype` value, a transient, a `reify` | `#object[my_ns.T 0x1f "my_ns.T@1f"]` |
| A reference: an atom or `reduced` value | `#object[suss.core.Atom 0x2 {:status :ready, :val 1}]` |
| An `ex-info` | `#error {:cause "x" :data {} :via [...] :trace []}`, on several lines |

- **Objects:** `#object[`, the type's name, `0x` and the identity hash in
  hexadecimal, then `pr` of the object's `str`. A type's own `toString` gives
  the string, otherwise it is `Name@hex` with the object's `hash`, as Clojure's
  `Object/toString`.
- **References** print their state as Clojure's `deref-as-map` does. The
  state is a map, so `*print-length*` and `*print-level*` apply to it. The
  printer also reports `:pending` and `:failed` states, but no Suss reference
  can be in either yet: `delay`, `future` and `volatile!` do not exist.
- **Errors:** an `ex-info` prints as Clojure's `print-throwable`: the root
  cause's message and data, then one `:via` entry per cause.
- **Metadata** prints before the object under `*print-meta*`, as in Clojure.
- **Reading back:** `read-string` rejects these forms with "No reader
  function for tag object" (or `error`), as Clojure's reader does.

Differences from Clojure:

- The hash after `0x` is Suss's identity hash, a small number assigned in
  order, not the JVM's `identityHashCode`.
- Core types are named `suss.core.X`, not `clojure.lang.X`.
- `#error` has no `:at` entries and `:trace` is always `[]`, because Suss
  keeps no stack traces. Errors other than `ex-info` print as objects.
- Functions still print as `#object[name]`, and `#js` objects and their
  printer helpers are not yet supported.
- Under `*print-dup*`, Clojure prints a type value as `#=name` and has no
  `print-dup` method for other objects, references or errors, so printing them
  throws. Suss prints them as `pr-str` does.
- `str` of an `ex-info` is `Error: x`, as in ClojureScript, where Clojure's is
  `clojure.lang.ExceptionInfo: x {}`.
- Types of numbers, strings, functions and runtime errors are not named yet
  ([type names](type-names.md)), so they print as before.

## `print-method` and `print-dup`

`print-method` and `print-dup` are Clojure's multimethods (#302), built on
Suss's multimethods ([multimethods](multimethods.md)). Each printer prints
each value as Clojure's `pr-on` does: with `print-dup` under `*print-dup*`,
and otherwise with `print-method`, which dispatches on a keyword `:type` in
the value's metadata, or else on its `type`.

```clojure
(deftype Point [x y])
(defmethod print-method Point [p w]
  (.write w "#point ")
  (print-method [(.-x p) (.-y p)] w))
(pr-str [(Point. 1 2)])                           ;=> "[#point [1 2]]"
(binding [*print-length* 1] (pr-str (Point. 1 2))) ;=> "#point [1 ...]"
```

- **Methods** take the value and a writer, and write with `.write`, as with
  a `java.io.Writer`; the writer also has `.append` and `.flush`. `.write`
  takes a string (Suss's characters are strings), not a code point, a
  character array, or an offset and length, which are ignored.
- **Options:** a method reads the options from the dynamic vars, as in
  Clojure. A collection it prints with `print-method` is one level deeper
  and limited by `*print-length*`. A method prints no metadata unless it
  does so itself, for example with `print-simple`.
- **Records and types:** a method for a record's or a `deftype`'s type
  replaces its printer, including under `*print-meta*`.
- **Under `*print-dup*`,** a value prints with its type's `print-dup` method.
  Without one, it prints with `print-method`, as Clojure's `print-dup`
  methods for vectors, maps, sets, seqs, symbols, strings, keywords, numbers,
  booleans and `nil` do. A record without one prints by its values, as
  Clojure's `print-dup` for records does.
- **A keyword `:type`** with no method of its own goes to the `:default`
  method, which, as in Clojure, prints the value without its `:type`. With no
  `:default` method either, printing throws "No method in multimethod
  'print-method' for dispatch value: :t".
- **`print-simple`** prints the value's metadata, as the printer does, and
  then its `str`.
- **`remove-method`** of a type's method restores its built-in printer.
- **`IPrintWithWriter`**, ClojureScript's printing protocol, still works: a
  `deftype` that implements it, or a record extended with it after its
  `defrecord`, prints with it when its type has no `print-method` method. A
  record's inline `IPrintWithWriter` is replaced by the generated printer.
  Printers that call `pr-sequential-writer` or `pr-writer` observe the
  options.
- **Every phase and target:** methods apply in the Runtime and Macro phases
  and in AOT components, and add no import (spec §8).

The built-in printer prints every value that has no method, as Clojure's
methods for its own types and `Object` do. Differences from Clojure:

- Suss's types have no supertypes, so a method applies only to its own type,
  never to a subtype or an interface. Core types have no methods of their
  own in either table: `(methods print-method)` and `(methods print-dup)`
  each hold only a built-in `:default` until a program adds a method. Their
  tables are array maps until they hold more than eight methods, where
  Clojure's are hash maps, so `methods` lists a few methods in the order they
  were added.
- A type's value reaches a user's `:default` method only when a method calls
  `print-method` for it directly; the printers use the built-in printer, as
  Clojure's `Object` method does.
- Clojure's `print-dup` has no `:default` and methods only for its own types,
  so a `deftype` with no `print-dup` method throws under `*print-dup*`. Suss
  prints it with `print-method`, and a `print-dup` method can call
  `print-dup` for any value. A user's `print-dup` `:default` method is used
  only by direct calls, as for `print-method`.
- Suss can't take the type of a function, a type or a `#js` object yet
  ([type names](type-names.md)), so they dispatch to `:default` and the
  printers print them as built in. Clojure dispatches on their classes.
- Removing `:default` leaves core types without a method for direct calls,
  so a method that calls `print-method` for a number then throws "No method";
  in Clojure, its own types keep their methods.
- The REPL displays a result without the printer (#288), so methods affect
  `pr-str` and the other string printers but not the REPL's display.

## Not yet supported

- **Non-numeric limits:** binding `*print-length*` or `*print-level*` to a
  non-number other than nil or false does not throw, as it does in Clojure.
- **`pr`, `prn`, `print`, `println`, `newline`, `flush` and `*out*`** need an
  output stream. Suss has no stream yet: its I/O belongs to the WASI command
  world (#27), and printing must not add an import to every component. They
  wait for #27 (decision on #302) and are recorded as unimplemented in
  `docs/compatibility/reviews.edn`.
  `*flush-on-newline*` exists so code that binds it compiles, and it has no
  effect until these functions do.

## Implementation

`pr-sequential-writer`, `print-meta?`, `pr-writer`, `pr-writer-impl`,
`pr-str*` and `StringBufferWriter` are imported from the pinned
ClojureScript source with patches that follow Clojure's printer
(`docs/compatibility/patches/printing-*.json`). Records print through
`pr-record-writer` in core's appendix
(`docs/compatibility/bootstrap/exact-integers.sus`), which the generated
`defrecord` printer calls; it takes the record's name from its type. The
printers of values that are not data (`pr-tagged-object`, `deref-as-map` and
`pr-throwable`) are there too, with `print-method`, `print-dup`,
`print-simple` and `print-method-for`, which chooses the method `pr-writer`
calls. `*flush-on-newline*` is imported unchanged.

The tests are in `crates/suss-cli/tests/printer_options.rs` and
`crates/suss-cli/tests/object_printing.rs`.
