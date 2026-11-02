# Analyzer output as EDN

`suss analyze` prints a source file's forms, or the compiler's analysis of
them, as EDN in the [tooling format](tooling-edn.md) (#299 step 2). Analysis
follows Clojure's tools.analyzer: each node is a plain map, and a generic
walker reaches every child through `:children`. A Suss program can read the
output with `suss.edn/read-string`.

**The shape is unstable.** It may change in any release until a spec change
promotes it. Each document carries `:suss.analyzer/version`, now 1.

## The command

```
suss analyze [--emit forms|hir] [--pretty] <FILE>
suss analyze [--emit forms|hir] [--pretty] -n <NS> [--src <DIR>]...
```

- **`--emit hir`** (the default) prints the analysis. It runs the ordinary
  compilation pipeline: macros run in the macro session, and required
  namespaces are analyzed too. No Runtime code runs. Only the selected file's
  forms are printed.
- **`--emit forms`** prints the forms as read, without analysis.
- **`-n`** finds the namespace's file on the source paths (default `src`), as
  `suss compile -n` does, and checks that the file declares it.
- **`--pretty`** breaks wide collections into one entry per line.

## Forms

```clojure
{:suss.analyzer/version 1, :suss.analyzer/kind :forms, :file "src/app.sus",
 :forms [(ns app) (defn f [x] (inc x))],
 :suss/source [{:start 0, :end 8, :line 1, :column 1, :end-line 1, :end-column 9,
                :items [...]}, ...]}
```

- **`:forms`** holds each top-level form as the data it reads as. Metadata is
  merged as tools.reader merges it, so `^:a ^{:b 1} x` has `{:a true, :b 1}`.
  A string that is not valid Unicode, such as one with a lone surrogate, is
  `#suss/utf16 [55296 ...]`, its UTF-16 code units.
- **`:suss/source`** holds each form's position, in a tree parallel to the
  form. Positions are not metadata (spec §5), and EDN cannot put metadata on a
  number or keyword, so they are kept apart.
  - `:start` and `:end` are byte offsets.
  - Lines and columns start at 1. Columns count UTF-16 code units, as Clojure's
    reader counts characters.
  - A collection's children are under `:items`, or `:entries` (`[key value]`
    pairs) for a map, in the order the form prints: canonical order for map
    entries and set items. Metadata prefixes are under `:meta`, and a tagged
    literal's form is under `:form`.
- Reader conditionals are already selected, and quote, deref and `#'` are
  lists, as `(quote x)`. Syntax quote is the `(syntax-quote ...)` the reader
  produces; analysis expands it.

## Analysis

```clojure
{:suss.analyzer/version 1, :suss.analyzer/kind :hir, :file "src/app.sus",
 :asts [{:op :suss/ns, :name app, :form (ns app), ...}
        {:op :def, :var app/f, :children [:init], :init {:op :fn, ...}, ...}],
 :global-env {:namespaces {app {:ns app, :aliases {}, :refers {}, :excludes #{},
                                :macro-aliases {}, :defs {f {:var app/f, :meta {...}}}}},
              :macro-namespaces {}}}
```

`:asts` has one node per top-level form, in order. `:global-env` is the state
of each namespace the file entered, after its last form, as tools.analyzer
keeps namespaces in a separate global environment. Definitions made by
`defmacro` belong to the macro phase, under `:macro-namespaces`.

### Nodes

Every node has:

- `:op`, a keyword;
- `:form`, the form analyzed, after macro expansion;
- `:env`, `{:ns ns, :phase :runtime|:macro, :context :ctx/statement|:ctx/expr|:ctx/return, :locals {name id}}`;
- `:children`, the keys of its child nodes in evaluation order, each holding a
  node or a vector of nodes;
- `:suss/source`, its position (below);
- `:tag`, when the analyzer infers one.

