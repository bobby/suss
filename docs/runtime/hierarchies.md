# Hierarchies, auto-resolved keywords, doseq and ->>

These follow Clojure 1.12 (#30). The functions are imported from the pinned
ClojureScript source with patches where Clojure differs; each patch is listed in
`docs/compatibility/reviews.edn` with its rationale.

## Auto-resolved keywords

`::name` is a keyword in the current namespace, and `::alias/name` one in the
namespace that `alias` names in the current namespace. The reader keeps them as
`AutoKeyword` forms. After a fragment's `ns` form is applied, and before macros
see the forms, the compiler resolves them, as Clojure's reader does when it reads
each top-level form. They resolve inside quoted forms, metadata and macro
arguments too.

| Case | Result |
| --- | --- |
| `::a` in `app` | `:app/a` |
| `::str/a` after `(:require [lib.str :as str])` | `:lib.str/a` |
| `::nope/a`, with no alias `nope` | Clojure's "Invalid token: ::nope/a" |
| `::`, `:::a`, `::a/` | "Invalid keyword" |

Two differences from Clojure remain:

- A require without `:as` also records the namespace's own name as an alias,
  so `::lib.str/a` resolves in Suss where Clojure, which looks only at aliases,
  fails.
- Aliases come from the fragment's `ns` form. A top-level `require` later in the
  same fragment does not add aliases for the forms before the next fragment.

The EDN readers (`suss.edn`, `clojure.edn/read-string`) reject auto-resolved
keywords, as Clojure's EDN reader does; `read-string` resolves them against
`*ns*` (`docs/runtime/read-string.md`).

## Hierarchies

`make-hierarchy`, `derive`, `underive`, `isa?`, `parents`, `ancestors` and
`descendants`. The global hierarchy is created on first use, as in
ClojureScript; Clojure keeps it in a private var changed with `alter-var-root`,
which is not observable separately.

| | Clojure (followed) | Pinned ClojureScript |
| --- | --- | --- |
| Assert failures | "Assert failed: (namespace parent)" and so on, with Clojure's form text | the same forms with `js-fn?` and `INamed` |
| Ancestor and cycle errors | built with `print-str`: ":a/x already has :a/y as ancestor" | built with `str_`, without spaces |
| Class tags | a type (`deftype`, `defrecord` or core) may be a tag | any JavaScript function may be a tag |
| `descendants` of a class | throws "Can't get descendants of classes" | throws "Can't get descendants of constructors" |
| `descendants` of a protocol | nil | throws, since protocols are functions |
| `isa?` on vectors | compares counts with `=` | compares with `==` |
| `underive` | reads parent entries with `key` and `val` | `first` and `second` |

Suss types have no supertypes, so `parents`, `ancestors` and `isa?` report only
relationships made with `derive`. Clojure also reports a class's superclasses
and interfaces; that part of the suite is skipped, as it is for ClojureScript
(CLJS-3464).

`underive` uses `interleave`, `interpose`, `flatten` (with `tree-seq`) and
`partition`, imported with this work. `partition` follows Clojure: each
partition is realized with `doall`. It steps with `drop` where Clojure uses
`nthrest`, which Suss lacks; the elements are the same.

`ident?`, `simple-ident?`, `qualified-ident?`, `simple-symbol?`,
`qualified-symbol?`, `simple-keyword?` and `qualified-keyword?` are imported too.

## doseq and ->>

`doseq` is Clojure's macro, with `:let`, `:when` and `:while` and destructuring
bindings. Its binding checks throw Clojure's messages, such as "doseq requires a
vector for its binding", without Clojure's "in ns:line" suffix, as `when-let`'s
do. Clojure also walks a chunked seq a chunk at a time; Suss leaves that branch
out. A chunked seq realizes each chunk when it is first seq'd either way, so
results and the order of effects are the same, and the chunk branch would
double the expansion for each binding.

`->>` is Clojure's macro, unchanged. `->` is a compiler intrinsic.

## Evidence

- `cargo test -p suss-compile --test core_state` covers auto-resolved keywords,
  hierarchies built with `derive` and `underive`, Clojure's error messages, the
  global hierarchy, type tags, the sequence helpers, the ident predicates,
  `doseq` (bindings, modifiers, destructuring, chunked realization order) and
  `->>`.
- `cargo test -p suss-compile --lib doseq_validation` checks `doseq`'s messages
  and their `:macro-syntax-check` phase.
- jank's clojure-test-suite: `derive`, `underive`, `parents`, `ancestors`,
  `descendants`, `make-hierarchy`, `interleave`, `interpose`, `doseq` and
  `mapcat` run and match the reviewed observations. Patch 0005 names a core type
  where the hierarchy tests name a host class; one correction records Clojure's
  nil for the descendants of a protocol.
