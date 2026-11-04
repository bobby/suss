# Multimethods

Suss's multimethods follow Clojure 1.12's `defmulti`, `defmethod` and `MultiFn`
(#30). `MultiFn` is imported from the pinned ClojureScript source with patches
where Clojure differs; each patch is listed in `docs/compatibility/reviews.edn`
with its rationale. `defmulti` and `defmethod` are Clojure's macros
(`crates/suss-compile/src/portable_core_macros.rs`).

`defmulti`, `defmethod`, `remove-method`, `remove-all-methods`,
`prefer-method`, `methods`, `get-method` and `prefers`.

## Dispatch

A call applies the dispatch function to the arguments and finds a method for
the dispatch value, as `MultiFn.getMethod` does:

1. If the hierarchy is no longer identical to the one cached, the cache is
   reset to the method table.
2. A method cached for the value is used.
3. Otherwise every method whose dispatch value the value `isa?`, in the
   multimethod's hierarchy, is a candidate. A candidate replaces the best so
   far if it dominates it: it is preferred to it, or `isa?` it. If the best
   does not dominate a candidate, the call throws.
4. With no candidate, the method for the default dispatch value is used.
5. The method found is cached for the value.

The method table and preferences are hash maps, as in Clojure, so `methods`
and `prefers` return hash maps and candidates are visited in hash order. That
order names the two dispatch values in the "Multiple methods" error; Suss lays
out hash maps as Clojure does, and the message matches Clojure 1.12.6's.

`defmulti` takes Clojure's docstring, attribute map and `:default` and
`:hierarchy` options. The default hierarchy is the global one; a `:hierarchy`
must be a reference, such as an atom, which is dereferenced on each call. A
second `defmulti` for a var that holds a multimethod does nothing, so its
methods are kept, even inside a local of the same name; a var holding anything
else is redefined.

Two differences from Clojure remain:

- A `:hierarchy` that is not a reference, such as a plain map, is accepted by
  `defmulti` and fails at the first `defmethod` or call. Clojure's constructor
  throws a `ClassCastException`.
- Suss types have no supertypes (`docs/runtime/hierarchies.md`), so a method
  for a type is chosen only for that type, never for a subclass or interface
  as Clojure's `Object` or `Number` methods are. Suss has no `#'`, so a
  `:hierarchy` var is written as an atom.

| | Clojure (followed) | Pinned ClojureScript |
| --- | --- | --- |
| Inherited preferences | `(prefer-method m ::xp ::y)` also prefers `::x`, a child of `::xp` | the parent loops discard their result, so the call is ambiguous |
| Preference parents | the multimethod's hierarchy | the global hierarchy |
| Preferring nil or false | checks membership | looks the value up, so nil and false are never preferred |
| `prefer-method` result | the multimethod | the hierarchy |
| Cache check | identity of the hierarchy | `=` |
| Name in messages | the var's name, `'area'` | the qualified symbol |
| `name` and `namespace` of a multimethod | not supported (`MultiFn` is not `Named`) | supported |
| `dispatch-fn`, `default-dispatch-val` | not in `clojure.core` | functions |

## Errors

Messages are Clojure's. Clojure formats each dispatch value with
`String.format`'s `%s`, which calls `toString`. Suss uses `str`, with `null` for
nil, which is Clojure's `toString` except where `toString` gives an identity
string, such as `clojure.lang.LazySeq@…` for a lazy sequence or a record
without its own `toString`. A value `str` can't format yet is printed instead:
`(type 1)` appears as `#object[Number]` where Clojure says `class
java.lang.Long`, since Suss's host types have no class names (#302).

| Case | Message |
| --- | --- |
| No method and no default | No method in multimethod 'area' for dispatch value: :circle |
| Two candidates, neither preferred | Multiple methods in multimethod 'amb' match dispatch value: :user/c -> :user/b and :user/a, and neither is preferred |
| `prefer-method` against an existing preference | Preference conflict in multimethod 'amb': :user/a is already preferred to :user/b |
| `(defmulti f identity :default)` | The syntax for defmulti has changed. Example: (defmulti name dispatch-fn :default dispatch-value) |
| An option other than `:default` or `:hierarchy` | Only these options are valid: :default, :hierarchy |

## Printing

A multimethod prints as Clojure prints one:
`#object[suss.core.MultiFn 0x… "suss.core.MultiFn@…"]`. Like other references,
the REPL can't yet display one directly as a result (#288). `pr-str` prints it.

## Evidence

- `cargo test -p suss-compile --test core_state multimethod defmulti` covers
  dispatch through hierarchies, vectors and defaults, variadic dispatch, named
  methods, the cache after hierarchy and method changes, inherited and nil or
  false preferences, a `:hierarchy` atom, the global hierarchy after `derive`,
  `defmulti`'s docstring, attribute map and `:default` (including nil),
  redefinition (including under a local of the same name), method removal,
  dispatch on `type` and the error messages.
- `cargo test -p suss-compile --lib defmulti_validation` checks `defmulti`'s
  syntax errors, including an odd number of options.
- Each result and message in those tests was compared with Clojure 1.12.6 on
  the JVM, apart from the host type name and Clojure's REPL printing a
  namespaced map as `#:user{…}`.
- jank's clojure-test-suite has no multimethod namespace. The three that use
  `defmulti` still stop earlier, at `ifn?` or a ratio.
