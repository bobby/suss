# delay and vswap! macros

`delay.sus` and `vswap.sus` are the pinned ClojureScript `cljs/core.cljc` macros
`delay` (line 2293) and `vswap!` (line 3021), commit
c4295f303100bbf5afac449242d30bca1126f1a1, under EPL-1.0
(`runtime/core-import/epl-v10.html`). They are loaded by
`crates/suss-compile/src/portable_state_macros.rs` and registered with the other
bootstrap source macros in `portable_macros.rs` (#30).

Changes from the pinned source:

- `core/defmacro` is written `defmacro`, as for `lazy-seq`.
- `delay` constructs `Delay` with a third argument, `nil`: the patched `Delay`
  type (`docs/compatibility/patches/delay-type.json`) has an added `exception`
  field, so it can cache an exception as Clojure's `Delay` does.

`vswap!` is otherwise unchanged. Like Clojure's, it evaluates the volatile
expression twice.