| `:op` | Keys | Form |
| --- | --- | --- |
| `:const` | `:val`, `:type` (`:nil`, `:bool`, `:number`, `:string`, `:keyword`) | `1`, `"s"`, `:k` |
| `:quote` | `:expr`, a `:const` with `:literal? true` | `(quote x)` |
| `:vector`, `:set` | `:items` | `[a b]`, `#{a}` |
| `:map` | `:keys`, `:vals` | `{k v}` |
| `:with-meta` | `:meta`, `:expr` | `^{:m 1} [x]` |
| `:local` | `:name`, `:id`, `:local` (`:let`, `:loop`, `:arg`, `:fn`, `:catch`, `:field`), `:arg-id` | `x` |
| `:var` | `:var`, a qualified symbol | `inc`, `app/f` |
| `:js-var` | `:var` | `js/Error` |
| `:do` | `:statements`, `:ret`, and `:body? true` for a body the analyzer made | `(do a b)` |
| `:if` | `:test`, `:then`, `:else` | `(if a b c)` |
| `:let`, `:loop` | `:bindings` (`:binding` nodes with `:init`), `:body`; a loop also has `:loop-id` | `(let* [x 1] x)` |
| `:recur` | `:exprs`, `:loop-id` of its target | `(recur x)` |
| `:fn` | `:methods` (`:fn-method` nodes with `:params`, `:body`, `:variadic?`, `:fixed-arity`, `:loop-id`), `:local` for a named fn, `:variadic?`, `:max-fixed-arity` | `(fn* f ([x] x))` |
| `:def` | `:var`, `:name`, `:meta`, `:doc`, `:init`, `:suss/once` | `(def x 1)` |
| `:set!` | `:target`, `:val` | `(set! x 1)` |
| `:invoke` | `:fn`, `:args` | `(f x)` |
| `:new` | `:class`, `:args` | `(new T x)` |
| `:throw` | `:exception` | `(throw e)` |
| `:try` | `:body`, `:catches` (`:catch` nodes with `:class`, `:local`, `:body`), `:finally` | `(try a (catch T e b) (finally c))` |
| `:suss/this-as` | `:local`, `:body` | `(this-as t body)` |
| `:suss/future`, `:suss/await` | `:body`, `:value` | `(future* ...)`, `(await* x)` |
| `:suss/ns` | `:name` | `(ns app ...)` |
| `:suss/intrinsic` | `:intrinsic`, the head symbol; `:args` | `(when a b)`, `(inc x)` |

- **Locals and loops** are numbered from 0 in walk order, per document, so the
  numbers do not depend on the compiler's counters. `:env :locals` maps each
  visible name to its number, and a `:local` refers to its `:binding` by
  `:id`. A `:recur`'s `:loop-id` is the innermost `:loop` or `:fn-method`.
- **Intrinsics.** Suss analyzes some macros itself instead of expanding them
  to special forms: `when`, `and`, `or`, `cond`, `case`, `inc`, `<`,
  `binding`, `deftype`, `defrecord`, `reify`, `extend-type`, `instance?`,
  interop such as `(.-x o)`, and others. They are `:suss/intrinsic` nodes. Their
  `:args` are the analyzed forms found inside them, in order.
- **Syntax quote** expands to calls of `sequence`, `seq`, `concat` and
  `list`, which are ordinary `:invoke` nodes.

### Positions

`:suss/source` has `:start`, `:end` (byte offsets), `:line`, `:column`,
`:end-line` and `:end-column`. A macro's expansion has the span of the macro
call, but keeps the reader's position metadata on its forms, as Clojure's do.
For a node whose form has `:line` metadata, the position is that metadata,
without byte offsets.

## emit-form

`suss_compile::portable::analyzer_edn::emit_form` turns a node back into a
form, as tools.analyzer's `emit-form` does. It works on the EDN data, so a pass
can edit nodes and emit the result.

- Special forms are emitted bare (`let*`, `fn*`, `if`), and vars fully
  qualified. A body `do` contributes its forms to its parent.
- A macro-phase `def` of a function of `&form`, `&env` and parameters is
  emitted as the `defmacro` that made it.
- An intrinsic whose `:args` are its operands in order, such as `(inc x)`, is
  emitted from them. Other intrinsics are emitted as their `:form`, so a pass
  cannot yet change what is inside them.

Analyzing the emitted forms in the same environment gives the same nodes,
apart from `:form`, positions, generated names (`G__12`, `x__3__auto__`) and
the order of map and set entries, which `:form` data does not keep. The tests
check this for the shipped core in both phases, the compiler's conformance
corpus (`reference/cljs-tests`) and the samples.

## Differences from tools.analyzer

- Intrinsics, above, stand for forms that Clojure expands to special forms.
- `:env` holds names and numbers, not binding nodes, and no `:file` or
  `:line`; positions are on the node.
- A var is a symbol, not a Var. A `:catch`'s `:class` is a `:const` of
  `:type :class` holding the class symbol, or `:default`.
- There are no `:raw-forms`, `:top-level` or `:o-tag` keys, and no
  `:host-call` or `:instance-field` ops, since interop is intrinsic.

## Implementation

- `crates/suss-reader/src/edn/source.rs`: forms as values, positions and the
  form printer.
- `crates/suss-compile/src/portable/analysis_record.rs`: records each
  fragment's analyzed forms while `suss analyze` runs the pipeline.
- `crates/suss-compile/src/portable/analyzer_edn.rs`: the projection,
  `emit_form` and the comparison.
- `crates/suss-compile/src/portable_analyze.rs`: the documents and the command.
- Tests: `crates/suss-cli/tests/analyzer_edn.rs` and
  `crates/suss-compile/tests/analyzer_edn_core.rs`.

Nodes are projected from the analyzer's source facts, which keep each special
form's operands before lowering, so the output is independent of how
the compiler lowers forms to Wasm. The shipped core's analysis has 672
top-level nodes and prints as 13 MB of EDN on one line.
