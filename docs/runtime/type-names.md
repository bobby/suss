# Type names

Each type knows its name at runtime (#304), so the printer and `str` can show
it as Clojure does, with no host import (spec §8).

## Names

A type's name is the class name Clojure gives it: the namespace with `-`
replaced by `_` (`namespace-munge`), a dot, and the type's name munged as
Clojure's `Compiler/munge` does.

| Definition | Name |
| --- | --- |
| `(deftype T [])` in `user` | `user.T` |
| `(defrecord R [])` in `my-ns.core` | `my_ns.core.R` |
| `(deftype a-b? [])` in `user` | `user.a_b_QMARK_` |
| Core's `PersistentVector` | `suss.core.PersistentVector` |
| `ExceptionInfo` | `suss.core.ExceptionInfo` |
| `(reify)` in `my-ns.core` | `my_ns.core$reify__N` |

Core types are named in `suss.core`, where Clojure's are in `clojure.lang`. A
`reify` name includes its analysis site `N`, which, unlike Clojure's, is not
unique across compilation units. Values whose type has no descriptor, such as
numbers, strings and functions, have no name; where one is needed it is
`Object`.

## Runtime representation

The name is a string in slot 3 of the type's descriptor. Type descriptors never
used that slot, so the recursive type group and runtime ABI version 2 are
unchanged. Only descriptor construction writes it:

- `descriptor-new` and its four core variants take the name as a third
  argument, after the field schema and mutable field names. The compiler
  passes it when it lowers `deftype`, `deftype*`, `defrecord*` and `reify`.
- The runtime's own `ExceptionInfo` descriptor carries its name as a constant.
- `class-value-name` returns the name of a type value, or nil for any other
  value. Core reads it as the private `suss.bootstrap/type-name`.

Changing the descriptor constructors' signatures moves compiled artifacts to
format 3 (`0.1.0+portable.3`). A manifest of an earlier format fails before
any initializer runs, and an old fragment would also fail to link.

## Uses

- A type value prints as its name and its `str` is `class Name`, as Clojure's
  `Class` does.
- A `deftype` or `reify` without its own `toString` gets one that returns
  `Name@hex`, as Clojure's `Object/toString`, with Suss's `hash`. Records
  already had one (#285); they now take the name from their type.
- Objects print as `#object[Name 0xhex ...]`
  ([printing](printing.md#values-that-are-not-data)).
- A record prints as `#Name{...}`, and `defrecord` registers the record's
  factories for `read-string` under the same name, so a printed record reads
  back ([reading data](read-string.md)).

## Tests

`crates/suss-cli/tests/object_printing.rs` covers both phases and an AOT
component that imports nothing.
